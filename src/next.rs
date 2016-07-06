//! Next.

use std::cmp::min;
use std::fmt;
use std::time::Duration;
use rotor::{EventSet, Scope, Time};


//------------ Next ---------------------------------------------------------

#[must_use]
#[derive(Clone)]
pub struct Next {
    interest: Interest,
    timeout: Option<Duration>,
}


impl Next {
    fn new(interest: Interest) -> Next {
        Next { interest: interest, timeout: None }
    }

    pub fn wait() -> Next { Next::new(Interest::Wait) }

    pub fn read() -> Next { Next::new(Interest::Read) }
    
    pub fn write() -> Next { Next::new(Interest::Write) }
    
    pub fn read_and_write() -> Next { Next::new(Interest::ReadWrite) }
    
    pub fn remove() -> Next { Next::new(Interest::Remove) }

    pub fn timeout(mut self, duration: Duration) -> Self {
        self.timeout = Some(duration);
        self
    }
}


//--- Debug

impl fmt::Debug for Next {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        try!(write!(f, "Next::{:?}", &self.interest));
        match self.timeout {
            Some(ref d) => write!(f, "({:?})", d),
            None => Ok(())
        }
    }
}


//------------ Interest -----------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Interest {
    Wait,
    Read,
    Write,
    ReadWrite,
    Remove
}


//------------ Intent -------------------------------------------------------

#[derive(Clone, Copy, Debug)]
pub struct Intent {
    interest: Option<Interest>,
    deadline: Option<Time>
}

impl Intent {
    pub fn new() -> Self {
        Intent { interest: None, deadline: None }
    }

    pub fn merge<X>(self, other: Next, scope: &mut Scope<X>) -> Intent {
        use self::Interest::*;

        let interest = match self.interest {
            Some(interest) =>  {
                match (interest, other.interest) {
                    (Remove, _) | (_, Remove) => Remove,
                    (ReadWrite, _) | (_, ReadWrite) | (Read, Write) |
                    (Write, Read) => {
                        ReadWrite
                    }
                    (Read, _) | (_, Read) => Read,
                    (Write, _) | (_, Write) => Write,
                    (Wait, Wait) => Wait
                }
            }
            None => other.interest
        };

        let deadline = match (self.deadline, other.timeout) {
            (Some(deadline), Some(timeout)) => {
                Some(min(deadline, scope.now() + timeout))
            }
            (None, Some(timeout)) => Some(scope.now() + timeout),
            (deadline, None) => deadline
        };
        
        Intent { interest: Some(interest), deadline: deadline }
    }

    pub fn interest(&self) -> Interest {
        match self.interest {
            Some(interest) => interest,
            None => Interest::Wait
        }
    }

    pub fn deadline(&self) -> Option<Time> {
        self.deadline
    }

    pub fn is_close(&self) -> bool {
        self.interest == Some(Interest::Remove)
    }

    /// Returns the events for self.
    ///
    /// Returns `None` for `Next::close()` or `Some(_)` for everything else.
    pub fn events(&self) -> Option<EventSet> {
        match self.interest {
            Some(Interest::Wait) => Some(EventSet::none()),
            Some(Interest::Read) => Some(EventSet::readable()),
            Some(Interest::Write) => Some(EventSet::writable()),
            Some(Interest::ReadWrite) => {
                Some(EventSet::readable() | EventSet::writable())
            }
            Some(Interest::Remove) => None,
            None => unreachable!()
        }
    }
}

//--- Default

impl Default for Intent {
    fn default() -> Self {
        Intent::new()
    }
}

