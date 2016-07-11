//! The little finger daemon.
//!
//! This daemon is an example for the netmachines crate. It also implements
//! a modified version of the finger protocol described most recently in
//! [RFC 1288]. Unlike real finger, it receives its information from a text
//! file instead of using real system information (which could be considered
//! irresponsible). In addition to real finger, it also accepts TLS
//! connection and responds to requests sent over UDP. Thus, the code herein
//! demonstrates how one would build a server using netmachines.
//!
//! # Server Architecture
//!
//! In order to demonstrate some inter-thread communication, the architecture
//! is a little more involved than strictly necessary. There are two threads:
//! the networking thread which runs the rotor loop, and the query thread
//! which processes queries, creates answers, and hands them back to the
//! requestor.
//!
//! # Pinky Information Files
//!
//! This daemon uses a text file as the source for the information returned
//! to queries. The file defines both the ‘users’ known to the daemon as well
//! as the information printed for them. It consists of sections, one section
//! per user. Sections are separated by lines consisting only of three hash
//! signs: `###` and optional trailing white space. Each section starts with
//! the name of the user as the first line. The first white space-separated
//! word of that line is the username, the rest is the full name of the user.
//! All following lines are treated as the user’s information.
//!
//! [RFC 1288]: https://tools.ietf.org/html/rfc1288

#[macro_use] extern crate log;
extern crate argparse;
extern crate bytes;
extern crate simplelog;
extern crate netmachines;
extern crate rotor;

#[cfg(feature = "openssl")]
extern crate openssl;

use std::cmp::max;
use std::collections::BTreeMap;
use std::fs::File;
use std::io;
use std::mem;
use std::net::{IpAddr, SocketAddr};
use std::ops::DerefMut;
use std::str::FromStr;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use bytes::{Buf, ByteBuf};
use netmachines::handlers::{AcceptHandler, Notifier, TransportHandler};
use netmachines::next::Next;
use netmachines::sockets::{Dgram, Stream};
use netmachines::sync::{Receiver, Sender, channel};
use rotor::mio::tcp::TcpListener;
use rotor::mio::udp::UdpSocket;
use simplelog::{TermLogger, LogLevelFilter};

#[cfg(feature = "openssl")]
use netmachines::net::openssl::TlsTcpUdpServer;

#[cfg(not(feature = "openssl"))]
use netmachines::net::clear::TcpUdpServer;


//============ Main: Start Here ==============================================

// XXX All of this here is preliminary (hence those unwrap() calls). We’ll
//     change all of this when moving to proper config options.

fn main() {
    TermLogger::new(LogLevelFilter::Trace);

    let config = Config::from_args();
    let info = UserInfo::from_config(&config).unwrap();
    let (processor, tx) = Processor::new(info);
    let join = thread::spawn(move || processor.run());

    let addr = SocketAddr::new(IpAddr::from_str("0.0.0.0").unwrap(), 8079);
    let tcp = TcpListener::bind(&addr).unwrap();
    let udp = UdpSocket::bound(&addr).unwrap();

    let mut lc = rotor::Loop::new(&rotor::Config::new()).unwrap();
    lc.add_machine_with(|scope| {
        FingerServer::new_tcp(tcp, StreamAccept::new(tx.clone()), scope)
    }).unwrap();

    lc.add_machine_with(|scope| {
        FingerServer::new_udp(udp, tx.clone(), scope)
    }).unwrap();

    add_tls_sockets(&config, &tx, &mut lc);

    println!("Setting up done.");
    lc.run(()).unwrap();
    drop(tx);
    join.join().unwrap();
}


