//! Secure sockets using OpenSSL.

use std::io;
use std::mem;
use std::net::{self, SocketAddr};
use openssl::ssl::{self, SslContext, SslStream};
use rotor::{Evented, EventSet, PollOpt};
use rotor::mio::{Selector, Token};
use rotor::mio::tcp::{TcpListener, TcpStream};
use super::{Accept, Blocked, HybridStream, SecureStream, Stream, Transport};
use ::error::Result;


//------------ TlsListener ---------------------------------------------------

pub struct TlsListener {
    sock: TcpListener,
    ctx: SslContext,
}

impl TlsListener {
    pub fn bind(addr: &SocketAddr, ctx: SslContext) -> Result<Self> {
        Ok(TlsListener { sock: try!(TcpListener::bind(addr)),
                         ctx: ctx })
    }

    pub fn from_listener(lsnr: net::TcpListener, addr: &SocketAddr,
                         ctx: SslContext) -> Result<Self> {
        Ok(TlsListener { sock: try!(TcpListener::from_listener(lsnr, addr)),
                         ctx: ctx })
    }
}

impl Accept for TlsListener {
    type Output = TlsStream;

    fn accept(&self) -> Result<Option<(TlsStream, SocketAddr)>> {
        match self.sock.accept() {
            Ok(Some((stream, addr))) => {
                Ok(Some((try!(TlsStream::accept(stream, &self.ctx)),
                         addr)))
            }
            Ok(None) => Ok(None),
            Err(err) => Err(err.into())
        }
    }
}

impl Evented for TlsListener {
    fn register(&self, selector: &mut Selector, token: Token,
                interest: EventSet, opts: PollOpt) -> io::Result<()> {
        self.sock.register(selector, token, interest, opts)
    }

    fn reregister(&self, selector: &mut Selector, token: Token,
                  interest: EventSet, opts: PollOpt) -> io::Result<()> {
        self.sock.reregister(selector, token, interest, opts)
    }

    fn deregister(&self, selector: &mut Selector) -> io::Result<()> {
        self.sock.deregister(selector)
    }
}


//------------ TlsStream -----------------------------------------------------

pub struct TlsStream {
    sock: SslStream<TcpStream>,
    blocked: Option<Blocked>,
}

impl TlsStream {
    fn accept(stream: TcpStream, ctx: &SslContext) -> Result<TlsStream> {
        Ok(TlsStream  { sock: try!(SslStream::accept(ctx, stream)),
                        blocked: None })
    }

    fn translate_error(&mut self, err: ssl::Error) -> io::Result<usize> {
        match err {
            ssl::Error::ZeroReturn => Ok(0),
            ssl::Error::WantWrite(err) => {
                self.blocked = Some(Blocked::Write);
                Err(err)
            }
            ssl::Error::WantRead(err) => {
                self.blocked = Some(Blocked::Read);
                Err(err)
            }
            ssl::Error::Stream(err) => Err(err),
            err => Err(io::Error::new(io::ErrorKind::Other, err))
        }
    }
}

impl SecureStream for TlsStream { }

impl Stream for TlsStream { }

impl io::Read for TlsStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.blocked = None;
        self.sock.ssl_read(buf).or_else(|err| self.translate_error(err))
    }
}

impl io::Write for TlsStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.blocked = None;
        self.sock.ssl_write(buf).or_else(|err| self.translate_error(err))
    }

    fn flush(&mut self) -> io::Result<()> {
        self.sock.flush()
    }
}

impl Transport for TlsStream {
    fn take_socket_error(&mut self) -> io::Result<()> {
        self.sock.get_mut().take_socket_error()
    }

    fn blocked(&self) -> Option<Blocked> {
        self.blocked
    }
}


impl Evented for TlsStream {
    fn register(&self, selector: &mut Selector, token: Token,
                interest: EventSet, opts: PollOpt) -> io::Result<()> {
        self.sock.get_ref().register(selector, token, interest, opts)
    }

    fn reregister(&self, selector: &mut Selector, token: Token,
                  interest: EventSet, opts: PollOpt) -> io::Result<()> {
        self.sock.get_ref().reregister(selector, token, interest, opts)
    }

    fn deregister(&self, selector: &mut Selector) -> io::Result<()> {
        self.sock.get_ref().deregister(selector)
    }
}


//------------ StartTlsListener ----------------------------------------------

pub struct StartTlsListener {
    sock: TcpListener,
    ctx: SslContext,
}

impl StartTlsListener {
    pub fn bind(addr: &SocketAddr, ctx: SslContext) -> Result<Self> {
        Ok(StartTlsListener { sock: try!(TcpListener::bind(addr)),
                              ctx: ctx })
    }

    pub fn from_listener(lsnr: net::TcpListener, addr: &SocketAddr,
                         ctx: SslContext) -> Result<Self> {
        Ok(StartTlsListener { sock: try!(TcpListener::from_listener(lsnr,
                                                                    addr)),
                              ctx: ctx })
    }
}

impl Accept for StartTlsListener {
    type Output = StartTlsStream;

    fn accept(&self) -> Result<Option<(StartTlsStream, SocketAddr)>> {
        match self.sock.accept() {
            Ok(Some((stream, addr))) => {
                Ok(Some((StartTlsStream::new(stream, self.ctx.clone()),
                         addr)))
            }
            Ok(None) => Ok(None),
            Err(err) => Err(err.into())
        }
    }
}

impl Evented for StartTlsListener {
    fn register(&self, selector: &mut Selector, token: Token,
                interest: EventSet, opts: PollOpt) -> io::Result<()> {
        self.sock.register(selector, token, interest, opts)
    }

