//! Miscellany.

use rotor::Response;


pub trait ResponseExt<M, N> {
    fn map_self<T, F>(self, op: F) -> Response<T, N>
                where F: FnOnce(M) -> T;
}

impl<M: Sized, N: Sized> ResponseExt<M, N> for Response<M, N> {
    fn map_self<T, F>(self, op: F) -> Response<T, N>
                where F: FnOnce(M) -> T {
        self.map(op, |seed| seed)
    }
}
