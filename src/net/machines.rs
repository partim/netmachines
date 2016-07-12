//! Fundamental machines for networked sockets.

use std::marker::PhantomData;
use rotor::{EventSet, GenericScope, Machine, PollOpt, Response, Scope, Void};
use ::error::Error;
use ::handlers::{AcceptHandler, RequestHandler, TransportHandler};
use ::machines::RequestMachine;
use ::next::Intent;
use ::sockets::{Accept, Blocked, Transport};
use ::sync::DuctSender;
use ::utils::ResponseExt;


//------------ TransportMachine ----------------------------------------------

pub struct TransportMachine<X, T: Transport, H: TransportHandler<T>> {
    sock: T,
    handler: H,
    intent: Intent,
    marker: PhantomData<X>
}

impl<X, T: Transport, H: TransportHandler<T>> TransportMachine<X, T, H> {
    pub fn new<S: GenericScope>(mut sock: T, seed: H::Seed, scope: &mut S)
                                -> Response<Self, Void> {
        let next = H::create(seed, &mut sock, scope.notifier());
        if let Some((intent, handler)) = Intent::new(next, scope) {
            let conn = TransportMachine::make(sock, handler, intent);
            match scope.register(&conn.sock, conn.intent.events(),
                                 PollOpt::level()) {
                Ok(_) => { }
                Err(err) => return Response::error(err.into())
            }
            conn.response()
        }
        else {
            Response::done()
        }
    }
}

impl<X, T: Transport, H: TransportHandler<T>> TransportMachine<X, T, H> {
    fn make(sock: T, handler: H, intent: Intent) -> Self {
        TransportMachine {
            sock: sock,
            handler: handler,
            intent: intent,
            marker: PhantomData
        }
    }

    fn next<S>(self, scope: &mut Scope<X>) -> Response<Self, S> {
        let events = match self.sock.blocked() {
            Some(Blocked::Read) => EventSet::readable(),
            Some(Blocked::Write) => EventSet::writable(),
            None => self.intent.events()
        };
        match scope.reregister(&self.sock, events, PollOpt::level()) {
            Ok(_) => { }
            Err(err) => return Response::error(err.into())
        }
        self.response()
    }

    fn response<S>(self) -> Response<Self, S> {
        if let Some(deadline) = self.intent.deadline() {
            Response::ok(self).deadline(deadline)
        }
        else {
            Response::ok(self)
        }
    }
}


//--- Machine

impl<X, T, H> Machine for TransportMachine<X, T, H>
              where T: Transport, H: TransportHandler<T> {
    type Context = X;
    type Seed = (T, H::Seed);

    fn create(seed: Self::Seed, scope: &mut Scope<X>)
              -> Response<Self, Void> {
        TransportMachine::new(seed.0, seed.1, scope)
    }

    fn ready(mut self, events: EventSet, scope: &mut Scope<X>)
                -> Response<Self, Self::Seed> {
        if events.is_error() {
            if let Err(err) = self.sock.take_socket_error() {
                let next = self.handler.error(err.into());
                if let Some((intent, handler)) = self.intent.merge(next,
                                                                   scope) {
                    return TransportMachine::make(self.sock, handler, intent)
                                            .next(scope);
                }
                else {
                    return Response::done()
                }
            }
        }

        // If the socket is blocked, we pretent the events are actually those
        // the handler has requested so the socket is read from or written to
        // and can become unblocked. (If the handler’s request was for wait,
        // then what are we doing here in the first place?)
        let events = if let Some(_) = self.sock.blocked() {
            self.intent.events()
        } else {
            events
        };

        if events.is_readable() {
            let next = self.handler.readable(&mut self.sock);
            if let Some((intent, handler)) = self.intent.merge(next, scope) {
                self = TransportMachine::make(self.sock, handler, intent)
            }
            else {
                return Response::done()
            }
        }

        if events.is_writable() {
            let next = self.handler.writable(&mut self.sock);
            if let Some((intent, handler)) = self.intent.merge(next, scope) {
                self = TransportMachine::make(self.sock, handler, intent)
            }
            else {
                return Response::done()
            }
        }
        self.next(scope)
    }

    fn spawned(self, _scope: &mut Scope<X>) -> Response<Self, Self::Seed> {
        Response::ok(self)
    }

    fn timeout(self, scope: &mut Scope<X>) -> Response<Self, Self::Seed> {
        let next = self.handler.error(Error::Timeout);
        if let Some((intent, handler)) = self.intent.merge(next, scope) {
            TransportMachine::make(self.sock, handler, intent).next(scope)
        }
        else {
            Response::done()
        }
    }

    fn wakeup(self, scope: &mut Scope<X>) -> Response<Self, Self::Seed> {
        let next = self.handler.wakeup();
        if let Some((intent, handler)) = self.intent.merge(next, scope) {
            TransportMachine::make(self.sock, handler, intent).next(scope)
        }
        else {
            Response::done()
        }
    }
}


