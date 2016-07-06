//! Handlers.

use std::net::SocketAddr;
use ::error::Error;
use ::next::Next;
use ::sync::Control;


pub trait AcceptHandler<X, T> {
    type Output: TransportHandler<T>;

    fn on_accept(context: &mut X, addr: &SocketAddr, ctrl: Control)
                 -> Option<Self::Output>;
}

pub trait ConnectHandler<X, T> {
    type Output: TransportHandler<T>;

    fn on_connect(context: &mut X, sock: &mut T, ctrl: Control)
                  -> Self::Output;
}

pub trait TransportHandler<T> {
    fn on_start(&mut self) -> Next;
    fn on_read(&mut self, stream: &mut T) -> Next;
    fn on_write(&mut self, stream: &mut T) -> Next;
    fn on_error(&mut self, err: Error) -> Next;
    fn on_close(self, stream: T);
}