#[cfg(feature = "openssl")]
fn add_tls_sockets(__config: &Config, tx: &RequestSender,
                   lc: &mut rotor::Loop<FingerServer>) {
    use openssl::x509::X509Generator;
    use openssl::crypto::hash::Type;
    use openssl::ssl::{SslContext, SslMethod};
    use netmachines::sockets::openssl::TlsListener;

    let gen = X509Generator::new()
            .set_bitlength(2048)
            .set_valid_period(7)
            .add_name("CN".to_owned(), "Pinky Daemon Corp.".to_owned())
            .set_sign_hash(Type::SHA256);
    let (cert, pkey) = gen.generate().unwrap();

    let mut ctx = SslContext::new(SslMethod::Tlsv1).unwrap();
    ctx.set_private_key(&pkey).unwrap();
    ctx.set_certificate(&cert).unwrap();
   
    let addr = SocketAddr::new(IpAddr::from_str("0.0.0.0").unwrap(), 8479);
    let tls = TlsListener::bind(&addr, ctx).unwrap();

    lc.add_machine_with(|scope| {
        FingerServer::new_tls(tls, StreamAccept::new(tx.clone()), scope)
    }).unwrap();
}


#[cfg(not(feature = "openssl"))]
fn add_tls_sockets(_config: &Config, _lc: &mut rotor::Loop<FingerServer>) {
}


//============ Network Handlers ==============================================

//------------ FingerServer --------------------------------------------------

#[cfg(feature = "openssl")]
type FingerServer = TlsTcpUdpServer<(), StreamAccept, StreamAccept, 
                                    DgramHandler>;

#[cfg(not(feature = "openssl"))]
type FingerServer = TcpUdpServer<(), StreamAccept, DgramHandler>;


//------------ StreamAccept --------------------------------------------------

#[derive(Clone)]
struct StreamAccept {
    req_tx: RequestSender
}

impl StreamAccept {
    fn new(req_tx: RequestSender) -> Self {
        StreamAccept { req_tx: req_tx }
    }
}

impl<T: Stream> AcceptHandler<T> for StreamAccept {
    type Output = StreamHandler;

    fn on_accept(&mut self, _addr: &SocketAddr) -> Option<RequestSender> {
        Some(self.req_tx.clone())
    }
}


//------------ StreamHandler -------------------------------------------------

enum StreamHandler {
    Request(StreamRequest),
    Await(StreamAwait),
    Response(StreamResponse),
}

impl<T: Stream> TransportHandler<T> for StreamHandler {
    type Seed = RequestSender;

    fn on_create(seed: Self::Seed, _sock: &mut T, notifier: Notifier)
                 -> Next<Self> {
        StreamRequest::new(seed, notifier)
    }

    fn on_read(self, sock: &mut T) -> Next<Self> {
        match self {
            StreamHandler::Request(req) => req.on_read(sock),
            val @ StreamHandler::Await(_) => Next::wait(val),
            val @ StreamHandler::Response(_) => Next::write(val),
        }
    }

    fn on_write(self, sock: &mut T) -> Next<Self> {
        match self {
            val @ StreamHandler::Request(_) => Next::read(val),
            val @ StreamHandler::Await(_) => Next::wait(val),
            StreamHandler::Response(res) => res.on_write(sock)
        }
    }

    fn on_notify(self) -> Next<Self> {
        match self {
            val @ StreamHandler::Request(_) => Next::read(val),
            StreamHandler::Await(await) => await.on_notify(),
            val @ StreamHandler::Response(_) => Next::write(val),
        }
    }
}


//--- StreamRequest

struct StreamRequest {
    sender: RequestSender,
    notifier: Notifier,
    buf: Vec<u8>
}

impl StreamRequest {
    fn new(sender: RequestSender, notifier: Notifier) -> Next<StreamHandler> {
        Next::read(
            StreamHandler::Request(
                StreamRequest { sender: sender, notifier: notifier,
                                buf: Vec::with_capacity(80) }
            )
        )
    }

    fn on_read<T: Stream>(mut self, sock: &mut T) -> Next<StreamHandler> {
        // XXX This is probably not the smartest way to do this, but what
        //     the hell ...
        let mut buf = [0u8; 80];
        match sock.try_read(&mut buf) {
            Ok(Some(len)) => self.buf.extend(&buf[..len]),
            Ok(None) => return Next::read(StreamHandler::Request(self)),
            Err(_) => return Next::remove()
        }
        match Request::parse(&self.buf) {
            Ok(Some(request)) => self.progress(request),
            Ok(None) => {
                if self.buf.len() > 1024 {
                    StreamResponse::new(b"Please stop typing!\r\n")
                }
                else {
                    Next::read(StreamHandler::Request(self))
                }
            }
            Err(err) => StreamResponse::new(err.as_bytes())
        }
    }

