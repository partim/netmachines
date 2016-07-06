//! Machines for networked clients.

use std::marker::PhantomData;
use rotor::{EventSet, Machine, Response, Scope, Void};
use ::handlers::ConnectHandler;
use ::sync::{ctrl_channel, Receiver, TryRecvError};
use super::transport::TransportMachine;


//------------ ClientMachine ------------------------------------------------

pub struct ClientMachine<X, T, H, C>(ClientInner<C, T>, PhantomData<(X, T, H)>)
           where T: Send, H: ConnectHandler<X, T>,
                 C: TransportMachine<X, T, H::Output>;

enum ClientInner<C, T> {
    Ctrl(Receiver<T>),
    Conn(C)
}

impl<X, T, H, C> ClientMachine<X, T, H, C>
                 where T: Send, H: ConnectHandler<X, T>,
                       C: TransportMachine<X, T, H::Output> {
    fn ctrl(ctrl: Receiver<T>) -> Self {
        ClientMachine(ClientInner::Ctrl(ctrl), PhantomData)
    }

    fn conn(conn: C) -> Self {
        ClientMachine(ClientInner::Conn(conn), PhantomData)
    }
}

impl<X, T, H, C> Machine for ClientMachine<X, T, H, C>
                 where T: Send, H: ConnectHandler<X, T>,
                       C: TransportMachine<X, T, H::Output> {
    type Context = X;
    type Seed = T;

    fn create(mut seed: T, scope: &mut Scope<X>) -> Response<Self, Void> {
        let (ctrl, rx) = ctrl_channel(scope.notifier());
        let handler = H::on_connect(scope, &mut seed, ctrl);
        C::create(seed, handler, rx, scope).map(ClientMachine::conn,
                                                |seed| seed)
    }

    fn ready(self, events: EventSet, scope: &mut Scope<X>)
             -> Response<Self, Self::Seed> {
        match self.0 {
            ClientInner::Ctrl(_) => {
                unreachable!("Controller can’t be ready")
            }
            ClientInner::Conn(conn) => {
                conn.ready(events, scope).map(ClientMachine::conn, |seed| seed)
            }
        }
    }

    fn spawned(self, scope: &mut Scope<X>) -> Response<Self, Self::Seed> {
        match self.0 {
            ClientInner::Ctrl(ctrl) => Response::ok(ClientMachine::ctrl(ctrl)),
            ClientInner::Conn(conn) => {
                conn.spawned(scope).map(ClientMachine::conn, |seed| seed)
            }
        }
    }

    fn timeout(self, scope: &mut Scope<X>) -> Response<Self, Self::Seed> {
        match self.0 {
            ClientInner::Ctrl(_) => {
                unreachable!("controller can’t timeout")
            }
            ClientInner::Conn(conn) => {
                conn.timeout(scope).map(ClientMachine::conn, |seed| seed)
            }
        }
    }

    fn wakeup(self, scope: &mut Scope<X>) -> Response<Self, Self::Seed> {
        match self.0 {
            ClientInner::Ctrl(rx) => {
                loop {
                    match rx.try_recv() {
                        Ok(sock) => {
                            return Response::spawn(ClientMachine::ctrl(rx), sock)
                        }
                        Err(TryRecvError::Empty) => {
                            return Response::ok(ClientMachine::ctrl(rx))
                        }
                        Err(TryRecvError::Disconnected) => {
                            return Response::done()
                        }
                    }
                }
            }
            ClientInner::Conn(conn) => {
                conn.wakeup(scope).map(ClientMachine::conn, |seed| seed)
            }
        }
    }
}

