//! Next.

use std::cmp::min;
use std::fmt;
use std::time::Duration;
use rotor::{EventSet, GenericScope, Time};


//------------ Next ---------------------------------------------------------

#[must_use]
#[derive(Clone)]
pub struct Next<T> {
    interest: Option<(Interest, T)>,
    timeout: Option<Duration>,
}


impl<T> Next<T> {
    fn new(interest: Interest, t: T) -> Self {
        Next { interest: Some((interest, t)), timeout: None }
    }

    pub fn wait(t: T) -> Self { Next::new(Interest::Wait, t) }

    pub fn read(t: T) -> Self { Next::new(Interest::Read, t) }
    
    pub fn write(t: T) -> Self { Next::new(Interest::Write, t) }
    
    pub fn read_and_write(t: T) -> Self { Next::new(Interest::ReadWrite, t) }
    
    pub fn remove() -> Self { Next { interest: None, timeout: None } }

    pub fn timeout(mut self, duration: Duration) -> Self {
        self.timeout = Some(duration);
        self
    }
}

impl<T> Next<T> {
    pub fn map<U, F>(self, op: F) -> Next<U>
           where F: FnOnce(T) -> U {
        Next {
            interest: self.interest.map(|(i, t)| (i, op(t))),
            timeout: self.timeout
        }
    }
}


//--- Debug

impl<T> fmt::Debug for Next<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if let Some((interest, _)) = self.interest {
            try!(write!(f, "Next::{:?}", interest));
        }
        else {
            try!(write!(f, "Next::Remove"));
        }
        match self.timeout {
            Some(ref d) => write!(f, "({:?})", d),
            None => Ok(())
        }
    }
}


//------------ Interest -----------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
enum Interest {
    Wait,
    Read,
    Write,
    ReadWrite
}


//------------ Intent -------------------------------------------------------

#[derive(Clone, Copy, Debug)]
pub struct Intent {
    interest: Interest,
    deadline: Option<Time>
}

impl Intent {
    fn make(interest: Interest, deadline: Option<Time>) -> Self {
        Intent { interest: interest, deadline: deadline }
    }

    pub fn new<T, S: GenericScope>(next: Next<T>, scope: &mut S)
                                   -> Option<(Self, T)> {
        use self::Interest::*;

        let dl = next.timeout.map(|dur| scope.now() + dur);
        match next.interest {
            Some((Wait, t)) => Some((Intent::make(Wait, dl), t)),
            Some((Read, t)) => Some((Intent::make(Read, dl), t)),
            Some((Write, t)) => Some((Intent::make(Write, dl), t)),
            Some((ReadWrite, t)) => Some((Intent::make(ReadWrite, dl), t)),
            None => None
        }
    }

    pub fn merge<T, S: GenericScope>(self, other: Next<T>, scope: &mut S)
                                     -> Option<(Self, T)> {
        use self::Interest::*;

        if let Some((interest, t)) = other.interest {
            let interest = match (self.interest, interest) {
                (ReadWrite, _) | (_, ReadWrite) |
                (Read, Write) | (Write, Read) => ReadWrite,
                (Read, _) | (_, Read) => Read,
                (Write, _) | (_, Write) => Write,
                (Wait, Wait) => Wait
            };
            let deadline = match (self.deadline, other.timeout) {
                (Some(deadline), Some(timeout)) => {
                    Some(min(deadline, scope.now() + timeout))
                }
                (None, Some(timeout)) => Some(scope.now() + timeout),
                (deadline, None) => deadline
            };
            Some((Intent::make(interest, deadline), t))
        }
        else {
            None
        }
    }

    pub fn deadline(&self) -> Option<Time> {
        self.deadline
    }

    /// Returns the events for self.
    pub fn events(&self) -> EventSet {
        match self.interest {
            Interest::Wait => EventSet::none(),
            Interest::Read => EventSet::readable(),
            Interest::Write => EventSet::writable(),
            Interest::ReadWrite => {
                EventSet::readable() | EventSet::writable()
            }
        }
    }
}

