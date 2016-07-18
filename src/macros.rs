//! Internal macros.

/// Creates the machine method implementation for a wrapped machine.
///
/// The type needs to be a tuple struct with the wrapped machine appearing
/// as its first element. The seed type used by the type must be the same
/// as that of the wrapped machine.
///
/// The macro takes the name of the inner machine--only the name
/// no type arguments as it is used for calling the machine `create()`
/// function--and an expression for mapping the wrapped machine into the
/// outer type.
///
/// It only creates the methods of the `Machine` implementation, not the
/// `impl` headline nor the two `type`s you need.
macro_rules! wrapped_machine {
    ($inner:ident, $map:expr) => {
        fn create(seed: Self::Seed, scope: &mut Scope<Self::Context>)
                  -> Response<Self, Void> {
            $inner::create(seed, scope).map_self($map)
        }

        fn ready(self, events: EventSet, scope: &mut Scope<Self::Context>)
                 -> Response<Self, Self::Seed> {
            self.0.ready(events, scope).map_self($map)
        }

        fn spawned(self, scope: &mut Scope<Self::Context>)
                   -> Response<Self, Self::Seed> {
            self.0.spawned(scope).map_self($map)
        }

        fn timeout(self, scope: &mut Scope<Self::Context>)
                   -> Response<Self, Self::Seed> {
            self.0.timeout(scope).map_self($map)
        }

        fn wakeup(self, scope: &mut Scope<Self::Context>)
                  -> Response<Self, Self::Seed> {
            self.0.wakeup(scope).map_self($map)
        }
    };
}