    fn progress(self, request: Request) -> Next<StreamHandler> {
        let store = Arc::new(Mutex::new(None));
        let ret = Return::Stream(store.clone(), self.notifier);

        if let Err(_) = self.sender.send((request, ret)) {
            return StreamResponse::new(b"Service temporarily kaputt.\r\n");
        }

        StreamAwait::new(store)
    }
}


//--- StreamAwait

struct StreamAwait {
    store: Arc<Mutex<Option<String>>>
}

impl StreamAwait {
    fn new(store: Arc<Mutex<Option<String>>>) -> Next<StreamHandler> {
        Next::wait(
            StreamHandler::Await(
                StreamAwait { store: store }
            )
        )
    }

    fn on_notify(self) -> Next<StreamHandler> {
        if let Ok(mut lock) = self.store.lock() {
            if let Some(response) = mem::replace(lock.deref_mut(), None) {
                return StreamResponse::new(response.as_bytes())
            }
        }
        else {
             return StreamResponse::new(b"Internal server error.\r\n")
        }
        Next::wait(StreamHandler::Await(self))
    }
}


//--- StreamResponse

struct StreamResponse {
    buf: ByteBuf
}

impl StreamResponse {
    fn new(bytes: &[u8]) -> Next<StreamHandler> {
        Next::write(
            StreamHandler::Response(
                StreamResponse { buf: ByteBuf::from_slice(bytes) }
            )
        )
    }

    fn on_write<T: Stream>(mut self, sock: &mut T) -> Next<StreamHandler> {
        if self.buf.has_remaining() {
            match sock.try_write(self.buf.bytes()) {
                Ok(Some(len)) => self.buf.advance(len),
                Ok(None) => { },
                Err(_) => return Next::remove()
            }
        }
        if self.buf.has_remaining() {
            Next::write(StreamHandler::Response(self))
        }
        else {
            Next::remove()
        }
    }
}


//------------ DgramHandler --------------------------------------------------

struct DgramHandler {
    req_tx: RequestSender,
    tx: Sender<(String, SocketAddr)>,
    rx: Receiver<(String, SocketAddr)>,
    send: Option<(String, SocketAddr)>,
}

impl DgramHandler {
    fn next(self) -> Next<Self> {
        if self.send.is_some() {
            Next::read_and_write(self)
        }
        else {
            Next::read(self)
        }
    }
}

impl<T: Dgram> TransportHandler<T> for DgramHandler {
    type Seed = RequestSender;

    fn on_create(seed: Self::Seed, _sock: &mut T, notifier: Notifier)
                 -> Next<Self> {
        let (tx, rx) = channel(notifier);
        Next::read(DgramHandler { req_tx: seed, tx: tx, rx: rx, send: None })
    }

    fn on_read(self, sock: &mut T) -> Next<Self> {
        let mut buf = [0u8; 4096];
        let (len, addr) = match sock.recv_from(&mut buf) {
            Ok(None) => return self.next(),
            Err(_) => return Next::remove(),
            Ok(Some((len, addr))) => (len, addr)
        };

        let buf = &buf[..len];
        match Request::parse(buf) {
            Err(err) => { self.tx.send((err.into(), addr)).ok(); },
            Ok(None) => {
                self.tx.send(("Broken request.".into(), addr)).ok();
            }
            Ok(Some(request)) => {
                let ret = Return::Dgram(self.tx.clone(), addr);
                if self.req_tx.send((request, ret)).is_err() {
                    self.tx.send(("Server failure.".into(), addr)).ok();
                }
            }
        }
        self.next()
    }