//------------ ServerMachine ------------------------------------------------

pub struct ServerMachine<X, A, H>(
    ServerInner<A, H, TransportMachine<X, A::Output, H::Output>>,
    PhantomData<X>)
           where A: Accept, H: AcceptHandler<A::Output>;

enum ServerInner<A, H, M> {
    Lsnr(A, H),
    Conn(M)
}

impl<X, A: Accept, H: AcceptHandler<A::Output>> ServerMachine<X, A, H> {
    pub fn new<S: GenericScope>(sock: A, handler: H, scope: &mut S)
                                -> Response<Self, Void> {
        match scope.register(&sock, EventSet::readable(), PollOpt::level()) {
            Ok(()) => Response::ok(ServerMachine::lsnr(sock, handler)),
            Err(err) => Response::error(err.into()),
        }
    }
}

impl<X, A: Accept, H: AcceptHandler<A::Output>> ServerMachine<X, A, H> {
    fn lsnr(lsnr: A, handler: H) -> Self {
        ServerMachine(ServerInner::Lsnr(lsnr, handler), PhantomData)
    }

    fn conn(conn: TransportMachine<X, A::Output, H::Output>)
            -> Self {
        ServerMachine(ServerInner::Conn(conn), PhantomData)
    }

    fn accept(lsnr: A, mut handler: H)
              -> Response<Self, <Self as Machine>::Seed> {
        match lsnr.accept() {
            Ok(Some((sock, addr))) => {
                if let Some(seed) = handler.accept(&addr) {
                    Response::spawn(ServerMachine::lsnr(lsnr, handler),
                                    (sock, seed))
                }
                else {
                    Response::ok(ServerMachine::lsnr(lsnr, handler))
                }
            }
            Ok(None) => {
                Response::ok(ServerMachine::lsnr(lsnr, handler))
            }
            Err(_) => {
                // XXX log
                Response::ok(ServerMachine::lsnr(lsnr, handler))
            }
        }
    }
}


//--- Machine

impl<X, A, H> Machine for ServerMachine<X, A, H>
              where A: Accept, H: AcceptHandler<A::Output> {
    type Context = X;
    type Seed = (A::Output, <H::Output as TransportHandler<A::Output>>::Seed);

    fn create(seed: Self::Seed, scope: &mut Scope<X>)
              -> Response<Self, Void> {
        TransportMachine::create(seed, scope).map_self(ServerMachine::conn)
    }

    fn ready(self, events: EventSet, scope: &mut Scope<X>)
             -> Response<Self, Self::Seed> {
        match self.0 {
            ServerInner::Lsnr(lsnr, handler) => {
                ServerMachine::accept(lsnr, handler)
            }
            ServerInner::Conn(conn) => {
                conn.ready(events, scope).map_self(ServerMachine::conn)
            }
        }
    }

    fn spawned(self, scope: &mut Scope<X>) -> Response<Self, Self::Seed> {
        match self.0 {
            ServerInner::Lsnr(lsnr, handler) => {
                ServerMachine::accept(lsnr, handler)
            }
            ServerInner::Conn(conn) => {
                conn.spawned(scope).map_self(ServerMachine::conn)
            }
        }
    }

    fn timeout(self, scope: &mut Scope<X>) -> Response<Self, Self::Seed> {
        match self.0 {
            ServerInner::Lsnr(..) => unreachable!("listener can’t timeout"),
            ServerInner::Conn(conn) => {
                conn.timeout(scope).map_self(ServerMachine::conn)
            }
        }
    }

    fn wakeup(self, scope: &mut Scope<X>) -> Response<Self, Self::Seed> {
        match self.0 {
            ServerInner::Lsnr(..) => unreachable!("listener can’t wakeup"),
            ServerInner::Conn(conn) => {
                conn.wakeup(scope).map_self(ServerMachine::conn)
            }
        }
    }
}


