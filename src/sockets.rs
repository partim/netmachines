//! Socket traits and implementations for standard network sockets.
//!
//! Instead of using socket object directly, we are generalizing everything
//! over a trait for the socket category we want to use. This way, we can
//! replace the actual networked sockets easily with various kinds of mock
//! sockets for testing.
//!
//! There’s five traits for the five categories of transport sockets. Three
//! traits are for stream sockets (ie., those based on TCP for networked
//! sockets): [ClearStream] for unencrypted streams, [SecureStream] for
//! uncrypted streams, and [HybridStream] for a stream that starts out
//! unencrypted but can be switched to an encrypted stream at any time.
//!
//! For datagram sockets (ie., UDP), there’s only two traits: [ClearDgram]
//! and [SecureDgram] for unencrypted and encrypted datagram sockets,
//! respectively. (If there actually is applications that start DTLS late
//! on a UDP socket, we’ll add HybrdDgram, too. For the moment, I am not
//! quite clear how that would actually work.)
//!
//! When implementing handlers, always make the implementation generic over
//! one of these traits so that you can use them with real networked sockets
//! and mock sockets.
//!
//! In addition, there is an [Accept] trait which defines the listener
//! socket for the stream sockets. This is only used to implement the
//! various state machines. You won’t need to worry about it when
//! implementing handlers.
//!
//! [ClearStream]: trait.ClearStream.html
//! [SecureStream]: trait.SecureStream.html
//! [HybridStream]: trait.HybridStream.html
//! [ClearDgram]: trait.ClearDgram.html
//! [SecureDgram]: trait.SecureDgram.html
//! [Accept]: trait.Accept.html

use std::io::{self, Read, Write};
use std::net::SocketAddr;
use rotor::mio::Evented;
use rotor::mio::tcp::{TcpListener, TcpStream};
use rotor::mio::udp::UdpSocket;
use ::error::Result;


//------------ Accept -------------------------------------------------------

/// A trait for listener sockets.
///
/// Listener sockets are bound to a given local address and are waiting for
/// peers to try to connect to this address. Any such pending connection
/// requests can be extracted by calling the [accept()](#tymethod.accept)
/// method.
pub trait Accept: Evented {
    /// The socket type produced by accepting.
    type Output;

    /// Accept a new connection.
    ///
    /// If there is at least one pending connection request on the socket,
    /// it returns a new stream socket for this request and the peer
    /// address.
    ///
    /// If there is no pending requests, simply returns `None`.
    ///
    /// The method may also fail with various IO errors. Generally, just
    /// shrugging and trying again later is fine.
    fn accept(&self) -> Result<Option<(Self::Output, SocketAddr)>>;
}


//--- impl for TcpListener

impl Accept for TcpListener {
    type Output = TcpStream;

    fn accept(&self) -> Result<Option<(Self::Output, SocketAddr)>> {
        Ok(try!(self.accept()))
    }
}


//------------ ClearStream --------------------------------------------------

/// A trait for unencrypted stream sockets.
///
/// These sockets provide an unencrypted, sequenced, reliable, two-way,
/// connection-based byte stream. This translates quite conveniently to
/// Rust’s `Read` and `Write` traits.
///
/// Note that `ClearStream`s are non-blocking sockets. Trying to read or
/// write when the socket isn’t ready will result in a `WouldBlock` error.
/// Note further that if reading or writing of non-empty buffers return
/// `Ok(0)`, the other side has performed an orderly shutdown of the
/// socket and it is time to let go.
pub trait ClearStream: Read + Write + Evented { }


//--- impl for TcpStream

impl ClearStream for TcpStream { }


//------------ SecureStream -------------------------------------------------

/// A trait for encrypted stream sockets.
///
/// These sockets are almost identical to [ClearStream] sockets except that
/// they transport all data in encrypted form. For networked sockets, this
/// means TLS. For mock sockets, this may mean nothing at all.
///
/// Like [ClearStream] sockets, these sockets map into Rust’s `Read` and
/// `Write` traits. However, because the encryption layer may have to do
/// some work of its own, reading and writing may fail with `WouldBlock`
/// even if readability or writability was signalled.
///
/// If the encryption handshake fails, this will be signalled to the
/// [TransportHandler]. Further reading or writing will simply fail with
/// `ConnectionAborted`.
///
/// [ClearStream]: trait.ClearStream.html
/// [TransportHandler]: ../handlers/trait.TransportHandler.html
pub trait SecureStream: Read + Write + Evented { }


//------------ HybridStream -------------------------------------------------

/// A trait for a stream socket that can start encryption later.
///
/// Hybrid stream sockets start out life as unencrypted stream sockets akin
/// to [ClearStream]s. By calling the [start_tls()](#tymethod.start_tls)
/// method, an encryption handshake can be started. If the handshake
/// succeeds, the sockets are encrypted akin to [SecureStream] sockets. If
/// the handshake fails, the socket becomes unusable.
///
/// Since the handshake happens asynchronously, a failure is signalled to
/// the [TransportHandler]. Success isn’t signalled at all, operation will
/// just continue.
///
/// [ClearStream]: trait.ClearStream.html
/// [SecureStream]: trait.SecureStream.html
/// [TransportHandler]: ../handlers/trait.TransportHandler.html
pub trait HybridStream: Read + Write + Evented {
    /// Starts the encryption handshake for this socket.
    ///
    /// The actual handshake will happen synchronously, so an `Ok(())`
    /// return value will not mean that the socket is now encrypted.
    /// However, reading and writing after calling this method will only
    /// succeed if the handshake has succeeded. While the handshake is
    /// still in progress, they will fail with `WouldBlock`. If the
    /// handshake fails, all reading and writing will fail with
    /// `ConnectionAborted`.
    fn start_tls(&mut self) -> Result<()> 
}