    fn on_write(mut self, sock: &mut T) -> Next<Self> {
        if let Some((message, addr)) = mem::replace(&mut self.send, None) {
            match sock.send_to(message.as_bytes(), &addr) {
                Ok(Some(_)) => { }
                Ok(None) => self.send = Some((message, addr)),
                Err(_) => return Next::remove()
            }
        }
        while let Ok((message, addr)) = self.rx.try_recv() {
            match sock.send_to(message.as_bytes(), &addr) {
                Ok(Some(_)) => { }
                Ok(None) => {
                    self.send = Some((message, addr));
                    break;
                }
                Err(_) => return Next::remove()
            }
        }
        self.next()
    }

    fn on_notify(mut self) -> Next<Self> {
        if let Ok((message, addr)) = self.rx.try_recv() {
            self.send = Some((message, addr));
        }
        self.next()
    }
}

//============ Processing ====================================================

//------------ Request -------------------------------------------------------

/// A finger request.
///
/// Since we don’t support finger forwarding, there is only four relevant
/// forms:
/// 
/// ```text
/// <CRLF>
/// "/W" <CRLF>
/// username <CRLF>
/// "/W" username <CRLF>
/// ```
///
/// In other words, there is an optional `/W` (for ‘whois’) and an optional
/// username.
struct Request {
    whois: bool,
    user: Option<String>
}


impl Request {
    fn parse(data: &[u8]) -> Result<Option<Self>, &'static str> {
        // If there is no "\r\n" in line, we need more data.
        let mut line = match data.split(|ch| *ch == b'\n')
                                 .next().map(|line| line.split_last()) {
            Some(Some((&b'\r', line))) => line,
            _ => return Ok(None)
        };

        // Get on optional starting "/W" followed by spaces.
        let whois = line.len() > 2 && line[0] == b'/' &&
                    (line[1] == b'W' || line[1] == b'w');
        let user = if line.is_empty() { None }
        else {
            if whois {
                line = &line[2..];
                while !line.is_empty() && line[0] == b' ' {
                    line = &line[1..];
                }
            }

            // If there is any space left, there is a syntax error.
            // (Yes, we are not being particularly liberal here.)
            if line.contains(&b' ') {
                return Err("I don't understand your request.\r\n")
            }

            // If there is any ‘@’ signs, the request is for finger relaying
            // and we most certainly won’t do that.
            if line.contains(&b'@') {
                return Err("Relaying disabled.\r\n")
            }

            // Now the line should be one username. 
            match String::from_utf8(line.into()) {
                Ok(user) => Some(user),
                Err(_) => return Err("No such user.\r\n")
            }
        };

        Ok(Some(Request { whois: whois, user: user }))
    }
}


//------------ Return -------------------------------------------------------

enum Return {
    Dgram(Sender<(String, SocketAddr)>, SocketAddr),
    Stream(Arc<Mutex<Option<String>>>, Notifier)
}

impl Return {
    fn send(self, response: String) {
        match self {
            Return::Dgram(tx, addr) => {
                let _ = tx.send((response, addr));
            }
            Return::Stream(store, notifier) => {
                if let Ok(mut guard) = store.lock() {
                    mem::replace(guard.deref_mut(), Some(response));
                    drop(guard);
                    notifier.wakeup().ok();
                }
            }
        }
    }
}


//------------ RequestSender -------------------------------------------------

type RequestSender = mpsc::Sender<(Request, Return)>;


//------------ Processor -----------------------------------------------------

struct Processor {
    info: UserInfo,
    tasks: mpsc::Receiver<(Request, Return)>
}

impl Processor {
    fn new(info: UserInfo) -> (Self, RequestSender) {
        let (tx, rx) = mpsc::channel();
        (Processor { info: info, tasks: rx }, tx)
    }

    fn run(self) {
        while let Ok((request, ret)) = self.tasks.recv() {
            let _ = request.whois; // We don’t actually support whois. Haha.
            ret.send(match request.user {
                Some(user) => {
                    match self.info.user_info(&user) {
                        Some(res) => res,
                        None => "No such user.\r\n".into()
                    }
                }
                None => self.info.user_list()
            });
        }
    }
}

//============ Configuration and User Information ============================

//------------ Config --------------------------------------------------------

/// The configuration.
struct Config {
    info_path: Option<String>,
}