//------------ ClientMachine -------------------------------------------------

pub struct ClientMachine<X, T, RH, TH>(
    ClientInner<RequestMachine<X, T, RH>, TransportMachine<X, T, TH>>,
    PhantomData<X>)
           where T: Transport,
                 RH: RequestHandler<T>,
                 TH: TransportHandler<T, Seed=RH::Seed>;

enum ClientInner<RM, TM> {
    Req(RM),
    Trsp(TM),
}

impl<X, T, RH, TH> ClientMachine<X, T, RH, TH>
                   where T: Transport,
                         RH: RequestHandler<T>,
                         TH: TransportHandler<T, Seed=RH::Seed> {
    pub fn new<S: GenericScope>(handler: RH, scope: &mut S)
                                -> (Self, DuctSender<RH::Request>) {
        let (req, f) = RequestMachine::new(handler, scope);
        (ClientMachine::req(req), f)
    }    
}

impl<X, T, RH, TH> ClientMachine<X, T, RH, TH>
                   where T: Transport,
                         RH: RequestHandler<T>,
                         TH: TransportHandler<T, Seed=RH::Seed> {
    fn req(machine: RequestMachine<X, T, RH>) -> Self {
        ClientMachine(ClientInner::Req(machine), PhantomData)
    }

    fn trsp(machine: TransportMachine<X, T, TH>) -> Self {
        ClientMachine(ClientInner::Trsp(machine), PhantomData)
    }
}

impl<X, T, RH, TH> Machine for ClientMachine<X, T, RH, TH>
                   where T: Transport,
                         RH: RequestHandler<T>,
                         TH: TransportHandler<T, Seed=RH::Seed> {
    type Context = X;
    type Seed = (T, TH::Seed);

    fn create(seed: Self::Seed, scope: &mut Scope<X>)
              -> Response<Self, Void> {
        TransportMachine::create(seed, scope).map_self(ClientMachine::trsp)
    }

    fn ready(self, events: EventSet, scope: &mut Scope<X>)
             -> Response<Self, Self::Seed> {
        match self.0 {
            ClientInner::Req(machine) => {
                machine.ready(events, scope).map_self(ClientMachine::req)
            }
            ClientInner::Trsp(machine) => {
                machine.ready(events, scope).map_self(ClientMachine::trsp)
            }
        }
    }

    fn spawned(self, scope: &mut Scope<X>) -> Response<Self, Self::Seed> {
        match self.0 {
            ClientInner::Req(machine) => {
                machine.spawned(scope).map_self(ClientMachine::req)
            }
            ClientInner::Trsp(machine) => {
                machine.spawned(scope).map_self(ClientMachine::trsp)
            }
        }
    }

    fn timeout(self, scope: &mut Scope<X>) -> Response<Self, Self::Seed> {
        match self.0 {
            ClientInner::Req(machine) => {
                machine.timeout(scope).map_self(ClientMachine::req)
            }
            ClientInner::Trsp(machine) => {
                machine.timeout(scope).map_self(ClientMachine::trsp)
            }
        }
    }

    fn wakeup(self, scope: &mut Scope<X>) -> Response<Self, Self::Seed> {
        match self.0 {
            ClientInner::Req(machine) => {
                machine.wakeup(scope).map_self(ClientMachine::req)
            }
            ClientInner::Trsp(machine) => {
                machine.wakeup(scope).map_self(ClientMachine::trsp)
            }
        }
    }
}

