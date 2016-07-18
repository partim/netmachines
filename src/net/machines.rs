//! Fundamental machines for networked sockets.
//!
//! This module defines two generic types that can be resused for specific
//! transport and server machines. There is no generic client machine; 
//! it is already provided by the top-level [RequestMachine].
//!
//! These machines are not intended to be used directly but rather are the
//! building blocks for the concrete machines provided the [net] module.
//!
//! [net]: ../index.html
//! [RequestMachine]: ../../request/struct.RequestMachine.html

use std::marker::PhantomData;
use rotor::{EventSet, GenericScope, Machine, PollOpt, Response, Scope, Void};
use ::error::Error;
use ::handlers::{AcceptHandler, TransportHandler};
use ::next::Intent;
use ::sockets::{Accept, Blocked, Transport};
use ::sync::{TriggerReceiver, TriggerSender, trigger};
use ::utils::ResponseExt;


//------------ TransportMachine ----------------------------------------------

/// A machine combining a transport socket and a transport handler.
///
/// The type is generic over the rotor context `X`, the transport socket
/// type `T`, and the transport handler type `H`.
///
/// Machine can be created either during loop creating using the
/// [new()](#method.new) function or, when the type is used in combined
/// machines, during the [Machine::create()] method. The seed for this case
/// is a pair of the new transport socket and the transport handler’s seed.
///
/// [Machine::create()]: ../../../rotor/trait.Machine.html#tymethod.create
pub struct TransportMachine<X, T: Transport, H: TransportHandler<T>> {
    /// The transport socket.
    sock: T,

    /// The transport handler.
    handler: H,

    /// The handler’s last intent. 
    intent: Intent,

    /// Binding the context.
    marker: PhantomData<X>
}

/// # Machine Creation
///
impl<X, T: Transport, H: TransportHandler<T>> TransportMachine<X, T, H> {
    /// Creates a new machine.
    ///
    /// The function takes a transport socket and a transport handler seed,
    /// as well as the scope for the new machine. It creates a new machine
    /// using this scope by calling the handler’s [create()] method.
    ///
    /// The return value is the one expected by the `add_machine_with()`
    /// functions of [LoopCreator] and [LoopInstance].
    ///
    /// [create()]: ../../handlers/trait.TransportHandler.html#tymethod.create
    /// [LoopCreator]: ../../../rotor/struct.LoopCreator.html
    /// [LoopInstance]: ../../../rotor/struct.LoopInstance.html
    pub fn new<S: GenericScope>(mut sock: T, seed: H::Seed, scope: &mut S)
                                -> Response<Self, Void> {
        let next = H::create(seed, &mut sock, scope.notifier());
        if let Some((intent, handler)) = Intent::new(next, scope) {
            let conn = TransportMachine::make(sock, handler, intent);
            match scope.register(&conn.sock, conn.intent.events(),
                                 PollOpt::level()) {
                Ok(_) => { }
                Err(err) => return Response::error(err.into())
            }
            conn.response()
        }
        else {
            Response::done()
        }
    }
}

/// # Internal Helpers
///
impl<X, T: Transport, H: TransportHandler<T>> TransportMachine<X, T, H> {
    /// Creates a new object from its parts.
    ///
    /// Sadly, `new()` is already taken …
    fn make(sock: T, handler: H, intent: Intent) -> Self {
        TransportMachine {
            sock: sock,
            handler: handler,
            intent: intent,
            marker: PhantomData
        }
    }

    /// Performs the final steps in successful event handling.
    ///
    /// Reregisters for the correct events depending on the socket’s
    /// blocked state and the handler’s interests and generates the
    /// correct response.
    fn next<S>(self, scope: &mut Scope<X>) -> Response<Self, S> {
        let events = match self.sock.blocked() {
            Some(Blocked::Read) => EventSet::readable(),
            Some(Blocked::Write) => EventSet::writable(),
            None => self.intent.events()
        };
        match scope.reregister(&self.sock, events, PollOpt::level()) {
            Ok(_) => { }
            Err(err) => return Response::error(err.into())
        }
        self.response()
    }

    /// Generates the correct response for this machine.
    ///
    /// This is a `Response::ok()` in any case, but may have a deadline
    /// attached.
    fn response<S>(self) -> Response<Self, S> {
        if let Some(deadline) = self.intent.deadline() {
            Response::ok(self).deadline(deadline)
        }
        else {
            Response::ok(self)
        }
    }
}


//--- Machine

