//! Fundamental machines for networked sockets.

use std::marker::PhantomData;
use rotor::{EventSet, GenericScope, Machine, PollOpt, Response, Scope, Void};
use ::error::Error;
use ::handlers::{AcceptHandler, RequestHandler, TransportHandler};
use ::machines::RequestMachine;
use ::next::{Intent, Interest, Next};
use ::sockets::{Accept, Transport};
use ::sync::{Receiver, Funnel, ctrl_channel};
use ::utils::ResponseExt;


//------------ TransportMachine ----------------------------------------------

pub struct TransportMachine<X, T: Transport, H: TransportHandler<T>> {
    sock: T,
    handler: H,
    intent: Intent,
    rx: Receiver<Next>,
    marker: PhantomData<X>
}

impl<X, T: Transport, H: TransportHandler<T>> TransportMachine<X, T, H> {
    fn new(sock: T, handler: H, rx: Receiver<Next>) -> Self {
        TransportMachine {
            sock: sock,
            handler: handler,
            intent: Intent::new(),
            rx: rx,
            marker: PhantomData
        }
    }

    fn merge(&mut self, next: Next, scope: &mut Scope<X>) {
        self.intent = self.intent.merge(next, scope)
    }

    fn next<S>(self, scope: &mut Scope<X>) -> Response<Self, S> {
        if let Some(events) = self.intent.events() {
            match scope.reregister(&self.sock, events, PollOpt::level()) {
                Ok(_) => { }
                Err(err) => return Response::error(err.into())
            }
        }
        self.response(scope)
    }

    fn response<S>(self, scope: &mut Scope<X>) -> Response<Self, S> {
        match self.intent.interest() {
            Interest::Remove => {
                let _ = scope.deregister(&self.sock);
                self.handler.on_remove(self.sock);
                Response::done()
            }
            _ => {
                if let Some(deadline) = self.intent.deadline() {
                    Response::ok(self).deadline(deadline)
                }
                else {
                    Response::ok(self)
                }
            }
        }
    }
}


//--- Machine

impl<X, T, H> Machine for TransportMachine<X, T, H>
              where T: Transport, H: TransportHandler<T> {
    type Context = X;
    type Seed = (T, H::Seed);

    fn create(mut seed: Self::Seed, scope: &mut Scope<X>)
              -> Response<Self, Void> {
        let (tx, rx) = ctrl_channel(scope.notifier());
        let mut handler = H::on_create(seed.1, &mut seed.0, tx);
        let next = handler.on_start();
        let mut conn = TransportMachine::new(seed.0, handler, rx);
        conn.merge(next, scope);
        if let Some(events) = conn.intent.events() {
            match scope.register(&conn.sock, events, PollOpt::level()) {
                Ok(_) => { }
                Err(err) => return Response::error(err.into())
            }
        }
        conn.response(scope)
    }

    fn ready(mut self, events: EventSet, scope: &mut Scope<X>)
                -> Response<Self, Self::Seed> {
        if events.is_error() {
            match self.sock.take_socket_error() {
                Ok(_) => { }
                Err(err) => {
                    let next = self.handler.on_error(err.into());
                    self.merge(next, scope);
                    return self.next(scope);
                }
            }
        }
        if events.is_readable() {
            let next = self.handler.on_read(&mut self.sock);
            self.merge(next, scope);
            if self.intent.is_close() {
                return Response::done()
            }
        }
        if events.is_writable() {
            let next = self.handler.on_write(&mut self.sock);
            self.merge(next, scope);
        }
        self.next(scope)
    }

    fn spawned(self, _scope: &mut Scope<X>) -> Response<Self, Self::Seed> {
        Response::ok(self)
    }

    fn timeout(mut self, scope: &mut Scope<X>) -> Response<Self, Self::Seed> {
        let next = self.handler.on_error(Error::Timeout);
        self.merge(next, scope);
        self.next(scope)
    }

    fn wakeup(mut self, scope: &mut Scope<X>) -> Response<Self, Self::Seed> {
        let mut intent = self.intent;
        while let Ok(next) = self.rx.try_recv() {
            intent = intent.merge(next, scope)
        };
        self.intent = intent;
        self.next(scope)
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
                if let Some(seed) = handler.on_accept(&addr) {
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
                                -> (Self, Funnel<RH::Request>) {
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

