//! Building blocks for generic transport machines.

use rotor::{EventSet, Response, Scope, Void};
use ::handlers::TransportHandler;
use ::next::Next;
use ::sync::Receiver;


pub trait TransportMachine<X, T, H: TransportHandler<T>>: Sized {
    fn create(sock: T, handler: H, rx: Receiver<Next>,
              scope: &mut Scope<X>) -> Response<Self, Void>;
    fn ready<S>(self, events: EventSet, scope: &mut Scope<X>)
                -> Response<Self, S>;
    fn spawned<S>(self, scope: &mut Scope<X>) -> Response<Self, S>;
    fn timeout<S>(self, scope: &mut Scope<X>) -> Response<Self, S>;
    fn wakeup<S>(self, scope: &mut Scope<X>) -> Response<Self, S>;
}
