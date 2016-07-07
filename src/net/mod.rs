
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
    }
}

pub mod machines;
pub mod clear;

#[cfg(feature = "openssl")]
pub mod openssl;

