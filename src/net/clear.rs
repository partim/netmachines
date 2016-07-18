//! Machines for unencrypted network sockets.

use std::marker::PhantomData;
use std::net::SocketAddr;
use rotor::{Compose2, EventSet, GenericScope, Machine, Response, Scope, Void};
use rotor::mio::tcp::{TcpListener, TcpStream};
use rotor::mio::udp::UdpSocket;
use super::machines::{ServerMachine, TransportMachine};
use ::handlers::{AcceptHandler, RequestHandler, TransportHandler};
use ::request::{RequestMachine, SeedFactory, TranslateError};
use ::utils::ResponseExt;
use ::sync::{DuctSender, TriggerSender};


//============ Transport Machines ============================================

//------------ TcpTransport --------------------------------------------------

/// The transport machine for unencrypted stream sockets.
///
/// This type is generic over the rotor context `X` and the transport
/// handler `H` which must accept [TcpStream] as its type argument.
///
/// The machine’s seed is a pair of a [TcpStream] and the handler’s seed.
///
/// You can add a machine to a loop before its start by using the
/// [new()](#method.new) function.
///
/// [TcpStream]: ../../../rotor/mio/tcp/struct.TcpStream.html
pub struct TcpTransport<X, H>(TransportMachine<X, TcpStream, H>)
           where H: TransportHandler<TcpStream>;

impl<X, H: TransportHandler<TcpStream>> TcpTransport<X, H> {
    /// Creates a new machine.
    ///
    /// The function takes a transport socket and a transport handler seed,
    /// as well as the scope for the new machine. It creates a new machine
    /// using this scope by calling the handler’s [create()] method.
    ///
    /// [create()]: ../../handlers/trait.TransportHandler.html#tymethod.create
    pub fn new<S: GenericScope>(sock: TcpStream, seed: H::Seed,
                                scope: &mut S) -> Response<Self, Void> {
        TransportMachine::new(sock, seed, scope).map_self(TcpTransport)
    }
}

impl<X, H: TransportHandler<TcpStream>> Machine for TcpTransport<X, H> {
    type Context = X;
    type Seed = (TcpStream, H::Seed);

    wrapped_machine!(TransportMachine, TcpTransport);
}


//------------ UdpTransport -------------------------------------------------

/// A transport machine for unencrypted datagram sockets.
///
/// The type is generic over the rotor context `X` and the transport
/// handler `H` which must accept [UdpSocket] as its type argument.
///
/// The machine’s seed is a pair of a [UdpSocket] and the handler’s seed.
///
/// You can add a machine to a loop before its start by using the
/// [new()](#method.new) function.
///
/// [UdpSocket]: ../../../rotor/mio/udp/struct.UdpSocket.html
pub struct UdpTransport<X, H>(TransportMachine<X, UdpSocket, H>)
           where H: TransportHandler<UdpSocket>;

impl<X, H: TransportHandler<UdpSocket>> UdpTransport<X, H> {
    /// Creates a new machine.
    ///
    /// The function takes a transport socket and a transport handler seed,
    /// as well as the scope for the new machine. It creates a new machine
    /// using this scope by calling the handler’s [create()] method.
    ///
    /// [create()]: ../../handlers/trait.TransportHandler.html#tymethod.create
    pub fn new<S: GenericScope>(sock: UdpSocket, seed: H::Seed,
                                scope: &mut S) -> Response<Self, Void> {
        TransportMachine::new(sock, seed, scope).map_self(UdpTransport)
    }
}

impl<X, H: TransportHandler<UdpSocket>> Machine for UdpTransport<X, H> {
    type Context = X;
    type Seed = (UdpSocket, H::Seed);

    wrapped_machine!(TransportMachine, UdpTransport);
}


//------------ TcpUdpTransport -----------------------------------------------

/// A transport machine for both unencrypted stream and datagram sockets.
///
/// The type is generic over the rotor context `X`, the transport handler for
/// stream sockets `TH` and for datagram sockets `UH`. The handlers types need
/// to be able to accept [TcpStream] and [UdpSocket] as their arguments, 
/// respectively.
///
/// Which transport a new machine is operating on is determined by the
/// machine’s seed. It uses the [TcpUdp] enum. If its `Tcp` variant is used
/// and contains a pair of a [TcpStream] and `TH`’s seed, the created
/// machine will be for a TCP transport. If the seed uses the `Udp` variant
/// with a pair of a [UdpSocket] and `UH`’s seed, a UDP transport will be
/// created.
///
/// There are two methods for creating a machine to add to a rotor loop before
/// its start, [new_tcp()](#method.new_tcp) and [new_udp()](#method.new_udp),
/// one for each flavor.
///
/// [TcpUdp]: enum.TcpUdp.html
/// [TcpStream]: ../../../rotor/mio/tcp/struct.TcpStream.html
/// [UdpSocket]: ../../../rotor/mio/udp/struct.UdpSocket.html
pub struct TcpUdpTransport<X, TH, UH>(TcpUdp<TcpTransport<X, TH>,
                                               UdpTransport<X, UH>>)
           where TH: TransportHandler<TcpStream>,
                 UH: TransportHandler<UdpSocket>;