impl<X, T, H> Machine for TransportMachine<X, T, H>
              where T: Transport, H: TransportHandler<T> {
    type Context = X;

    /// Our seed is a pair of the new socket and the handler’s seed.
    type Seed = (T, H::Seed);

    fn create(seed: Self::Seed, scope: &mut Scope<X>)
              -> Response<Self, Void> {
        TransportMachine::new(seed.0, seed.1, scope)
    }

    fn ready(mut self, events: EventSet, scope: &mut Scope<X>)
                -> Response<Self, Self::Seed> {
        if events.is_error() {
            if let Err(err) = self.sock.take_socket_error() {
                let next = self.handler.error(err.into());
                if let Some((intent, handler)) = self.intent.merge(next,
                                                                   scope) {
                    return TransportMachine::make(self.sock, handler, intent)
                                            .next(scope);
                }
                else {
                    return Response::done()
                }
            }
        }

        // If the socket is blocked, we pretent the events are actually those
        // the handler has requested so the socket is read from or written to
        // and can become unblocked. (If the handler’s request was for wait,
        // then what are we doing here in the first place?)
        let events = if let Some(_) = self.sock.blocked() {
            self.intent.events()
        } else {
            events
        };

        if events.is_readable() {
            let next = self.handler.readable(&mut self.sock);
            if let Some((intent, handler)) = self.intent.merge(next, scope) {
                self = TransportMachine::make(self.sock, handler, intent)
            }
            else {
                return Response::done()
            }
        }

        if events.is_writable() {
            let next = self.handler.writable(&mut self.sock);
            if let Some((intent, handler)) = self.intent.merge(next, scope) {
                self = TransportMachine::make(self.sock, handler, intent)
            }
            else {
                return Response::done()
            }
        }
        self.next(scope)
    }

    fn spawned(self, _scope: &mut Scope<X>) -> Response<Self, Self::Seed> {
        Response::ok(self)
    }

    fn timeout(self, scope: &mut Scope<X>) -> Response<Self, Self::Seed> {
        let next = self.handler.error(Error::Timeout);
        if let Some((intent, handler)) = self.intent.merge(next, scope) {
            TransportMachine::make(self.sock, handler, intent).next(scope)
        }
        else {
            Response::done()
        }
    }

    fn wakeup(self, scope: &mut Scope<X>) -> Response<Self, Self::Seed> {
        let next = self.handler.wakeup();
        if let Some((intent, handler)) = self.intent.merge(next, scope) {
            TransportMachine::make(self.sock, handler, intent).next(scope)
        }
        else {
            Response::done()
        }
    }
}


//------------ ServerMachine ------------------------------------------------

/// A server machine for a stream transport.
///
/// The type is generic over the rotor context `X`, the accept socket type
/// `A` (which implies the transport socket type through `A::Output`), and
/// the accept handler type `H` (which implies the transport handler type
/// through `H::Output`).
///
/// The machine comes in two flavors. Either it consists of an accept socket
/// and the accept handler or it wraps a transport machine for the implied
/// transport type and handler. The first flavor calls the accept handler
/// for every new connection request on the accept socket and, if the handler
/// returns `Some(_)`thing creates a new machine of the second flavor.
///
/// Typically, you will create one or more machines of the accept flavor
/// during loop creating using the [new()](#method.new) function. If you need
/// to create and close accept sockets on the fly, you should wrap the server
/// machine into a [RequestMachine].
///
/// [RequestMachine]: ../../request/struct.RequestMachine.html
pub struct ServerMachine<X, A, H>(
    ServerInner<A, H, TransportMachine<X, A::Output, H::Output>>,
    PhantomData<X>
) where A: Accept, H: AcceptHandler<A::Output>;


/// The two flavors of a server machine.
enum ServerInner<A, H, M> {
    /// Accept socket and handler.
    ///
    /// Never mind the use of term ‘listener’ here …
    Lsnr(ServerListener<A, H>),

    /// A wrapped transport machine.
    Conn(M)
}

/// All we need for a listenig flavor machine.
struct ServerListener<A, H> {
    /// The accept socket.
    sock: A,

    /// The accept handler.
    handler: H,

    /// The receiving end of a trigger for shutting down the machine.
    rx: TriggerReceiver
}


