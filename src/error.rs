//! Error.

use std::error;
use std::fmt;
use std::io;
use std::result;

//------------ Error --------------------------------------------------------

#[derive(Debug)]
pub enum Error {
    Timeout,
    Io(io::Error),
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
            Error::Timeout => "Timeout",
            Error::Io(ref err) => err.description()
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


//------------ Result -------------------------------------------------------

pub type Result<T> = result::Result<T, Error>;

