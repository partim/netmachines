//! A collection of rotor state machines for typical networking tasks.
//!
//! The *netmachines* crate provides a thin layer over [rotor] with the aim
//! to make it straightforward to implement and use networking protocols.
//!
//! See the [intro] module for an introduction to the crate and the
//! ecosystem it operates in.
//!
//! [intro]: intro/index.html
//! [rotor]: ../rotor/index.html

#[macro_use] extern crate log;
extern crate rotor;

#[cfg(feature = "openssl")]
extern crate openssl;

#[cfg(feature = "security-framework")]
extern crate security_framework;

pub use error::{Error, Result};
pub use handlers::{AcceptHandler, RequestHandler, TransportHandler};

#[macro_use] mod macros;

pub mod error;
pub mod handlers;
pub mod intro;
pub mod net;
pub mod next;
pub mod request;
pub mod sockets;
pub mod sync;
pub mod utils;

mod compose;