/// # Machine Creation 
///
impl<X, TH, UH> TcpUdpTransport<X, TH, UH>
                where TH: TransportHandler<TcpStream>,
                      UH: TransportHandler<UdpSocket> {
    /// Creates a new machine for TCP transport.
    ///
    /// The machine will use the given socket, create a transport handler
    /// with the given seed, and will operate atop the given scope.
    pub fn new_tcp<S: GenericScope>(sock: TcpStream, seed: TH::Seed,
                                    scope: &mut S) -> Response<Self, Void> {
        TcpTransport::new(sock, seed, scope).map_self(TcpUdpTransport::from)
    }

    /// Creates a new machine for UDP transport.
    ///
    /// The machine will use the given socket, create a transport handler
    /// with the given seed, and will operate atop the given scope.
    pub fn new_udp<S: GenericScope>(sock: UdpSocket, seed: UH::Seed,
                                    scope: &mut S) -> Response<Self, Void> {
        UdpTransport::new(sock, seed, scope).map_self(TcpUdpTransport::from)
    }
}


//--- From

impl<X, TH, UH> From<TcpTransport<X, TH>> for TcpUdpTransport<X, TH, UH>
                where TH: TransportHandler<TcpStream>,
                      UH: TransportHandler<UdpSocket> {
    fn from(tcp: TcpTransport<X, TH>) -> Self {
        TcpUdpTransport(TcpUdp::Tcp(tcp))
    }
}

impl<X, TH, UH> From<UdpTransport<X, UH>> for TcpUdpTransport<X, TH, UH>
                where TH: TransportHandler<TcpStream>,
                      UH: TransportHandler<UdpSocket> {
    fn from(udp: UdpTransport<X, UH>) -> Self {
        TcpUdpTransport(TcpUdp::Udp(udp))
    }
}


//--- Machine

impl<X, TH, UH> Machine for TcpUdpTransport<X, TH, UH>
                where TH: TransportHandler<TcpStream>,
                      UH: TransportHandler<UdpSocket> {
    type Context = X;
    type Seed = TcpUdp<(TcpStream, TH::Seed), (UdpSocket, UH::Seed)>;

    fn create(seed: Self::Seed, scope: &mut Scope<X>)
              -> Response<Self, Void> {
        match seed {
            TcpUdp::Tcp(seed) => {
                TcpTransport::create(seed, scope)
                             .map_self(TcpUdpTransport::from)
            }
            TcpUdp::Udp(seed) => {
                UdpTransport::create(seed, scope)
                             .map_self(TcpUdpTransport::from)
            }
        }
    }

    fn ready(self, events: EventSet, scope: &mut Scope<X>)
             -> Response<Self, Self::Seed> {
        match self.0 {
            TcpUdp::Tcp(tcp) => {
                tcp.ready(events, scope)
                    .map(TcpUdpTransport::from, TcpUdp::Tcp)
            }
            TcpUdp::Udp(udp) => {
                udp.ready(events, scope)
                    .map(TcpUdpTransport::from, TcpUdp::Udp)
            }
        }
    }

    fn spawned(self, scope: &mut Scope<X>) -> Response<Self, Self::Seed> {
        match self.0 {
            TcpUdp::Tcp(tcp) => {
                tcp.spawned(scope).map(TcpUdpTransport::from, TcpUdp::Tcp)
            }
            TcpUdp::Udp(udp) => {
                udp.spawned(scope).map(TcpUdpTransport::from, TcpUdp::Udp)
            }
        }
    }

    fn timeout(self, scope: &mut Scope<Self::Context>)
               -> Response<Self, Self::Seed> {
        match self.0 {
            TcpUdp::Tcp(tcp) => {
                tcp.timeout(scope).map(TcpUdpTransport::from, TcpUdp::Tcp)
            }
            TcpUdp::Udp(udp) => {
                udp.timeout(scope).map(TcpUdpTransport::from, TcpUdp::Udp)
            }
        }
    }

    fn wakeup(self, scope: &mut Scope<Self::Context>)
              -> Response<Self, Self::Seed> {
        match self.0 {
            TcpUdp::Tcp(tcp) => {
                tcp.wakeup(scope).map(TcpUdpTransport::from, TcpUdp::Tcp)
            }
            TcpUdp::Udp(udp) => {
                udp.wakeup(scope).map(TcpUdpTransport::from, TcpUdp::Udp)
            }
        }
    }
}


