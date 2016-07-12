//! Fundamental machines.

use std::marker::PhantomData;
use rotor::{GenericScope, EventSet, Machine, Response, Scope, Void};
use ::handlers::RequestHandler;
use ::sync::{DuctReceiver, DuctSender, duct};

//------------ RequestMachine -----------------------------------------------

pub struct RequestMachine<X, T, H: RequestHandler<T>> {
    handler: H,
    rx: DuctReceiver<H::Request>,
    marker: PhantomData<X>
}

impl<X, T, H: RequestHandler<T>> RequestMachine<X, T, H> {
    pub fn new<S: GenericScope>(handler: H, scope: &mut S)
                                -> (Self, DuctSender<H::Request>) {
        let (tx, rx) = duct(scope.notifier());
        (RequestMachine { handler: handler, rx: rx, marker: PhantomData },
         tx)
    }
}

impl<X, T, H: RequestHandler<T>> Machine for RequestMachine<X, T, H> {
    type Context = X;
    type Seed = (T, H::Seed);

    fn create(_seed: Self::Seed, _scope: &mut Scope<X>)
              -> Response<Self, Void> {
        unreachable!("RequestMachines cannot be created.")
    }

    fn ready(self, _events: EventSet, _scope: &mut Scope<X>)
             -> Response<Self, Self::Seed> {
        unreachable!("RequestMachines cannot become ready.")
    }

    fn spawned(mut self, _scope: &mut Scope<X>)
               -> Response<Self, Self::Seed> {
        loop {
            match self.rx.try_recv() {
                Ok(Some(request)) => {
                    match self.handler.request(request) {
                        Some(seed) => return Response::spawn(self, seed),
                        None => { }
                    }
                }
                Ok(None) => return Response::ok(self),
                Err(_) => return Response::done()
            }
        }
    }

    fn timeout(self, _scope: &mut Scope<X>) -> Response<Self, Self::Seed> {
        unreachable!("RequestMachines cannot time out.")
    }

    fn wakeup(self, scope: &mut Scope<X>) -> Response<Self, Self::Seed> {
        self.spawned(scope)
    }
}


