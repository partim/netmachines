//! Synchronization.

use std::error;
use std::fmt;
use std::sync::{Arc, mpsc};
use std::sync::atomic::{AtomicBool, Ordering};
use rotor::{Notifier, WakeupError};
use super::next::Next;

pub use std::sync::mpsc::TryRecvError;

//------------ Freestanding Functions ---------------------------------------

pub fn channel<T>(notify: Notifier) -> (Sender<T>, Receiver<T>) {
    let awake = Arc::new(AtomicBool::new(false));
    let (tx, rx) = mpsc::channel();
    (Sender::new(awake.clone(), notify, tx), Receiver::new(awake.clone(), rx))
}

pub fn ctrl_channel(notify: Notifier) -> (Control, Receiver<Next>) {
    let (tx, rx) = channel(notify);
    (Control { tx: tx }, rx)
}


//------------ Sender -------------------------------------------------------

pub struct Sender<T> {
    awake: Arc<AtomicBool>,
    notify: Notifier,
    tx: mpsc::Sender<T>,
}

impl<T> Sender<T> {
    fn new(awake: Arc<AtomicBool>, notify: Notifier, tx: mpsc::Sender<T>)
           -> Self {
        Sender { awake: awake, notify: notify, tx: tx }
    }
}

impl<T: Send> Sender<T> {
    pub fn send(&self, value: T) -> Result<(), SendError<T>> {
        try!(self.tx.send(value));
        if !self.awake.swap(true, Ordering::SeqCst) {
            try!(self.notify.wakeup());
        }
        Ok(())
    }
}

impl Sender<Next> {
    pub fn ctrl(&self) -> Control {
        Control { tx: self.clone() }
    }
}


//--- Clone

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Sender<T> {
        Sender {
            awake: self.awake.clone(),
            notify: self.notify.clone(),
            tx: self.tx.clone()
        }
    }
}


//--- Debug

impl<T> fmt::Debug for Sender<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Sender")
            .field("notify", &self.notify)
            .finish()
    }
}


//------------ Receiver -----------------------------------------------------

pub struct Receiver<T> {
    awake: Arc<AtomicBool>,
    rx: mpsc::Receiver<T>
}

impl<T> Receiver<T> {
    fn new(awake: Arc<AtomicBool>, rx: mpsc::Receiver<T>) -> Self {
        Receiver { awake: awake, rx: rx }
    }
}

impl<T: Send> Receiver<T> {
    pub fn try_recv(&self) -> Result<T, TryRecvError> {
        self.awake.store(false, Ordering::Relaxed);
        self.rx.try_recv()
    }
} 


//------------ SendError ----------------------------------------------------

#[derive(Debug)]
pub struct SendError<T>(pub Option<T>);

impl<T> From<mpsc::SendError<T>> for SendError<T> {
    fn from(e: mpsc::SendError<T>) -> SendError<T> {
        SendError(Some(e.0))
    }
}

impl<T> From<WakeupError> for SendError<T> {
    fn from(_: WakeupError) -> SendError<T> {
        SendError(None)
    }
}


//------------ Control ------------------------------------------------------

#[derive(Clone, Debug)]
pub struct Control {
    tx: Sender<Next>
}

impl Control {
    pub fn ready(&self, next: Next) -> Result<(), ControlError> {
        self.tx.send(next).map_err(|_| ControlError(()))
    }
}


//------------ ControlError -------------------------------------------------

#[derive(Debug)]
pub struct ControlError(());

impl error::Error for ControlError {
    fn description(&self) -> &str {
        "Cannot wakeup event loop: loop is closed"
    }
}

impl fmt::Display for ControlError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(error::Error::description(self))
    }
}

