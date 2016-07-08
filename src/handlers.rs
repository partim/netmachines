//! Traits for handlers.

use std::net::SocketAddr;
use ::error::Error;
use ::next::Next;
use ::sync::Control;


pub trait AcceptHandler<T> {
    type Output: TransportHandler<T>;

    /// Accepts an incoming connection request.
    fn on_accept(&mut self, addr: &SocketAddr)
                 -> Option<<Self::Output as TransportHandler<T>>::Seed>;
}

pub trait RequestHandler<T> {
    type Request: Send;
    type Seed;

    fn on_request(&mut self, request: Self::Request)
                  -> Option<(T, Self::Seed)>;
}

pub trait TransportHandler<T> {
    type Seed;

    fn on_create(seed: Self::Seed, sock: &mut T, ctrl: Control) -> Self;

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

