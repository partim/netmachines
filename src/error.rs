//! Error and result.
//!
//! Currently, netmachines uses a mixture of `std::io::Error` and its own
//! error type. This hierarchy bears some thinking about, so this is likely
//! to change.

use std::error;
use std::fmt;
use std::io;
use std::result;

#[cfg(feature = "openssl")]
use openssl::ssl::error::SslError as OpensslError;


//------------ Error --------------------------------------------------------

/// The error type.
///
#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    NoSlabSpace,
    Timeout,
    Tls, // XXX Make this proper.
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Error::Io(ref err) => err.fmt(f),
            ref err => f.write_str(error::Error::description(err))
        }
    }
}

impl error::Error for Error {
    fn description(&self) -> &str {
        match *self {
            Error::Io(ref err) => err.description(),
            Error::NoSlabSpace => "slab space limit reached",
            Error::Timeout => "Timeout",
            Error::Tls => "TLS error",
        }
    }

    fn cause(&self) -> Option<&error::Error> {
        match *self {
            Error::Io(ref err) => Some(err),
            _ => None
        }
    }
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Error {
        Error::Io(err)
    }
}

#[cfg(feature = "openssl")]
impl From<OpensslError> for Error {
    fn from(err: OpensslError) -> Error {
        match err {
            OpensslError::StreamError(err) => Error::Io(err),
            _ => Error::Tls
        }
    }
}


//------------ Result -------------------------------------------------------

pub type Result<T> = result::Result<T, Error>;

