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
use std::str::FromStr;
use std::thread;
use bytes::{Buf, ByteBuf};
use netmachines::error::Error;
use netmachines::handlers::{AcceptHandler, TransportHandler};
use netmachines::next::Next;
use netmachines::sockets::{Dgram, Stream};
use netmachines::sync::{DuctReceiver, DuctSender, GateReceiver, GateSender,
                        Receiver, Sender, channel, duct, gate};
use rotor::Notifier;
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
    TermLogger::init(LogLevelFilter::Debug).unwrap();

    // Create the processor and spawn it off into its own thread.
    let config = Config::from_args();
    let info = UserInfo::from_config(&config).unwrap();
    let (processor, tx) = Processor::new(info);
    let join = thread::spawn(move || processor.run());

    // Create the actual sockets.
    let addr = SocketAddr::new(IpAddr::from_str("0.0.0.0").unwrap(), 8079);
    let tcp = TcpListener::bind(&addr).unwrap();
    let udp = UdpSocket::bound(&addr).unwrap();

    // Create a rotor loop with default config.
    let mut lc = rotor::Loop::new(&rotor::Config::new()).unwrap();

    // When creating a machine, you need the scope. But for that the
    // underyling rotor bits need to be there, hence the closure.
    //
    // The FingerServer has a new function for each of the machine types it
    // supports. First we create the TCP server machine. It wants a value
    // of the accept handler, so we create one.
    lc.add_machine_with(|scope| {
        // XXX Do something with the trigger.
        FingerServer::new_tcp(tcp, StreamAccept::new(tx.clone()), scope).0
    }).unwrap();

    // ... and the UDP socket. This one needs a value of the seed for the
    // transport handler it uses (which will be created in the usual way
    // via its create() functions). See the StreamAccept type below for
    // a discussion of transport seeds.
    lc.add_machine_with(|scope| {
        FingerServer::new_udp(udp, tx.clone(), scope)
    }).unwrap();

    // We only do TLS if netmachines has been built with a TLS implementation.
    // The cfg attributes only work on item level, so we have to have a
    // separate function for it.
    add_tls_sockets(&config, &tx, &mut lc);

    info!("Setting up done.");
    lc.run(()).unwrap();

    // Cleanup. Since we currently don’t handle signals, we probably never
    // will arrive here.
    drop(tx);
    join.join().unwrap();
}


/// Creates the TLS socket if OpenSSL is included.
///
/// Creates a self-signed certificate on the fly.
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
        // XXX Do something with the trigger.
        FingerServer::new_tls(tls, StreamAccept::new(tx.clone()), scope).0
    }).unwrap();
}


/// Creates the TLS socket if there is no TLS.
///
/// Ie., it doesn’t.
#[cfg(not(feature = "openssl"))]
fn add_tls_sockets(_config: &Config, _lc: &mut rotor::Loop<FingerServer>) {
}


//============ Networking ====================================================

//------------ FingerServer --------------------------------------------------
// 
// The actual networking state machine is a combination of the state machines
// for the different transport protocols we support. Luckily, netmachines
// provides these for the common combinations, so a type alias is good enough.
//
// However, the type differs on whether we have a TLS library or not. So,
// cfg attributes it is.

/// The server type if OpenSSL is enabled.
///
/// This is a three-way combination of TLS, TCP, and UDP.
///
/// The first type argument is the rotor context type. A value of this type
/// will be passed into the `run()` call of rotor loop and will then be
/// available to all machines. We don’t actually need the context, so we use
/// `()`.
///
/// The next three type arguments are the handler types for TLS, TCP, and
/// UDP, respectively. TLS and TCP want the accept handler that gets called
/// whenever a new connection arrives. UDP wants a transport handler right
/// away since there are no connections.
#[cfg(feature = "openssl")]
type FingerServer = TlsTcpUdpServer<(), StreamAccept, StreamAccept, 
                                    DgramHandler>;

/// The server type if there is no TLS.
///
/// This is just a two-way combination of TCP and UDP.
#[cfg(not(feature = "openssl"))]
type FingerServer = TcpUdpServer<(), StreamAccept, DgramHandler>;


//------------ StreamAccept --------------------------------------------------

/// The accept handler for stream sockets.
///
/// This type stores all information that needs to be passed to each and
/// every stream transport handler which, in our case, is the sending end
/// of the request queue.
#[derive(Clone)]
struct StreamAccept {
    req_tx: RequestSender
}

impl StreamAccept {
    /// Creates a new accept handler value.
    fn new(req_tx: RequestSender) -> Self {
        StreamAccept { req_tx: req_tx }
    }
}

