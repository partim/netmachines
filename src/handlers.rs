//! Traits for handlers.

use std::net::SocketAddr;
use ::error::Error;
use ::next::Next;

//============ Reexports =====================================================

pub use rotor::Notifier;


//============ The Handler Traits ============================================

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

pub trait TransportHandler<T>: Sized {
    type Seed;

    fn on_create(seed: Self::Seed, sock: &mut T, notifier: Notifier)
                 -> Next<Self>;

    /// Called when the socket becomes readable.
    fn on_read(self, sock: &mut T) -> Next<Self>;

    /// Called when the socket becomes writable.
    fn on_write(self, sock: &mut T) -> Next<Self>;

    /// Called upon wakeup via a notifier.
    fn on_notify(self) -> Next<Self>;

    /// Called when an error has occured on the socket.
    ///
    /// You are free to signal any next value here, though most likely
    /// `Next::remove()` is the safest choice.
    fn on_error(self, _err: Error) -> Next<Self> {
        Next::remove()
    }

    /// Called after `Next::remove()`.
    ///
    /// Both `self` and the socket are moved into the method. So this is
    /// your last chance to transfer them out if you want to hang on to
    /// them.
    fn on_remove(self, _sock: T) { }
}

