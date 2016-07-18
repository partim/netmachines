//! Unencrypted machines.

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

pub struct TcpTransport<X, H>(TransportMachine<X, TcpStream, H>)
           where H: TransportHandler<TcpStream>;

impl<X, H: TransportHandler<TcpStream>> TcpTransport<X, H> {
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

pub struct UdpTransport<X, H>(TransportMachine<X, UdpSocket, H>)
           where H: TransportHandler<UdpSocket>;

impl<X, H: TransportHandler<UdpSocket>> UdpTransport<X, H> {
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

pub struct TcpUdpTransport<X, TH, UH>(TcpUdp<TcpTransport<X, TH>,
                                               UdpTransport<X, UH>>)
           where TH: TransportHandler<TcpStream>,
                 UH: TransportHandler<UdpSocket>;

impl<X, TH, UH> TcpUdpTransport<X, TH, UH>
                where TH: TransportHandler<TcpStream>,
                      UH: TransportHandler<UdpSocket> {
    pub fn new_tcp<S: GenericScope>(sock: TcpStream, seed: TH::Seed,
                                    scope: &mut S) -> Response<Self, Void> {
        TcpTransport::new(sock, seed, scope).map_self(TcpUdpTransport::from)
    }

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

pub struct TcpServer<X, H>(ServerMachine<X, TcpListener, H>)
           where H: AcceptHandler<TcpStream>;

impl<X, H: AcceptHandler<TcpStream>> TcpServer<X, H> {
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


//------------ UdpServer ----------------------------------------------------

pub struct UdpServer<X, H>(TransportMachine<X, UdpSocket, H>)
           where H: TransportHandler<UdpSocket>;

impl<X, H: TransportHandler<UdpSocket>> UdpServer<X, H> {
    pub fn new<S: GenericScope>(sock: UdpSocket, seed: H::Seed, scope: &mut S)
                                -> Response<Self, Void> {
        TransportMachine::new(sock, seed, scope).map_self(UdpServer)
    }
}

impl<X, H: TransportHandler<UdpSocket>> Machine for UdpServer<X, H> {
    type Context = X;
    type Seed = (UdpSocket, H::Seed);

    wrapped_machine!(TransportMachine, UdpServer);
}


//------------ TcpUdpServer -------------------------------------------------

pub struct TcpUdpServer<X, AH, UH>(Compose2<TcpServer<X, AH>,
                                            UdpServer<X, UH>>)
           where AH: AcceptHandler<TcpStream>,
                 UH: TransportHandler<UdpSocket>;

impl<X, AH, UH> TcpUdpServer<X, AH, UH>
                where AH: AcceptHandler<TcpStream>,
                      UH: TransportHandler<UdpSocket> {
    pub fn new_tcp<S>(sock: TcpListener, handler: AH, scope: &mut S)
                      -> (Response<Self, Void>, TriggerSender)
                   where S: GenericScope {
        let (m, t) = TcpServer::new(sock, handler, scope);
        (m.map_self(|m| TcpUdpServer(Compose2::A(m))), t)
    }

    pub fn new_udp<S: GenericScope>(sock: UdpSocket, seed: UH::Seed,
                                    scope: &mut S) -> Response<Self, Void> {
        UdpServer::new(sock, seed, scope)
                  .map_self(|m| TcpUdpServer(Compose2::B(m)))
    }
}
                
impl<X, AH, UH> Machine for TcpUdpServer<X, AH, UH>
                where AH: AcceptHandler<TcpStream>,
                      UH: TransportHandler<UdpSocket> {
    type Context = X;
    type Seed = <Compose2<TcpServer<X, AH>,
                          UdpServer<X, UH>> as Machine>::Seed;

    wrapped_machine!(Compose2, TcpUdpServer);
}


//============ Client Machines ===============================================

//------------ TcpClient ----------------------------------------------------

pub struct TcpClient<X, RH, TH>(RequestMachine<X, TcpTransport<X, TH>, RH,
                                               TcpFactory<TH::Seed>>)
    where RH: RequestHandler<Output=(SocketAddr, TH::Seed)>,
          TH: TransportHandler<TcpStream>;


impl<X, RH, TH> TcpClient<X, RH, TH>
                where RH: RequestHandler<Output=(SocketAddr, TH::Seed)>,
                      TH: TransportHandler<TcpStream> {
    pub fn new<S>(handler: RH, scope: &mut S)
                  -> (Response<Self, Void>, DuctSender<RH::Request>)
               where S: GenericScope {
        let (m, tx) = RequestMachine::new(handler, TcpFactory::new(), scope);
        (m.map_self(TcpClient), tx)
    }
}

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

