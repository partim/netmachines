//! Fundamental machines.

use std::marker::PhantomData;
use rotor::{GenericScope, EventSet, Machine, Response, Scope, Void};
use ::error::Error;
use ::handlers::RequestHandler;
use ::sync::{DuctReceiver, DuctSender, duct};
use ::utils::ResponseExt;

pub trait SeedFactory<O, S> {
    fn translate(&self, output: O) -> Result<S, TranslateError<O>>;
}

pub struct TranslateError<O>(pub O, pub Error);

//------------ RequestMachine -----------------------------------------------

pub struct RequestMachine<X, M, H, F>(Inner<M, H, F>, PhantomData<X>)
                          where M: Machine<Context=X>,
                                H: RequestHandler,
                                F: SeedFactory<H::Output, M::Seed>;

enum Inner<M: Machine, H: RequestHandler, F: SeedFactory<H::Output, M::Seed>> {
    Req(Req<H, M::Seed, F>),
    M(M)
}

impl<X, M, H, F> RequestMachine<X, M, H, F>
                 where M: Machine<Context=X>,
                       H: RequestHandler,
                       F: SeedFactory<H::Output, M::Seed> {
    pub fn new<S>(handler: H, factory: F, scope: &mut S)
                  -> (Response<Self, Void>, DuctSender<H::Request>)
               where S: GenericScope {
        let (tx, rx) = duct(scope.notifier());
        (Response::ok(RequestMachine::req(Req::new(rx, handler, factory))),
         tx)
    }
}

impl<X, M, H, F> RequestMachine<X, M, H, F>
                 where M: Machine<Context=X>,
                       H: RequestHandler,
                       F: SeedFactory<H::Output, M::Seed> {
    fn req(req: Req<H, M::Seed, F>) -> Self {
        RequestMachine(Inner::Req(req), PhantomData)
    }

    fn m(m: M) -> Self {
        RequestMachine(Inner::M(m), PhantomData)
    }
}

impl<X, M, H, F> Machine for RequestMachine<X, M, H, F>
                 where M: Machine<Context=X>,
                       H: RequestHandler,
                       F: SeedFactory<H::Output, M::Seed> {
    type Context = X;
    type Seed = M::Seed;

    fn create(seed: Self::Seed, scope: &mut Scope<X>)
              -> Response<Self, Void> {
        M::create(seed, scope).map_self(RequestMachine::m)
    }

    fn ready(self, events: EventSet, scope: &mut Scope<X>)
             -> Response<Self, Self::Seed> {
        match self.0 {
            Inner::Req(_) => {
                unreachable!("Request handler can’t be ready")
            }
            Inner::M(machine) => {
                machine.ready(events, scope).map_self(RequestMachine::m)
            }
        }
    }

    fn spawned(self, scope: &mut Scope<X>) -> Response<Self, Self::Seed> {
        match self.0 {
            Inner::Req(req) => {
                req.process_requests().map_self(RequestMachine::req)
            }
            Inner::M(machine) => {
                machine.spawned(scope).map_self(RequestMachine::m)
            }
        }
    }

    fn timeout(self, scope: &mut Scope<X>) -> Response<Self, Self::Seed> {
        match self.0 {
            Inner::Req(_) => {
                unreachable!("Request handler can’t time out")
            }
            Inner::M(machine) => {
                machine.timeout(scope).map_self(RequestMachine::m)
            }
        }
    }

    fn wakeup(self, scope: &mut Scope<X>) -> Response<Self, Self::Seed> {
        match self.0 {
            Inner::Req(req) => {
                req.process_requests().map_self(RequestMachine::req)
            }
            Inner::M(machine) => {
                machine.wakeup(scope).map_self(RequestMachine::m)
            }
        }
    }
}


//------------ Req -----------------------------------------------------------

struct Req<H: RequestHandler, S, F: SeedFactory<H::Output, S>> {
    rx: DuctReceiver<H::Request>,
    handler: H,
    factory: F,
    marker: PhantomData<S>
}

impl<H: RequestHandler, S, F: SeedFactory<H::Output, S>> Req<H, S, F> {
    fn new(rx: DuctReceiver<H::Request>, handler: H, factory: F) -> Self {
        Req { rx: rx, handler: handler, factory: factory,
              marker: PhantomData }
    }

    fn process_requests(mut self) -> Response<Self, S> {
        loop {
            match self.rx.try_recv() {
                Ok(Some(request)) => {
                    if let Some(output) = self.handler.request(request) {
                        match self.factory.translate(output) {
                            Ok(seed) => return Response::spawn(self, seed),
                            Err(err) => self.handler.error(err.0, err.1)
                        }
                    }
                }
                Ok(None) => return Response::ok(self),
                Err(_) => return Response::done()
            }
        }
    }
}