//============ Server Machines ===============================================

//------------ TcpServer -----------------------------------------------------

/// A server machine for unencrypted stream sockets.
///
/// The type is generic over the rotor context `X` and an accept handler `H`
/// which implies a transport handler type for the created stream sockets
/// via its `H::Output` type.
///
/// One or more machines of this type should be added to the loop initially
/// with the [new()](#method.new) function. Whenever a new connection is
/// accepted by the accept handler’s [accept()] method, a new machine for
/// this connection is added to the loop on the fly.
///
/// [accept()]: ../../handlers/trait.AcceptHandler.html#tymethod.accept
pub struct TcpServer<X, H>(ServerMachine<X, TcpListener, H>)
           where H: AcceptHandler<TcpStream>;

/// # Machine Creation 
///
impl<X, H: AcceptHandler<TcpStream>> TcpServer<X, H> {
    /// Creates a new accept machine with the given socket and handler.
    ///
    /// Returns the rotor response for the new machine and a the sending
    /// side of a [trigger] that can be used to terminate the machine.
    ///
    /// [trigger]: ../../sync/fn.trigger.html
    pub fn new<S: GenericScope>(sock: TcpListener, handler: H, scope: &mut S)
                                -> (Response<Self, Void>, TriggerSender) {
        let (m, t) = ServerMachine::new(sock, handler, scope);
        (m.map_self(TcpServer), t)
    }
}

impl<X, H: AcceptHandler<TcpStream>> Machine for TcpServer<X, H> {
    type Context = X;
    type Seed = <ServerMachine<X, TcpListener, H> as Machine>::Seed;

    wrapped_machine!(ServerMachine, TcpServer);
}


//------------ TcpUdpServer -------------------------------------------------

/// A machine that combines a TCP server and a UDP transport.
///
/// The type is generic over the rotor context `X`, the accept handler for
/// the TCP server `AH`, and the transport handler for the UDP transport
/// `UH`. The transport handler for the TCP server is given implicitely
/// through `AH::Output`.
///
/// There are two methods for creating a machine to add to a rotor loop
/// before its start, [new_tcp()](#method.new_tcp) for the accept socket
/// of the TCP server and [new_udp()](#method.new_udp) for a new UDP
/// transport socket.
pub struct TcpUdpServer<X, AH, UH>(Compose2<TcpServer<X, AH>,
                                            UdpTransport<X, UH>>)
           where AH: AcceptHandler<TcpStream>,
                 UH: TransportHandler<UdpSocket>;

/// # Machine Creation 
///
impl<X, AH, UH> TcpUdpServer<X, AH, UH>
                where AH: AcceptHandler<TcpStream>,
                      UH: TransportHandler<UdpSocket> {
    /// Creates a new machine for an accept socket for the TCP server.
    ///
    /// The machine will use the given socket and accept handler and will
    /// operate atop the given scope.
    ///
    /// The function returns the rotor response and the sending end of a
    /// [trigger] that can be used to shut down the accept machine and close
    /// the socket.
    ///
    /// [trigger]: ../../sync/fn.trigger.html
    pub fn new_tcp<S>(sock: TcpListener, handler: AH, scope: &mut S)
                      -> (Response<Self, Void>, TriggerSender)
                   where S: GenericScope {
        let (m, t) = TcpServer::new(sock, handler, scope);
        (m.map_self(|m| TcpUdpServer(Compose2::A(m))), t)
    }

    /// Creates a new machine for a UDP transport socket.
    ///
    /// The machine will use the given socket and create a transport handler
    /// using the given seed. It will operate atop the provided context.
    ///
    /// There is no explicit way to end the machine and close the socket.
    /// This needs to be taken care of by the transport handler.
    pub fn new_udp<S: GenericScope>(sock: UdpSocket, seed: UH::Seed,
                                    scope: &mut S) -> Response<Self, Void> {
        UdpTransport::new(sock, seed, scope)
                  .map_self(|m| TcpUdpServer(Compose2::B(m)))
    }
}
                