/// # Machine Creation
///
impl<X, A: Accept, H: AcceptHandler<A::Output>> ServerMachine<X, A, H> {
    /// Creates a new machine.
    ///
    /// More specifically, it creates a machine of the accept flavor using
    /// the provided accept socket and accept handler atop the given scope.
    ///
    /// Returns a response to be passed to rotor and the sending end of a
    /// [trigger] that can be used to shut down the machine later and close
    /// the accept socket later.
    ///
    /// Note that the response may be an error, in which case calling
    /// `is_stopped()` on it will return true. While this is relatively
    /// unlikely, it may happen.
    pub fn new<S: GenericScope>(sock: A, handler: H, scope: &mut S)
                                -> (Response<Self, Void>, TriggerSender) {
        let (tx, rx) = trigger(scope.notifier());
        match scope.register(&sock, EventSet::readable(), PollOpt::level()) {
            Ok(()) => {
                let lsnr = ServerListener { sock: sock, handler: handler,
                                            rx: rx };
                (Response::ok(ServerMachine::lsnr(lsnr)), tx)
            }
            Err(err) => (Response::error(err.into()), tx),
        }
    }
}


/// # Internal Helpers
/// 
impl<X, A: Accept, H: AcceptHandler<A::Output>> ServerMachine<X, A, H> {
    /// Creates an accept flavor value.
    fn lsnr(lsnr: ServerListener<A, H>) -> Self {
        ServerMachine(ServerInner::Lsnr(lsnr), PhantomData)
    }

    /// Creates a connection flavor value.
    fn conn(conn: TransportMachine<X, A::Output, H::Output>)
            -> Self {
        ServerMachine(ServerInner::Conn(conn), PhantomData)
    }

    /// Accepts a new connection request.
    ///
    /// If a call to [Accept::accept()] fails, simply logs the error and
    /// moves on. Alternatively, we could adda  
    fn accept(mut lsnr: ServerListener<A, H>)
              -> Response<Self, <Self as Machine>::Seed> {
        match lsnr.sock.accept() {
            Ok(Some((sock, addr))) => {
                if let Some(seed) = lsnr.handler.accept(&addr) {
                    Response::spawn(ServerMachine::lsnr(lsnr), (sock, seed))
                }
                else {
                    Response::ok(ServerMachine::lsnr(lsnr))
                }
            }
            Ok(None) => {
                Response::ok(ServerMachine::lsnr(lsnr))
            }
            Err(err) => {
                match lsnr.handler.error(err.into()) {
                    Ok(()) => Response::ok(ServerMachine::lsnr(lsnr)),
                    Err(()) => Response::done()
                }
            }
        }
    }
}


//--- Machine

impl<X, A, H> Machine for ServerMachine<X, A, H>
              where A: Accept, H: AcceptHandler<A::Output> {
    type Context = X;
    type Seed = (A::Output, <H::Output as TransportHandler<A::Output>>::Seed);

    fn create(seed: Self::Seed, scope: &mut Scope<X>)
              -> Response<Self, Void> {
        TransportMachine::create(seed, scope).map_self(ServerMachine::conn)
    }

    fn ready(self, events: EventSet, scope: &mut Scope<X>)
             -> Response<Self, Self::Seed> {
        match self.0 {
            ServerInner::Lsnr(lsnr) => {
                ServerMachine::accept(lsnr)
            }
            ServerInner::Conn(conn) => {
                conn.ready(events, scope).map_self(ServerMachine::conn)
            }
        }
    }

    fn spawned(self, scope: &mut Scope<X>) -> Response<Self, Self::Seed> {
        match self.0 {
            ServerInner::Lsnr(lsnr) => {
                ServerMachine::accept(lsnr)
            }
            ServerInner::Conn(conn) => {
                conn.spawned(scope).map_self(ServerMachine::conn)
            }
        }
    }

    fn timeout(self, scope: &mut Scope<X>) -> Response<Self, Self::Seed> {
        match self.0 {
            ServerInner::Lsnr(_) => unreachable!("listener can’t timeout"),
            ServerInner::Conn(conn) => {
                conn.timeout(scope).map_self(ServerMachine::conn)
            }
        }
    }

    fn wakeup(self, scope: &mut Scope<X>) -> Response<Self, Self::Seed> {
        match self.0 {
            ServerInner::Lsnr(lsnr) => {
                if lsnr.rx.triggered() {
                    Response::done()
                }
                else {
                    Response::ok(ServerMachine::lsnr(lsnr))
                }
            }
            ServerInner::Conn(conn) => {
                conn.wakeup(scope).map_self(ServerMachine::conn)
            }
        }
    }
}