/// Implementation of the `AcceptHandler` trait.
///
/// This trait is used by server machines when accepting incoming
/// connections.
///
/// This happens in two stages. First, the accept handler’s `accept()`
/// method is called. If it returns `None`, the connection is closed again
/// right away. Otherwise it returns the seed for its transport handler.
/// This seed is then passed, together with some more information, to the
/// transport handler’s `create()` function which creates the actual
/// transport handler.
///
/// The reason for this somewhat complex approach is that the underlying
/// rotor machine hasn’t been created yet when `accept()` is called.
/// In particular, this means that the notifier for waking up that machine
/// isn’t available yet. This notifier, however, is necessary in many cases
/// and transport handler types often want to store it. If you would want to
/// do that when the transport handler is created in `accept()` already,
/// you’d have to use an `Option<Notifier>` and lots of `unwrap()`s.
///
/// If your transport handler type doesn’t need the notifier, you can simply
/// declare it its own seed and created it directly in `accept()`.
impl<T: Stream> AcceptHandler<T> for StreamAccept {
    type Output = StreamHandler;

    fn accept(&mut self, _addr: &SocketAddr) -> Option<RequestSender> {
        Some(self.req_tx.clone())
    }
}


//------------ StreamHandler -------------------------------------------------

/// The transport handler for stream transports.
///
/// Stream transports, ie., TCP and TLS connections, only ever process exactly
/// one finger transaction. They read a single line with a finger request,
/// process this request, send the response, and close the socket.
///
/// In other words, there are three stages of processing: the request stage
/// where we read a line from the socket and, if we have one, parese a request
/// from it and send it off to the processor; the await stage where we wait
/// for the processor to give us an answer; and the response stage where we
/// send the answer back.
///
/// This is best modelled as a algebraic type (aka an enum) with one variant
/// for each stage. To make things a little more clean, each variant contains
/// a type of its own that assembles all the information this stage needs and
/// also implements all the actual processing. The handler type then merely
/// dispatches to this ‘sub-types.‘
enum StreamHandler {
    Request(StreamRequest),
    Await(StreamAwait),
    Response(StreamResponse),
}

/// The transport handler implementation for the stream handler.
///
/// Each transport machine has an associated transport handler that performs
/// the actual work. These handlers are generic over the transport socket
/// type (the `T` below). When limiting the `T` in your `TransportHandler<T>`
/// implementation, you should stay as loose as possible. Netmachines
/// provides a number of socket traits for this purpose. Since our handler
/// works with all stream sockets, we pick the `Stream` trait. If the
/// handler would only work for unencrypted streams for some reason, we would
/// have picked `ClearStream`.
///
/// A transport handler has four functions that are mandatory to implement
/// plus two more for which there are somewhat sane default implementations
/// that should work for most protocols.
///
/// Each of these functions returns what should happen next through the
/// `Next<Self>` type. This could be waiting for the socket to be ready for
/// reading (`Next::read()`), writing (`Next::write()`), or both
/// (`Next::read_or_write()`). It could be to wait for the machine to be
/// woken up via a notifier (`Next::wait()`). It could also be to close the
/// underlying socket and end processing (`Next::remove()`).
///
/// Each of these options, except for remove, takes a transport handler
/// value. The idea here is that the current handler is moved into the
/// functions and, once processing has finished, a new handler is constructed
/// from the old one somehow which is moved into the `Next<Self>`.
///
/// Remove doesn’t take a value because it literally is the end of the world.
impl<T: Stream> TransportHandler<T> for StreamHandler {
    /// The seed type.
    ///
    /// This type should contain all the information that is necessary to
    /// construct a new transport handler value. See the discussion of
    /// `StreamHandler` above as to why introducing this seed type may have
    /// been a good idea.
    type Seed = RequestSender;

    /// A new transport has been created.
    ///
    /// This function receives everything it could possibly want for creating
    /// a new handler value. The `seed` holds everything passed in from the
    /// outside world. The `_sock` is a reference to the new socket. Most
    /// often you won’t need it, but just in cases it is there.
    ///
    /// The notifier can be used to wake up the transport machine. This will
    /// be necessary whenever the handler relies on outside help, much like
    /// we do here. When we have received a request, we send it to the
    /// processor and then go to sleep with `Next::wait()`. The machine will
    /// now remain dormant until `wakeup()` is being called on the notifier.
    /// It can safely be cloned and send across to other threads.
    ///
    /// In our case, we simply defer processing to our first state.
    fn create(seed: Self::Seed, _sock: &mut T, notifier: Notifier)
                 -> Next<Self> {
        StreamRequest::new(seed, notifier)
    }

