//! Machines for networked servers.

use std::marker::PhantomData;
use std::net::SocketAddr;
use rotor::{EventSet, Machine, Response, Scope, Void};
use ::handlers::AcceptHandler;
use ::sockets::Accept;
use ::sync::ctrl_channel;
use ::utils::ResponseExt;
use super::transport::TransportMachine;


//------------ ServerMachine ------------------------------------------------

pub struct ServerMachine<X, T, H, M>(ServerInner<T, M>, PhantomData<(X, T, H)>)
           where T: Accept,
                 H: AcceptHandler<X, T::Output>,
                 M: TransportMachine<X, T::Output, H::Output>;

enum ServerInner<T, M> {
    Lsnr(T),
    Conn(M)
}


impl<X, T, H, M> ServerMachine<X, T, H, M>
           where T: Accept,
                 H: AcceptHandler<X, T::Output>,
                 M: TransportMachine<X, T::Output, H::Output> {
    fn lsnr(lsnr: T) -> Self {
        ServerMachine(ServerInner::Lsnr(lsnr), PhantomData)
    }

    fn conn(conn: M) -> Self {
        ServerMachine(ServerInner::Conn(conn), PhantomData)
    }
}

impl<X, T, H, M> Machine for ServerMachine<X, T, H, M>
           where T: Accept,
                 H: AcceptHandler<X, T::Output>,
                 M: TransportMachine<X, T::Output, H::Output> {
    type Context = X;
    type Seed = (T::Output, SocketAddr);

    fn create(seed: Self::Seed, scope: &mut Scope<X>)
              -> Response<Self, Void> {
        let (ctrl, rx) = ctrl_channel(scope.notifier());
        let handler = match H::on_accept(scope, &seed.1, ctrl) {
            Some(handler) => handler,
            None => return Response::done()
        };
        M::create(seed.0, handler, rx, scope).map_self(ServerMachine::conn)
    }

    fn ready(self, events: EventSet, scope: &mut Scope<X>)
             -> Response<Self, Self::Seed> {
        match self.0 {
            ServerInner::Lsnr(lsnr) => {
                match lsnr.accept() {
                    Ok(Some(seed)) => {
                        Response::spawn(ServerMachine::lsnr(lsnr), seed)
                    }
                    Ok(None) => {
                        Response::ok(ServerMachine::lsnr(lsnr))
                    }
                    Err(_) => {
                        // XXX log
                        Response::ok(ServerMachine::lsnr(lsnr))
                    }
                }
            }
            ServerInner::Conn(conn) => {
                conn.ready(events, scope).map_self(ServerMachine::conn)
            }
        }
    }

    fn spawned(self, scope: &mut Scope<X>) -> Response<Self, Self::Seed> {
        match self.0 {
            ServerInner::Lsnr(lsnr) => {
                match lsnr.accept() {
                    Ok(Some(seed)) => {
                        Response::spawn(ServerMachine::lsnr(lsnr), seed)
                    }
                    Ok(None) => {
                        Response::ok(ServerMachine::lsnr(lsnr))
                    }
                    Err(_) => {
                        // XXX log
                        Response::ok(ServerMachine::lsnr(lsnr))
                    }
                }
            }
            ServerInner::Conn(conn) => {
                conn.spawned(scope).map_self(ServerMachine::conn)
            }
        }
    }

    fn timeout(self, scope: &mut Scope<X>) -> Response<Self, Self::Seed> {
        match self.0 {
            ServerInner::Lsnr(_) => unreachable!("listener can’t timeout"),
            ServerInner::Conn(conn) => {
                conn.timeout(scope).map_self(ServerMachine::conn)
            }
        }
    }

    fn wakeup(self, scope: &mut Scope<X>) -> Response<Self, Self::Seed> {
        match self.0 {
            ServerInner::Lsnr(_) => unreachable!("listener can’t wakeup"),
            ServerInner::Conn(conn) => {
                conn.wakeup(scope).map_self(ServerMachine::conn)
            }
        }
    }
}