impl Config {
    /// Creates a new default configuration.
    fn new() -> Self {
        Config {
            info_path: None,
        }
    }

    /// Creates a config from the command line arguments.
    fn from_args() -> Self {
        let mut res = Config::new();
        res.parse_args();
        res
    }
    
    fn parse_args(&mut self) {
        use argparse::{ArgumentParser, StoreOption};

        let mut parser = ArgumentParser::new();

        parser.refer(&mut self.info_path)
              .add_option(&["-f", "--info-file"], StoreOption,
                          "path to the information file");

        parser.parse_args_or_exit();
    }
}


//------------ UserInfo ------------------------------------------------------

/// Collection of all the user information.
struct UserInfo { 
    map: BTreeMap<String, (Option<String>, String)>,
    max_user_len: usize
}

impl UserInfo {
    fn new() -> Self {
        UserInfo { map: BTreeMap::new(), max_user_len: 0 }
    }

    fn from_config(config: &Config) -> io::Result<Self> {
        let mut build = InfoBuilder::new();
        if let Some(ref path) = config.info_path {
            try!(build.add_file(path))
        }
        else { 
            try!(build.add_str(DEFAULT_INFO))
        }
        Ok(build.done())
    }

    fn user_info(&self, user: &str) -> Option<String> {
        self.map.get(user).map(|res| res.1.clone())
    }

    fn user_list(&self) -> String {
        let mut res = String::new();
        for (key, value) in self.map.iter() {
            res.push_str(key);
            for _ in key.len()..self.max_user_len {
                res.push(' ');
            }
            if let Some(ref full) = value.0 {
                res.push_str("    ");
                res.push_str(full);
            }
            res.push_str("\r\n");
        }
        res
    }
}


const DEFAULT_INFO: &'static str = "
pinky The Pinky Daemon
At your service!
###
thebrain The Brain
Taking over the world.
";


//------------ InfoBuilder ---------------------------------------------------

/// Builder for user information.
struct InfoBuilder {
    /// Where the parsed out user info goes.
    target: UserInfo,

    /// Parsing state.
    ///
    /// First string is the user name, second string is the user information
    /// collected so far. If this is `None`, we are either at the start or
    /// right after a `###` line.
    state: Option<(String, Option<String>, String)>
}

impl InfoBuilder {
    fn new() -> InfoBuilder {
        InfoBuilder { target: UserInfo::new(), state: None }
    }

    fn add_user(&mut self, user: String, full: Option<String>, info: String) {
        self.target.max_user_len = max(self.target.max_user_len, user.len());
        self.target.map.insert(user, (full, info));
    }

    fn add_str(&mut self, s: &str) -> io::Result<()> {
        for line in s.lines() {
            try!(self.add_line(line))
        }
        self.adding_done();
        Ok(())
    }

    fn done(self) -> UserInfo {
        self.target
    }

    fn add_file(&mut self, path: &str) -> io::Result<()> {
        use std::io::BufRead;

        let file = try!(File::open(path));
        let reader = io::BufReader::new(file);
        for line in reader.lines() {
            try!(self.add_line(&try!(line)));
        }
        self.adding_done();
        Ok(())
    }
}

/// # Steps for Adding
impl InfoBuilder {
    fn add_line(&mut self, line: &str) -> io::Result<()> {
        let line = line.trim_right();
        if line == "###" {
            let state = mem::replace(&mut self.state, None);
            if let Some((user, full, info)) = state {
                self.add_user(user, full, info);
            }
        }
        else {
            if let Some((_, _, ref mut info)) = self.state {
                info.push_str(line);
                info.push_str("\r\n");
            } else if line.trim() != "" {
                let line = line.trim();
                let user = line.split_whitespace().next().unwrap();
                let full = match line[user.len()..].trim() {
                    "" => None,
                    val => Some(val.into())
                };
                self.state = Some((user.into(), full, String::new()));
            }
        }
        Ok(())
    }

    fn adding_done(&mut self) {
        let state = mem::replace(&mut self.state, None);
        if let Some((user, full, info)) = state {
            self.add_user(user, full, info);
        }
    }
}

