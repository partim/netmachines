//! Synchronization.

use std::mem;
use std::ops::DerefMut;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, TryRecvError}; 
use rotor::{Notifier, WakeupError};

pub use std::sync::mpsc::{RecvError, SendError};


//------------ Channel ------------------------------------------------------

pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
    let (tx, rx) = mpsc::channel();
    (Sender(tx), Receiver(rx))
}

#[derive(Debug)]
pub struct Sender<T>(mpsc::Sender<T>);

impl<T> Sender<T> {
    pub fn send(&self, t: T) -> Result<(), SendError<T>> {
        self.0.send(t)
    }
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        Sender(self.0.clone())
    }
}

#[derive(Debug)]
pub struct Receiver<T>(mpsc::Receiver<T>);

impl<T> Receiver<T> {
    pub fn recv(&self) -> Result<T, RecvError> {
        self.0.recv()
    }

    pub fn try_recv(&self) -> Result<Option<T>, RecvError> {
        match self.0.try_recv() {
            Ok(t) => Ok(Some(t)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => Err(RecvError)
        }
    }

    pub fn iter(&self) -> mpsc::Iter<T> {
        self.0.iter()
    }
}


//------------ Duct ----------------------------------------------------------

pub fn duct<T>(notifier: Notifier) -> (DuctSender<T>, DuctReceiver<T>) {
    let awake = Arc::new(AtomicBool::new(false));
    let (tx, rx) = mpsc::channel();
    (DuctSender { awake: awake.clone(), notifier: notifier, tx: tx },
     DuctReceiver { awake: awake, rx: rx })
}

pub struct DuctSender<T> {
    awake: Arc<AtomicBool>,
    notifier: Notifier,
    tx: mpsc::Sender<T>
}

impl<T: Send> DuctSender<T> {
    pub fn send(&self, value: T) -> Result<(), DuctSendError<T>> {
        try!(self.tx.send(value));
        if !self.awake.swap(true, Ordering::SeqCst) {
            try!(self.notifier.wakeup());
        }
        Ok(())
    }
}

impl<T> Clone for DuctSender<T> {
    fn clone(&self) -> Self {
        DuctSender {
            awake: self.awake.clone(),
            notifier: self.notifier.clone(),
            tx: self.tx.clone()
        }
    }
}

pub struct DuctReceiver<T> {
    awake: Arc<AtomicBool>,
    rx: mpsc::Receiver<T>
}

impl<T: Send> DuctReceiver<T> {
    pub fn try_recv(&self) -> Result<Option<T>, RecvError> {
        self.awake.store(false, Ordering::Relaxed);
        match self.rx.try_recv() {
            Ok(t) => Ok(Some(t)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => Err(RecvError)
        }
    }
}


//------------ DuctSendError -------------------------------------------------

#[derive(Debug)]
pub enum DuctSendError<T> {
    SendError(T),
    WakeupError,
}

impl<T> From<SendError<T>> for DuctSendError<T> {
    fn from(e: SendError<T>) -> DuctSendError<T> {
        DuctSendError::SendError(e.0)
    }
}

impl<T> From<WakeupError> for DuctSendError<T> {
    fn from(_: WakeupError) -> DuctSendError<T> {
        DuctSendError::WakeupError
    }
}


//------------ Gate ---------------------------------------------------------

pub fn gate<T>(notifier: Notifier) -> (GateSender<T>, GateReceiver<T>) {
    let item = Arc::new(Mutex::new(None));
    (GateSender { item: item.clone(), notifier: notifier },
     GateReceiver(item))
}

pub struct GateSender<T> {
    item: Arc<Mutex<Option<T>>>,
    notifier: Notifier
}

impl<T: Send> GateSender<T> {
    pub fn send(self, value: T) -> Result<(), GateSendError<T>> {
        match self.item.lock() {
            Ok(mut guard) => {
                let _ = mem::replace(guard.deref_mut(), Some(value));
                try!(self.notifier.wakeup());
                Ok(())
            }
            Err(_) => Err(GateSendError::Poisoned(value))
        }
    }
}


pub enum GateSendError<T> {
    Poisoned(T),
    WakeupError,
}

impl<T> From<WakeupError> for GateSendError<T> {
    fn from(_: WakeupError) -> GateSendError<T> {
        GateSendError::WakeupError
    }
}


pub struct GateReceiver<T>(Arc<Mutex<Option<T>>>);

impl<T: Send> GateReceiver<T> {
    pub fn try_get(&self) -> Result<Option<T>, GateRecvError> {
        match self.0.lock() {
            Ok(mut guard) => {
                match mem::replace(guard.deref_mut(), None) {
                    Some(t) => Ok(Some(t)),
                    None => Ok(None)
                }
            }
            Err(_) => Err(GateRecvError)
        }
    }
}

pub struct GateRecvError;


//------------ Trigger ------------------------------------------------------

pub fn trigger(notifier: Notifier) -> (TriggerSender, TriggerReceiver) {
    let flag = Arc::new(AtomicBool::new(false));
    (TriggerSender { flag: flag.clone(), notifier: notifier },
     TriggerReceiver(flag))
}

#[derive(Clone)]
pub struct TriggerSender {
    flag: Arc<AtomicBool>,
    notifier: Notifier
}

impl TriggerSender {
    pub fn trigger(&self) -> Result<(), WakeupError> {
        if !self.flag.swap(true, Ordering::SeqCst) {
            try!(self.notifier.wakeup());
        }
        Ok(())
    }
}

pub struct TriggerReceiver(Arc<AtomicBool>);

impl TriggerReceiver {
    pub fn triggered(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

