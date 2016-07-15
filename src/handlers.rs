//! Traits for handlers.
//!
//! Each rotor machine provided by the netmachines crate uses a number of
//! *handler types* to perform the actual work. There are three categories
//! of handler types, each defined by the handler trait it has to implement:
//! an [AcceptHandler] is used by machines listening for incoming
//! [stream][Stream] connections, a [RequestHandler] is used by client
//! machines to translate client requests into
//! [transports sockets][Transport], and a [TransportHandler] implements the
//! behaviour of such transport sockets. 
//!
//! [AcceptHandler]: trait.AcceptHandler.html
//! [RequestHandler]: trait.RequestHandler.html
//! [TransportHandler]: trait.TransportHandler.html
//! [Stream]: ../sockets/trait.Stream.html
//! [Transport]: ../sockets/trait.Transport.html

use std::net::SocketAddr;
use rotor::Notifier;
use ::error::Error;
use ::next::Next;


/// The trait implemented by an accept handler.
///
/// An accept handler is used by strem servers to process incoming
/// connection requests. For each request, the [accept()](#tymethod.accept)
/// method is called once.
///
/// Note that trait is generic over the transport socket `T` used by the
/// connections created by accepting, not the accept socket.
pub trait AcceptHandler<T> {
    /// The transport handler ultimately created when accepting a connection. 
    type Output: TransportHandler<T>;

    /// Accepts an incoming connection request.
    ///
    /// The `addr` argument contains the peer address of the incoming request.
    ///
    /// The method can decide whether to accept the request or not. If it
    /// returns `None`, the connection is closed cleanly immediately.
    /// Otherwise, the method returns the seed for the transport handler to
    /// be created for processing the connection. See the discussion of how
    /// transport handlers are created at the [TransportHandler] trait.
    ///
    /// [TransportHandler]: trait.TransportHandler.html
    fn accept(&mut self, addr: &SocketAddr)
              -> Option<<Self::Output as TransportHandler<T>>::Seed>;
}

/// The trait implemented by a request handler.
///
/// Request handlers are currently still in flux. A proper explanation will
/// follow.
pub trait RequestHandler {
    type Request: Send;
    type Output;

    fn request(&mut self, request: Self::Request) -> Option<Self::Output>;
    fn error(&mut self, _output: Self::Output, _err: Error) { }
}

pub trait TransportHandler<T>: Sized {
    type Seed;

    fn create(seed: Self::Seed, sock: &mut T, notifier: Notifier)
              -> Next<Self>;

    /// Called when the socket becomes readable.
    fn readable(self, sock: &mut T) -> Next<Self>;

    /// Called when the socket becomes writable.
    fn writable(self, sock: &mut T) -> Next<Self>;

    /// Called upon wakeup via a notifier.
    fn wakeup(self) -> Next<Self>;

    /// Called when an error has occured on the socket.
    ///
    /// You are free to signal any next value here, though most likely
    /// `Next::remove()` is the safest choice.
    fn error(self, _err: Error) -> Next<Self> {
        Next::remove()
    }

    /// Called after `Next::remove()`.
    ///
    /// Both `self` and the socket are moved into the method. So this is
    /// your last chance to transfer them out if you want to hang on to
    /// them.
    fn remove(self, _sock: T) { }
}