impl<X, AH, UH> Machine for TcpUdpServer<X, AH, UH>
                where AH: AcceptHandler<TcpStream>,
                      UH: TransportHandler<UdpSocket> {
    type Context = X;
    type Seed = <Compose2<TcpServer<X, AH>,
                          UdpTransport<X, UH>> as Machine>::Seed;

    wrapped_machine!(Compose2, TcpUdpServer);
}


//============ Client Machines ===============================================

//------------ TcpClient ----------------------------------------------------

/// A client machine for unencrypted stream sockets.
///
/// The type is generic over the rotor context `X`, a request handler `RH`,
/// and a transport handler `TH` that needs to accept a [TcpStream] as its
/// type argument.
///
/// The request handler must output a pair of a socket address and the
/// transport handler’s seed. The machine will try to connect that address
/// and, if it succeeds, will create a transport machine for that socket
/// using the seed.
///
/// The client machine is in fact a [RequestMachine] wrapping a
/// [TcpTransport]. That is, it can either be a request handling machine or
/// a TCP transport machine. The former variant is explicitely created using
/// the [new()](#method.new) function. It will remain alive while there are
/// still copies of the sending end of its request [duct] alive.
///
/// Machines of the transport variant are created by the request handler as
/// needed.
///
/// [TcpStream]: ../../../rotor/mio/tcp/struct.TcpStream.html
/// [TcpTransport]: struct.TcpTransport.html
/// [duct]: ../../sync/fn.duct.html
pub struct TcpClient<X, RH, TH>(RequestMachine<X, TcpTransport<X, TH>, RH,
                                               TcpFactory<TH::Seed>>)
    where RH: RequestHandler<Output=(SocketAddr, TH::Seed)>,
          TH: TransportHandler<TcpStream>;

/// # Machine Creation 
///
impl<X, RH, TH> TcpClient<X, RH, TH>
                where RH: RequestHandler<Output=(SocketAddr, TH::Seed)>,
                      TH: TransportHandler<TcpStream> {
    /// Creates a new request machine for the TCP client.
    ///
    /// The machine will use the given handler and operate atop the given
    /// scope.
    ///
    /// The function returns a rotor response and the sending end of a
    /// [duct] for dispatching requests to the new machine. The machine will
    /// remain alive for as long as this duct remains alive, ie., as long as
    /// someone sill owns a copy of the returned sending end.
    pub fn new<S>(handler: RH, scope: &mut S)
                  -> (Response<Self, Void>, DuctSender<RH::Request>)
               where S: GenericScope {
        let (m, tx) = RequestMachine::new(handler, TcpFactory::new(), scope);
        (m.map_self(TcpClient), tx)
    }
}

//--- Machine

impl<X, RH, TH> Machine for TcpClient<X, RH, TH>
                where RH: RequestHandler<Output=(SocketAddr, TH::Seed)>,
                      TH: TransportHandler<TcpStream> {
    type Context = X;
    type Seed = (TcpStream, TH::Seed);

    wrapped_machine!(RequestMachine, TcpClient);
}


//------------ UdpClient ----------------------------------------------------

pub struct UdpClient<X, RH, TH>(RequestMachine<X, UdpTransport<X, TH>,
                                               RH, UdpFactory<TH::Seed>>)
                     where RH: RequestHandler<Output=(SocketAddr, TH::Seed)>,
                           TH: TransportHandler<UdpSocket>;

impl<X, RH, TH> UdpClient<X, RH, TH>
                where RH: RequestHandler<Output=(SocketAddr, TH::Seed)>,
                      TH: TransportHandler<UdpSocket> {
    pub fn new<S>(handler: RH, scope: &mut S)
                  -> (Response<Self, Void>, DuctSender<RH::Request>)
               where S: GenericScope {
        let (m, tx) = RequestMachine::new(handler, UdpFactory::new(), scope);
        (m.map_self(UdpClient), tx)
    }
}

impl<X, RH, TH> Machine for UdpClient<X, RH, TH>
                where RH: RequestHandler<Output=(SocketAddr, TH::Seed)>,
                      TH: TransportHandler<UdpSocket> {
    type Context = X;
    type Seed = (UdpSocket, TH::Seed);

    wrapped_machine!(RequestMachine, UdpClient);
}


//------------ TcpUdpClient -------------------------------------------------

pub struct TcpUdpClient<X, RH, TH, UH>(RequestMachine<
                                               X, TcpUdpTransport<X, TH, UH>,
                                               RH, TcpUdpFactory<TH::Seed,
                                                                   UH::Seed>>)
            where RH: RequestHandler<Output=TcpUdp<(SocketAddr, TH::Seed),
                                                     (SocketAddr, UH::Seed)>>,
                  TH: TransportHandler<TcpStream>,
                  UH: TransportHandler<UdpSocket>;