    /// The transport socket may have become readable.
    ///
    /// A reference to the socket is passed in for your reading pleasure.
    /// See `StreamRequest::readble()` below for a discussion of some
    /// curious pitfalls.
    ///
    /// Since the request stage is the only stage where we actually read,
    /// we simply return for the other ones with the appropriate intent.
    fn readable(self, sock: &mut T) -> Next<Self> {
        match self {
            StreamHandler::Request(req) => req.readable(sock),
            val @ StreamHandler::Await(_) => Next::wait(val),
            val @ StreamHandler::Response(_) => Next::write(val),
        }
    }

    /// The transport socket may have become writable.
    ///
    /// This is like `readable()` except for writing.
    fn writable(self, sock: &mut T) -> Next<Self> {
        match self {
            val @ StreamHandler::Request(_) => Next::read(val),
            val @ StreamHandler::Await(_) => Next::wait(val),
            StreamHandler::Response(res) => res.writable(sock)
        }
    }

    /// The machine has been woken up through a notifier.
    ///
    /// This happens once for each time `wakeup()` is successfully called on
    /// a copy of the machine’s notifier. Calls are not limited to when
    /// `Next::wait()` was returned but can happen at any time.
    fn wakeup(self, _sock: &mut T) -> Next<Self> {
        match self {
            val @ StreamHandler::Request(_) => Next::read(val),
            StreamHandler::Await(await) => await.wakeup(),
            val @ StreamHandler::Response(_) => Next::write(val),
        }
    }

    /// An error has occured.
    ///
    /// What this error means depends, unsurprisingly, on `err`. Most errors
    /// relate to something bad having happened to the socket. In this case
    /// it is probably best to simply return `Next::remove()`.
    ///
    /// If the error is `Error::Timeout`, then a timeout happened. You can
    /// set a timeout by calling `Next`’s `timeout()` function. If no event
    /// happens before that time has passed, `Error::Timeout` happens instead.
    ///
    /// The implementation below is identical to the default implementation
    /// and given here merely for posterity.
    fn error(self, _err: Error) -> Next<Self> {
        Next::remove()
    }
}


//--- StreamRequest

/// The request stage of handling a stream transaction.
struct StreamRequest {
    /// The sending end of the channel for requests.
    sender: RequestSender,

    /// A notifier to wake ourselves up later.
    notifier: Notifier,

    /// A buffer to store what we have read so far.
    buf: Vec<u8>
}

impl StreamRequest {
    /// Creates the next stream handler for the request stage.
    ///
    /// Most attributes have to be passed in from the outside. The buffer,
    /// however, is created anew. We reserve space for one standard-sized
    /// line which should really be enough.
    fn new(sender: RequestSender, notifier: Notifier) -> Next<StreamHandler> {
        Next::read(
            StreamHandler::Request(
                StreamRequest { sender: sender, notifier: notifier,
                                buf: Vec::with_capacity(80) }
            )
        )
    }

    /// The transport socket may have become readable.
    ///
    /// As the headline suggests, this event only is an indication that
    /// reading from the socket may succeed. It is quite possible trying
    /// to read would actually block the socket. One example is a TLS socket
    /// that is stuck in a handshake. Another is a spurious event which is
    /// always possible.
    ///
    /// If you use `Read::read()` for reading, there would be an error with
    /// `ErrorKind::WouldBlock`. Since matching on `io::Error` is a little
    /// unwieldy, `TryRead::try_read()` is the better choice. It simply
    /// returns `Ok(None)` which is quite simple to match on.
    ///
    /// This is exactly what we do here. If reading succeeds, we try to
    /// parse out a request and if that succeeds, too, we move on.
    ///
    /// If we don’t like what we’ve read, we turn the error into a response
    /// (which is simply a string with some text) and progress to the
    /// response stage directly.
    fn readable<T: Stream>(mut self, sock: &mut T) -> Next<StreamHandler> {
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

    /// Dispatches a request and moves on to await stage.
    ///
    /// The method creates a ‘portal’ for the response and then sends it off
    /// to the processor. See `Return` for a discussion of how responses are
    /// returned.
    fn progress(self, request: Request) -> Next<StreamHandler> {
        let (tx, rx) = gate(self.notifier);

        if let Err(_) = self.sender.send((request, Return::Stream(tx))) {
            return StreamResponse::new(b"Service temporarily kaputt.\r\n");
        }

        StreamAwait::new(rx)
    }
}


//--- StreamAwait

/// The await stage of handling a stream transaction.
struct StreamAwait {
    /// A response will mysteriously appear here.
    rx: GateReceiver<String>
}

impl StreamAwait {
    /// Creates the initial next stream handler for the await stage.
    fn new(rx: GateReceiver<String>) -> Next<StreamHandler> {
        Next::wait(
            StreamHandler::Await(
                StreamAwait { rx: rx }
            )
        )
    }

