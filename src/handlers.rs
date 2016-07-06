//! Traits for handlers.
//!
//! There’s three types of handler traits. The most important one is
//! [TransportHandler<T>] which is implemented by all types operating on
//! an already present transport socket. Two more traits assign these
//! types to their socket: servers that listen on an address and
//! create transport sockets for incoming connections use
//! [AcceptHandler<X, T>] and for cases where sockets are created
//! elsewhere in the program and handed in, [CreateHandler<X, T>] is
//! used.
//!
//! All these traits are generic to a transport socket type `T`. Types
//! that implement these traits should be generic over that type, too,
//! but should limit it to one of the traits defined in the
//! [socket] module. This way, they are limited to a specific category
//! of sockets but being flexible as to whether actual network sockets
//! are used or mock sockets.
//!
//! [TransportHandler<T>]: trait.TransportHandler.html
//! [AcceptHandler<X, T>]: trait.AcceptHandler.html
//! [CreateHandler<X, T>]: trait.CreateHandler.html

use std::net::SocketAddr;
use ::error::Error;
use ::next::Next;
use ::sync::Control;


/// A trait for creating a transport handler for an incoming connection.
///
/// The trait is generic over two types. `X` is the state machine’s context
/// and can be used to inject additional ’global’ information into the
/// process of creating the handler. `T` is the transport socket type of
/// the transport handler created.
///
/// Every time a connection request arrives,
/// [on_accept()](#tymethod.on_accept) is called with the context, the peer
/// address of the connection and a [Control] value that can be used to
/// wake up the underlying state machine of the created transport. The
/// method can now either create a new transport handler and return it or
/// decide to return `None` in which case the new connection is shut down
/// right away.
///
/// [Control]: ../control/struct.Control.html
pub trait AcceptHandler<X, T> {
    /// The type of the transport handler created by this accept handler.
    type Output: TransportHandler<T>;

    /// Accepts an incoming connection request.
    ///
    /// The state machine’s context is passed in the `context` argument.
    /// The address of the peer making the connection request is passed
    /// in `addr`.
    ///
    /// Finally, a [Control] value is given in `ctrl`. This value can be
    /// used to wake up the state machine underlying the transport if the
    /// transport handler returns `[Next.wait()]` from any method.
    ///
    /// The method should return either a new transport handler value or
    /// `None` indicating that it does not wish to honor the connection
    /// request.
    ///
    /// [Control]: ../control/struct.Control.html
    /// [Next.wait()]: ../next/struct.Next.html#method.wait
    fn on_accept(context: &mut X, addr: &SocketAddr, ctrl: Control)
                 -> Option<Self::Output>;
}

/// A trait for creating a transport handler for application-provided sockets.
///
/// This handler trait is generic over two types. `T` is the transport
/// socket trait for which the handler can be used. `S` is the seed type
/// handed in when adding sockets.
///
/// The motivation for this somewhat complicated construction lies in the
/// fact that the [Control] value for waking up a transport machine is only
/// available after the machine has been created. For cases where the
/// transport handler type wishes to keep the control, passing it in after
/// the it has been created would require an `Option<Control>` either
/// always unwrapping it or extra matches.
///
/// Whenever the type doesn’t need the control or doesn’t need to hang on to
/// it, the type itself can be used as `S` and the handler directly created
/// outside.
pub trait CreateHandler<S, T> {
    type Output: TransportHandler<T>;

    fn on_create(seed: S, sock: &mut T, ctrl: Control) -> Self::Output;
}

/// A trait for processing a transport socket.
///
/// The trait is generic over the transport socket type `T`. A type
/// implementing this trait should be generic over `T` as well, possibly
/// restricting it to a specific socket category through the socket traits.
///
/// A type implementing this trait will not get to own the socket in
/// question but rather will receive a mutable reference to it in the
/// relevant methods. The only exception is after the socket has been
/// removed from the state machine after returning `Next::remove()`, when
/// the socket is actually moved to the type for further treatment.
pub trait TransportHandler<T> {
    /// Called when the socket is first inserted into the state machine.
    ///
    /// The purpose of this method is to determine the initial set of events
    /// to wait for. All `Next` values are allowed here, even `Next::remove()`.
    fn on_start(&mut self) -> Next;

    /// Called when the socket becomes readable.
    fn on_read(&mut self, sock: &mut T) -> Next;

    /// Called when the socket becomes writable.
    fn on_write(&mut self, sock: &mut T) -> Next;

    /// Called when an error has occured on the socket.
    ///
    /// You are free to signal any next value here, though most likely
    /// `Next::remove()` is the safest choice.
    fn on_error(&mut self, err: Error) -> Next;

    /// Called after `Next::remove()`.
    ///
    /// Both `self` and the socket are moved into the method. So this is
    /// your last chance to transfer them out if you want to hang on to
    /// them.
    fn on_remove(self, sock: T);
}