impl<X, RH, TH, UH> TcpUdpClient<X, RH, TH, UH>
            where RH: RequestHandler<Output=TcpUdp<(SocketAddr, TH::Seed),
                                                     (SocketAddr, UH::Seed)>>,
                  TH: TransportHandler<TcpStream>,
                  UH: TransportHandler<UdpSocket> {
    pub fn new<S>(handler: RH, scope: &mut S)
                  -> (Response<Self, Void>, DuctSender<RH::Request>)
               where S: GenericScope {
        let (m, tx) = RequestMachine::new(handler, TcpUdpFactory::new(),
                                          scope);
        (m.map_self(TcpUdpClient), tx)
    }
}

impl<X, RH, TH, UH> Machine for TcpUdpClient<X, RH, TH, UH>
            where RH: RequestHandler<Output=TcpUdp<(SocketAddr, TH::Seed),
                                                     (SocketAddr, UH::Seed)>>,
                  TH: TransportHandler<TcpStream>,
                  UH: TransportHandler<UdpSocket> {
    type Context = X;
    type Seed = TcpUdp<(TcpStream, TH::Seed), (UdpSocket, UH::Seed)>;

    wrapped_machine!(RequestMachine, TcpUdpClient);
}


//============ Socket Factories ==============================================

//------------ TcpFactory ----------------------------------------------------

pub struct TcpFactory<S>(PhantomData<S>);

impl<S> TcpFactory<S> {
    fn new() -> Self { TcpFactory(PhantomData) }
}

impl<S> SeedFactory<(SocketAddr, S), (TcpStream, S)> for TcpFactory<S> {
    fn translate(&self, output: (SocketAddr, S))
                 -> Result<(TcpStream, S), TranslateError<(SocketAddr, S)>> {
        let (addr, seed) = output;
        match TcpStream::connect(&addr) {
            Ok(sock) => Ok((sock, seed)),
            Err(err) => Err(TranslateError((addr, seed), err.into()))
        }
    }
}


//------------ UdpFactory ---------------------------------------------------

struct UdpFactory<S>(PhantomData<S>);

impl<S> UdpFactory<S> {
    fn new() -> Self { UdpFactory(PhantomData) }
}

impl<S> SeedFactory<(SocketAddr, S), (UdpSocket, S)> for UdpFactory<S> {
    fn translate(&self, output: (SocketAddr, S))
                 -> Result<(UdpSocket, S), TranslateError<(SocketAddr, S)>> {
        let (addr, seed) = output;
        match UdpSocket::bound(&addr) {
            Ok(sock) => Ok((sock, seed)),
            Err(err) => Err(TranslateError((addr, seed), err.into()))
        }
    }
}


//------------ TcpUdpFactory ------------------------------------------------

struct TcpUdpFactory<TS, US>(PhantomData<(TS, US)>);

impl<TS, US> TcpUdpFactory<TS, US> {
    fn new() -> Self { TcpUdpFactory(PhantomData) }
}

impl<TS, US> SeedFactory<TcpUdp<(SocketAddr, TS), (SocketAddr, US)>,
                         TcpUdp<(TcpStream, TS), (UdpSocket, US)>>
             for TcpUdpFactory<TS, US> {
    fn translate(&self, output: TcpUdp<(SocketAddr, TS), (SocketAddr, US)>)
                 -> Result<TcpUdp<(TcpStream, TS), (UdpSocket, US)>,
                           TranslateError<TcpUdp<(SocketAddr, TS),
                                          (SocketAddr, US)>>> {
        use self::TcpUdp::*;

        match output {
            Tcp((addr, seed)) => {
                match TcpStream::connect(&addr) {
                    Ok(sock) => Ok(Tcp((sock, seed))),
                    Err(err) => Err(TranslateError(Tcp((addr, seed)),
                                                   err.into()))
                }
            }
            Udp((addr, seed)) => {
                match UdpSocket::bound(&addr) {
                    Ok(sock) => Ok(Udp((sock, seed))),
                    Err(err) => Err(TranslateError(Udp((addr, seed)),
                                                   err.into()))
                }
            }
        }
    }
}


//============ Composition Types =============================================

//------------ TcpUdp ------------------------------------------------------

pub enum TcpUdp<T, U> {
    Tcp(T),
    Udp(U)
}

