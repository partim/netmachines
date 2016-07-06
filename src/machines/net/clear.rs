//! Transport machines for unencrypted transports.

use std::marker::PhantomData;
use rotor::{EventSet, PollOpt, Response, Scope, Void};
use rotor::mio::tcp::TcpStream;
use rotor::mio::udp::UdpSocket;
use ::error::Error;
use ::handlers::TransportHandler;
use ::next::{Intent, Next};
use ::sync::Receiver;
use super::transport::TransportMachine;


//------------ TcpTransport -------------------------------------------------

pub struct TcpTransport<X, H: TransportHandler<TcpStream>> {
    sock: TcpStream,
    handler: H,
    intent: Intent,
    rx: Receiver<Next>,
    marker: PhantomData<X>
}

impl<X, H: TransportHandler<TcpStream>> TcpTransport<X, H> {
    fn new(sock: TcpStream, handler: H, rx: Receiver<Next>) -> Self {
        TcpTransport { sock: sock, handler: handler, intent: Intent::new(),
                       rx: rx, marker: PhantomData }
    }

    fn merge(&mut self, next: Next, scope: &mut Scope<X>) {
        self.intent = self.intent.merge(next, scope)
    }

    fn next<S>(self, scope: &mut Scope<X>) -> Response<Self, S> {
        if let Some(events) = self.intent.events() {
            match scope.reregister(&self.sock, events, PollOpt::level()) {
                Ok(_) => { }
                Err(err) => return Response::error(err.into())
            }
        }
        self.intent.response(self)
    }
}


//--- TransportMachine

impl<X, H> TransportMachine<X, TcpStream, H> for TcpTransport<X, H>
           where H: TransportHandler<TcpStream> {
    fn create(sock: TcpStream, mut handler: H, rx: Receiver<Next>,
              scope: &mut Scope<X>) -> Response<Self, Void> {
        let next = handler.on_start();
        let mut conn = TcpTransport::new(sock, handler, rx);
        conn.merge(next, scope);
        if let Some(events) = conn.intent.events() {
            match scope.register(&conn.sock, events, PollOpt::level()) {
                Ok(_) => { }
                Err(err) => return Response::error(err.into())
            }
        }
        conn.intent.response(conn)
    }

    fn ready<S>(mut self, events: EventSet, scope: &mut Scope<X>)
                -> Response<Self, S> {
        if events.is_error() {
            match self.sock.take_socket_error() {
                Ok(_) => { }
                Err(err) => {
                    let next = self.handler.on_error(err.into());
                    self.merge(next, scope);
                    if self.intent.is_close() {
                        return Response::done()
                    }
                }
            }
        }
        if events.is_readable() {
            let next = self.handler.on_read(&mut self.sock);
            self.merge(next, scope);
            if self.intent.is_close() {
                return Response::done()
            }
        }
        if events.is_writable() {
            let next = self.handler.on_write(&mut self.sock);
            self.merge(next, scope);
        }
        self.next(scope)
    }

    fn spawned<S>(self, _scope: &mut Scope<X>) -> Response<Self, S> {
        Response::ok(self)
    }

    fn timeout<S>(mut self, scope: &mut Scope<X>) -> Response<Self, S> {
        let next = self.handler.on_error(Error::Timeout);
        self.merge(next, scope);
        self.next(scope)
    }

    fn wakeup<S>(mut self, scope: &mut Scope<X>) -> Response<Self, S> {
        let mut intent = self.intent;
        while let Ok(next) = self.rx.try_recv() {
            intent = intent.merge(next, scope)
        };
        self.intent = intent;
        self.next(scope)
    }
}


//------------ UdpTransport -------------------------------------------------

pub struct UdpTransport<X, H: TransportHandler<UdpSocket>> {
    sock: UdpSocket,
    handler: H,
    intent: Intent,
    rx: Receiver<Next>,
    marker: PhantomData<X>
}

impl<X, H: TransportHandler<UdpSocket>> UdpTransport<X, H> {
    fn new(sock: UdpSocket, handler: H, rx: Receiver<Next>) -> Self {
        UdpTransport { sock: sock, handler: handler, intent: Intent::new(),
                       rx: rx, marker: PhantomData }
    }

    fn merge(&mut self, next: Next, scope: &mut Scope<X>) {
        self.intent = self.intent.merge(next, scope)
    }

    fn next<S>(self, scope: &mut Scope<X>) -> Response<Self, S> {
        if let Some(events) = self.intent.events() {
            match scope.reregister(&self.sock, events, PollOpt::level()) {
                Ok(_) => { }
                Err(err) => return Response::error(err.into())
            }
        }
        self.intent.response(self)
    }
}


//--- TransportMachine

impl<X, H> TransportMachine<X, UdpSocket, H> for UdpTransport<X, H>
           where H: TransportHandler<UdpSocket> {
    fn create(sock: UdpSocket, mut handler: H, rx: Receiver<Next>,
              scope: &mut Scope<X>) -> Response<Self, Void> {
        let next = handler.on_start();
        let mut conn = UdpTransport::new(sock, handler, rx);
        conn.merge(next, scope);
        if let Some(events) = conn.intent.events() {
            match scope.register(&conn.sock, events, PollOpt::level()) {
                Ok(_) => { }
                Err(err) => return Response::error(err.into())
            }
        }
        conn.intent.response(conn)
    }

    fn ready<S>(mut self, events: EventSet, scope: &mut Scope<X>)
                -> Response<Self, S> {
        if events.is_readable() {
            let next = self.handler.on_read(&mut self.sock);
            self.merge(next, scope);
            if self.intent.is_close() {
                return Response::done()
            }
        }
        if events.is_writable() {
            let next = self.handler.on_write(&mut self.sock);
            self.merge(next, scope);
        }
        self.next(scope)
    }

    fn spawned<S>(self, _scope: &mut Scope<X>) -> Response<Self, S> {
        Response::ok(self)
    }

    fn timeout<S>(mut self, scope: &mut Scope<X>) -> Response<Self, S> {
        let next = self.handler.on_error(Error::Timeout);
        self.merge(next, scope);
        self.next(scope)
    }

    fn wakeup<S>(mut self, scope: &mut Scope<X>) -> Response<Self, S> {
        let mut intent = self.intent;
        while let Ok(next) = self.rx.try_recv() {
            intent = intent.merge(next, scope)
        };
        self.intent = intent;
        self.next(scope)
    }
}