    fn reregister(&self, selector: &mut Selector, token: Token,
                  interest: EventSet, opts: PollOpt) -> io::Result<()> {
        self.sock.reregister(selector, token, interest, opts)
    }

    fn deregister(&self, selector: &mut Selector) -> io::Result<()> {
        self.sock.deregister(selector)
    }
}


//------------ StartTlsStream ------------------------------------------------

pub struct StartTlsStream {
    sock: Option<StartTlsSock>,
    ctx: SslContext,
    blocked: Option<Blocked>
}

enum StartTlsSock {
    Clear(TcpStream),
    Secure(SslStream<TcpStream>)
}

impl StartTlsStream {
    fn new(stream: TcpStream, ctx: SslContext) -> StartTlsStream {
        StartTlsStream {
            sock: Some(StartTlsSock::Clear(stream)),
            ctx: ctx,
            blocked: None
        }
    }

    fn translate_result(&mut self,
                        res: ::std::result::Result<usize, ssl::Error>)
                        -> io::Result<usize> {
        match res {
            Ok(res) => Ok(res),
            Err(ssl::Error::ZeroReturn) => Ok(0),
            Err(ssl::Error::WantWrite(err)) => {
                self.blocked = Some(Blocked::Write);
                Err(err)
            }
            Err(ssl::Error::WantRead(err)) => {
                self.blocked = Some(Blocked::Read);
                Err(err)
            }
            Err(ssl::Error::Stream(err)) => Err(err),
            Err(err) => Err(io::Error::new(io::ErrorKind::Other, err))
        }
    }

    fn get_sock(&self) -> io::Result<&TcpStream> {
        match self.sock {
            Some(StartTlsSock::Clear(ref sock)) => Ok(sock),
            Some(StartTlsSock::Secure(ref sock)) => Ok(sock.get_ref()),
            None => Err(io::Error::new(io::ErrorKind::ConnectionAborted,
                                       "stream unusable"))
        }
    }

    fn get_mut_sock(&mut self) -> io::Result<&mut TcpStream> {
        match self.sock {
            Some(StartTlsSock::Clear(ref mut sock)) => Ok(sock),
            Some(StartTlsSock::Secure(ref mut sock)) => Ok(sock.get_mut()),
            None => Err(io::Error::new(io::ErrorKind::ConnectionAborted,
                                       "stream unusable"))
        }
    }
}

impl HybridStream for StartTlsStream {
    fn connect_secure(&mut self) -> Result<()> {
        let sock = mem::replace(&mut self.sock, None);
        if let Some(StartTlsSock::Clear(sock)) = sock {
            let sock = try!(SslStream::connect(&self.ctx, sock));
            self.sock = Some(StartTlsSock::Secure(sock));
            Ok(())
        }
        else {
            panic!("Stream is already encrypted.")
        }
    }

    fn accept_secure(&mut self) -> Result<()> {
        let sock = mem::replace(&mut self.sock, None);
        if let Some(StartTlsSock::Clear(sock)) = sock {
            let sock = try!(SslStream::accept(&self.ctx, sock));
            self.sock = Some(StartTlsSock::Secure(sock));
            Ok(())
        }
        else {
            panic!("Stream is already encrypted.")
        }
    }

    fn is_secure(&self) -> bool {
        match self.sock {
            Some(StartTlsSock::Secure(_)) => true,
            _ => false,
        }
    }
}

impl io::Read for StartTlsStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let res = match self.sock {
            Some(StartTlsSock::Clear(ref mut sock)) => return sock.read(buf),
            Some(StartTlsSock::Secure(ref mut sock)) => {
                self.blocked = None;
                sock.ssl_read(buf)
            }
            None => return Err(io::Error::new(io::ErrorKind::ConnectionAborted,
                                              "stream unusable"))
        };
        self.translate_result(res)
    }
}

impl io::Write for StartTlsStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let res = match self.sock {
            Some(StartTlsSock::Clear(ref mut sock)) => return sock.write(buf),
            Some(StartTlsSock::Secure(ref mut sock)) => {
                self.blocked = None;
                sock.ssl_write(buf)
            }
            None => return Err(io::Error::new(io::ErrorKind::ConnectionAborted,
                                              "stream unusable"))
        };
        self.translate_result(res)
    }

    fn flush(&mut self) -> io::Result<()> {
        try!(self.get_mut_sock()).flush()
    }
}

impl Transport for StartTlsStream {
    fn take_socket_error(&mut self) -> io::Result<()> {
        match self.sock {
            Some(StartTlsSock::Clear(ref mut sock)) => {
                sock.take_socket_error()
            }
            Some(StartTlsSock::Secure(ref mut sock)) => {
                sock.get_mut().take_socket_error()
            }
            None => {
                Err(io::Error::new(io::ErrorKind::ConnectionAborted,
                                   "stream unusable"))
            }
        }
    }

    fn blocked(&self) -> Option<Blocked> {
        self.blocked
    }
}

impl Evented for StartTlsStream {
    fn register(&self, selector: &mut Selector, token: Token,
                interest: EventSet, opts: PollOpt) -> io::Result<()> {
        try!(self.get_sock()).register(selector, token, interest, opts)
    }

    fn reregister(&self, selector: &mut Selector, token: Token,
                  interest: EventSet, opts: PollOpt) -> io::Result<()> {
        try!(self.get_sock()).reregister(selector, token, interest, opts)
    }

    fn deregister(&self, selector: &mut Selector) -> io::Result<()> {
        try!(self.get_sock()).deregister(selector)
    }
}

