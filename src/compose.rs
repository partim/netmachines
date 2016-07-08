
use rotor::{EventSet, Machine, Response, Scope, Void};
use ::utils::ResponseExt;

//------------ Compose2 -----------------------------------------------------

pub use rotor::Compose2;


//------------ Compose3 -----------------------------------------------------

pub enum Compose3<A: Sized, B: Sized, C: Sized> {
    A(A),
    B(B),
    C(C)
}

pub enum Compose3Seed<A: Sized, B: Sized, C: Sized> {
    As(A),
    Bs(B),
    Cs(C)
}

impl<X, AA, BB, CC> Machine for Compose3<AA, BB, CC>
                    where AA: Machine<Context=X>,
                          BB: Machine<Context=X>,
                          CC: Machine<Context=X> {
    type Context = X;
    type Seed = Compose3Seed<AA::Seed, BB::Seed, CC::Seed>;

    fn create(seed: Self::Seed, scope: &mut Scope<X>)
              -> Response<Self, Void> {
        use self::Compose3::*;
        use self::Compose3Seed::*;

        match seed {
            As(s) => AA::create(s, scope).map_self(A),
            Bs(s) => BB::create(s, scope).map_self(B),
            Cs(s) => CC::create(s, scope).map_self(C)
        }
    }

    fn ready(self, events: EventSet, scope: &mut Scope<X>)
             -> Response<Self, Self::Seed> {
        use self::Compose3::*;
        use self::Compose3Seed::*;

        match self {
            A(m) => m.ready(events, scope).map(A, As),
            B(m) => m.ready(events, scope).map(B, Bs),
            C(m) => m.ready(events, scope).map(C, Cs)
        }
    }

    fn spawned(self, scope: &mut Scope<X>) -> Response<Self, Self::Seed> {
        use self::Compose3::*;
        use self::Compose3Seed::*;

        match self {
            A(m) => m.spawned(scope).map(A, As),
            B(m) => m.spawned(scope).map(B, Bs),
            C(m) => m.spawned(scope).map(C, Cs)
        }
    }

    fn timeout(self, scope: &mut Scope<X>) -> Response<Self, Self::Seed> {
        use self::Compose3::*;
        use self::Compose3Seed::*;

        match self {
            A(m) => m.timeout(scope).map(A, As),
            B(m) => m.timeout(scope).map(B, Bs),
            C(m) => m.timeout(scope).map(C, Cs)
        }
    }

    fn wakeup(self, scope: &mut Scope<X>) -> Response<Self, Self::Seed> {
        use self::Compose3::*;
        use self::Compose3Seed::*;

        match self {
            A(m) => m.wakeup(scope).map(A, As),
            B(m) => m.wakeup(scope).map(B, Bs),
            C(m) => m.wakeup(scope).map(C, Cs)
        }
    }
}