//------------ ClearDgram ---------------------------------------------------

/// A trait for unencrypted datagram sockets.
///
/// These sockets provide transportation of unencrypted, unreliable,
/// connectionless messages of a limited size. Sockets are bound to a local
/// address and can receive messages from any remote address.
///
/// Messages can be send to a specific remote address with the
/// [send_to()](#tymethod.send_to) method whenever the socket is writable.
/// An incoming message can be retrieved with the
/// [recv_from()](#tymethod.recv_from) method whenever the socket is
/// readable. This will return both the message content and the remote
/// address the message was sent from.
///
/// XXX Should we add the triple of `connect()`, `send()` and `recv()`
///     to the trait?
///
pub trait ClearDgram: Evented {
    /// Attempts to retrieve an incoming message from the socket.
    ///
    /// If there is at least one pending message available and it was
    /// successfully retrieved, the method will copy the message’s content
    /// into `buf` and return `Ok(Some(..))` with the number of bytes
    /// copied and the remote address the message was sent from. If the
    /// message was longer than the provided buffer, excess bytes will be
    /// discarded quietly. Zero-length messages are valid, so
    /// `Ok(Some((0, _))` is a perfectly fine result and (unlike with stream
    /// sockets) has no special meaning attached.
    ///
    /// If there are no pending messages, returns `Ok(None)` and doesn’t do
    /// anything else.
    ///
    /// Any other returned error condition is likely fatal.
    fn recv_from(&self, buf: &mut [u8])
                 -> io::Result<Option<(usize, SocketAddr)>>;

    /// Sends a message to the socket.
    ///
    /// The message content is given in `buf` and the remote address to
    /// which the message should be sent in `target`.
    ///
    /// If the socket is writable and the message was sent successfully,
    /// returns `Ok(Some(_))` with the number of bytes sent. Because
    /// datagram sockets are unreliable, this does not mean the message has
    /// actually arrived at the far end.
    ///
    /// If the socket is not writable, returns `Ok(None)`.
    ///
    /// If the buffer is too large to be sent, the method will fail with
    /// `Other` (XXX presumably, someone should try that).
    fn send_to(&self, buf: &[u8], target: &SocketAddr)
               -> io::Result<Option<usize>>;
}


//--- impl for UdpSocket

impl ClearDgram for UdpSocket {
    fn recv_from(&self, buf: &mut [u8])
                 -> io::Result<Option<(usize, SocketAddr)>> {
        self.recv_from(buf)
    }

    fn send_to(&self, buf: &[u8], target: &SocketAddr)
               -> io::Result<Option<usize>> {
        self.send_to(buf, target)
    }
}


//------------ SecureDgram --------------------------------------------------

/// A trait for encrypted datagram sockets.
///
/// These sockets provide transportation of unencrypted, unreliable,
/// connectionless messages of a limited size. They behave mostly identical
/// to unencrypted [ClearDgram] sockets accept that messages are
/// transparently encrypted when sending and decrypted when receiving.
/// Networked sockets use DTLS for this purpose, mock sockets can do
/// whatever they want.
///
/// In order to allow for encryption, a separate encryption session has to
/// be established for each remote address communication is desired with.
/// The [SecureDgram] socket takes care of all that. This means that both
/// `send_to()` and `recv_from()` will return `Ok(None)` more often while
/// the socket deals with handshake.
pub trait SecureDgram: Evented {
    /// Attempts to retrieve an incoming message from the socket.
    ///
    /// If there is at least one pending message available and it was
    /// successfully retrieved, the method will copy the message’s content
    /// into `buf` and return `Ok(Some(..))` with the number of bytes
    /// copied and the remote address the message was sent from. If the
    /// message was longer than the provided buffer, excess bytes will be
    /// discarded quietly. Zero-length messages are valid, so
    /// `Ok(Some((0, _))` is a perfectly fine result and (unlike with stream
    /// sockets) has no special meaning attached.
    ///
    /// If there are no pending messages, returns `Ok(None)` and doesn’t do
    /// anything else.
    ///
    /// Any other returned error condition is likely fatal.
    fn recv_from(&self, buf: &mut [u8])
                 -> io::Result<Option<(usize, SocketAddr)>>;

    /// Sends a message to the socket.
    ///
    /// The message content is given in `buf` and the remote address to
    /// which the message should be sent in `target`.
    ///
    /// If the socket is writable and the message was sent successfully,
    /// returns `Ok(Some(_))` with the number of bytes sent. Because
    /// datagram sockets are unreliable, this does not mean the message has
    /// actually arrived at the far end.
    ///
    /// If the socket is not writable, returns `Ok(None)`.
    ///
    /// If the buffer is too large to be sent, the method will fail with
    /// `Other` (XXX presumably, someone should try that).
    fn send_to(&self, buf: &[u8], target: &SocketAddr)
               -> io::Result<Option<usize>>;
}