    /// The machine has been woken up through a notifier.
    ///
    /// This happens when the processor has finished its job. We try to
    /// get the response by replacing whatever is in `self.rx` with a
    /// `None`. If that leads to `Some(_)`thing, then we have a response
    /// and can move on. Otherwise, we just keep waiting.
    fn wakeup(self) -> Next<StreamHandler> {
        match self.rx.try_get() {
            Ok(Some(response)) => StreamResponse::new(response.as_bytes()),
            Ok(None) => Next::wait(StreamHandler::Await(self)),
            Err(_) => StreamResponse::new(b"Internal server error.\r\n")
        }
    }
}


//--- StreamResponse

/// The response stage of handling a stream transaction.
struct StreamResponse {
    /// The response.
    ///
    /// This is basically a bytes vector that remembers how much we have
    /// written already. We need this since TCP may not send all the data
    /// at once.
    buf: ByteBuf
}

impl StreamResponse {
    /// Creates the initial next stream handler for the response stage.
    fn new(bytes: &[u8]) -> Next<StreamHandler> {
        Next::write(
            StreamHandler::Response(
                StreamResponse { buf: ByteBuf::from_slice(bytes) }
            )
        )
    }

    /// The transport socket may have become writable.
    ///
    /// Whatever was said for reading in `StreamRequest` above holds for
    /// writing as well. If we have some data left, we try to send that out,
    /// advancing the buffer accordingly.
    ///
    /// Once our buffer is empty, we close the socket and the machine by
    /// returning `Next::remove()`. Game over.
    fn writable<T: Stream>(mut self, sock: &mut T) -> Next<StreamHandler> {
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

/// The transport handler for datagram transports.
///
/// With datagram transport such as UDP there are no stages. Instead, for
/// every message received, we let the processor create a response and send
/// it back to wherever the message came from.
struct DgramHandler {
    /// Where to send requests for processing.
    req_tx: RequestSender,

    /// The sending end of the duct to get responses back.
    ///
    /// A duct is a synchronization type that comes with netmachines. It is
    /// similar to Rust’s own channel except that it is associated with a
    /// state machine. Every time someone sends an item, this state machine
    /// is being woken up.
    ///
    /// We need to keep the sending end around since we have to pass a clone
    /// of it to the processor every time we give it a request.
    tx: DuctSender<(String, SocketAddr)>,

    /// The receiving end of the duct to get responses back.
    rx: DuctReceiver<(String, SocketAddr)>,

    /// A response to be send out, if there is one.
    send: Option<(String, SocketAddr)>,
}

impl DgramHandler {
    /// Returns the next transport handler.
    ///
    /// Most importantly, this helper method determines the socket events we
    /// are interested in. We are always interested in reading. Whenever we
    /// have a response to send out, in which case `self.send` is `Some(_)`,
    /// we also are interested in writing.
    fn next(self) -> Next<Self> {
        if self.send.is_some() {
            Next::read_and_write(self)
        }
        else {
            Next::read(self)
        }
    }
}

/// The transport handler implementation for the stream handler.
///
/// This should be routine by now.
impl<T: Dgram> TransportHandler<T> for DgramHandler {
    type Seed = RequestSender;

    fn create(seed: Self::Seed, _sock: &mut T, notifier: Notifier)
                 -> Next<Self> {
        let (tx, rx) = duct(notifier);
        Next::read(DgramHandler { req_tx: seed, tx: tx, rx: rx, send: None })
    }

    fn readable(self, sock: &mut T) -> Next<Self> {
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

    fn writable(mut self, sock: &mut T) -> Next<Self> {
        if let Some((message, addr)) = mem::replace(&mut self.send, None) {
            match sock.send_to(message.as_bytes(), &addr) {
                Ok(Some(_)) => { }
                Ok(None) => self.send = Some((message, addr)),
                Err(_) => return Next::remove()
            }
        }
        while let Ok(Some((message, addr))) = self.rx.try_recv() {
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

    fn wakeup(mut self, _sock: &mut T) -> Next<Self> {
        if let Ok(Some((message, addr))) = self.rx.try_recv() {
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
    Dgram(DuctSender<(String, SocketAddr)>, SocketAddr),
    Stream(GateSender<String>)
}

impl Return {
    fn send(self, response: String) {
        match self {
            Return::Dgram(tx, addr) => {
                let _ = tx.send((response, addr));
            }
            Return::Stream(tx) => {
                let _ = tx.send(response);
            }
        }
    }
}


//------------ RequestSender -------------------------------------------------

type RequestSender = Sender<(Request, Return)>;


//------------ Processor -----------------------------------------------------

struct Processor {
    info: UserInfo,
    tasks: Receiver<(Request, Return)>
}

impl Processor {
    fn new(info: UserInfo) -> (Self, RequestSender) {
        let (tx, rx) = channel();
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

