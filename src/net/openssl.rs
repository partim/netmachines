//! Encrypted and combined machines using OpenSSL.

use std::marker::PhantomData;
use std::net::SocketAddr;
use openssl::ssl::SslContext;
use rotor::{EventSet, GenericScope, Machine, Response, Scope, Void};
use rotor::mio::tcp::{TcpListener, TcpStream};
use rotor::mio::udp::UdpSocket;
use ::sockets::openssl::{TlsListener, TlsStream, StartTlsListener,
                         StartTlsStream};
use super::machines::{ServerMachine, TransportMachine};
use super::clear::{TcpServer, TcpTransport, UdpTransport};
use ::compose::{Compose2, Compose3};
use ::handlers::{AcceptHandler, RequestHandler, TransportHandler};
use ::request::{RequestMachine, SeedFactory, TranslateError};
use ::utils::ResponseExt;
use ::sync::{DuctSender, TriggerSender};

//============ Transport Machines ============================================

//------------ TlsTransport --------------------------------------------------

pub struct TlsTransport<X, H>(TransportMachine<X, TlsStream, H>)
           where H: TransportHandler<TlsStream>;

impl<X, H: TransportHandler<TlsStream>> TlsTransport<X, H> {
    pub fn new<S: GenericScope>(sock: TlsStream, seed: H::Seed,
                                scope: &mut S) -> Response<Self, Void> {
        TransportMachine::new(sock, seed, scope).map_self(TlsTransport)
    }
}

impl<X, H: TransportHandler<TlsStream>> Machine for TlsTransport<X, H> {
    type Context = X;
    type Seed = (TlsStream, H::Seed);

    wrapped_machine!(TransportMachine, TlsTransport);
}


//------------ StartTlsTransport ---------------------------------------------

pub struct StartTlsTransport<X, H>(TransportMachine<X, StartTlsStream, H>)
           where H: TransportHandler<StartTlsStream>;

impl<X, H: TransportHandler<StartTlsStream>> StartTlsTransport<X, H> {
    pub fn new<S: GenericScope>(sock: StartTlsStream, seed: H::Seed,
                                scope: &mut S) -> Response<Self, Void> {
        TransportMachine::new(sock, seed, scope).map_self(StartTlsTransport)
    }
}

impl<X, H> Machine for StartTlsTransport<X, H>
           where H: TransportHandler<StartTlsStream> {
    type Context = X;
    type Seed = (StartTlsStream, H::Seed);

    wrapped_machine!(TransportMachine, StartTlsTransport);
}


//------------ TlsTcpTransport -----------------------------------------------

pub struct TlsTcpTransport<X, SH, CH>(TlsTcp<TlsTransport<X, SH>,
                                               TcpTransport<X, CH>>)
           where SH: TransportHandler<TlsStream>,
                 CH: TransportHandler<TcpStream>;

impl<X, SH, CH> TlsTcpTransport<X, SH, CH>
                where SH: TransportHandler<TlsStream>,
                      CH: TransportHandler<TcpStream> {
    pub fn new_tls<S: GenericScope>(sock: TlsStream, seed: SH::Seed,
                                    scope: &mut S) -> Response<Self, Void> {
        TlsTransport::new(sock, seed, scope).map_self(TlsTcpTransport::from)
    }

    pub fn new_tcp<S: GenericScope>(sock: TcpStream, seed: CH::Seed,
                                    scope: &mut S) -> Response<Self, Void> {
        TcpTransport::new(sock, seed, scope).map_self(TlsTcpTransport::from)
    }
}


//--- From

impl<X, SH, CH> From<TlsTransport<X, SH>> for TlsTcpTransport<X, SH, CH>
                where SH: TransportHandler<TlsStream>,
                      CH: TransportHandler<TcpStream> {
    fn from(tls: TlsTransport<X, SH>) -> Self {
        TlsTcpTransport(TlsTcp::Tls(tls))
    }
}

impl<X, SH, CH> From<TcpTransport<X, CH>> for TlsTcpTransport<X, SH, CH>
                where SH: TransportHandler<TlsStream>,
                      CH: TransportHandler<TcpStream> {
    fn from(tcp: TcpTransport<X, CH>) -> Self {
        TlsTcpTransport(TlsTcp::Tcp(tcp))
    }
}


//--- Machine

impl<X, SH, CH> Machine for TlsTcpTransport<X, SH, CH>
                where SH: TransportHandler<TlsStream>,
                      CH: TransportHandler<TcpStream> {
    type Context = X;
    type Seed = TlsTcp<<TlsTransport<X, SH> as Machine>::Seed,
                        <TcpTransport<X, CH> as Machine>::Seed>;

    fn create(seed: Self::Seed, scope: &mut Scope<X>)
              -> Response<Self, Void> {
        match seed {
            TlsTcp::Tls(seed) => {
                TlsTransport::create(seed, scope)
                             .map_self(TlsTcpTransport::from)
            }
            TlsTcp::Tcp(seed) => {
                TcpTransport::create(seed, scope)
                             .map_self(TlsTcpTransport::from)
            }
        }
    }

    fn ready(self, events: EventSet, scope: &mut Scope<X>)
             -> Response<Self, Self::Seed> {
        match self.0 {
            TlsTcp::Tls(tls) => {
                tls.ready(events, scope)
                   .map(TlsTcpTransport::from, TlsTcp::Tls)
            }
            TlsTcp::Tcp(tcp) => {
                tcp.ready(events, scope)
                   .map(TlsTcpTransport::from, TlsTcp::Tcp)
            }
        }
    }

    fn spawned(self, scope: &mut Scope<X>) -> Response<Self, Self::Seed> {
        match self.0 {
            TlsTcp::Tls(tls) => {
                tls.spawned(scope).map(TlsTcpTransport::from, TlsTcp::Tls)
            }
            TlsTcp::Tcp(tcp) => {
                tcp.spawned(scope).map(TlsTcpTransport::from, TlsTcp::Tcp)
            }
        }
    }

    fn timeout(self, scope: &mut Scope<X>) -> Response<Self, Self::Seed> {
        match self.0 {
            TlsTcp::Tls(tls) => {
                tls.timeout(scope).map(TlsTcpTransport::from, TlsTcp::Tls)
            }
            TlsTcp::Tcp(tcp) => {
                tcp.timeout(scope).map(TlsTcpTransport::from, TlsTcp::Tcp)
            }
        }
    }
    
    fn wakeup(self, scope: &mut Scope<X>) -> Response<Self, Self::Seed> {
        match self.0 {
            TlsTcp::Tls(tls) => {
                tls.wakeup(scope).map(TlsTcpTransport::from, TlsTcp::Tls)
            }
            TlsTcp::Tcp(tcp) => {
                tcp.wakeup(scope).map(TlsTcpTransport::from, TlsTcp::Tcp)
            }
        }
    }
}


//------------ TlsUdpTransport -----------------------------------------------

pub struct TlsUdpTransport<X, TH, UH>(TlsUdp<TlsTransport<X, TH>,
                                             UdpTransport<X, UH>>)
           where TH: TransportHandler<TlsStream>,
                 UH: TransportHandler<UdpSocket>;

impl<X, TH, UH> TlsUdpTransport<X, TH, UH>
                where TH: TransportHandler<TlsStream>,
                      UH: TransportHandler<UdpSocket> {
    pub fn new_tls<S: GenericScope>(sock: TlsStream, seed: TH::Seed,
                                    scope: &mut S) -> Response<Self, Void> {
        TlsTransport::new(sock, seed, scope).map_self(TlsUdpTransport::from)
    }

    pub fn new_udp<S: GenericScope>(sock: UdpSocket, seed: UH::Seed,
                                    scope: &mut S) -> Response<Self, Void> {
        UdpTransport::new(sock, seed, scope).map_self(TlsUdpTransport::from)
    }
}


//--- From

impl<X, TH, UH> From<TlsTransport<X, TH>> for TlsUdpTransport<X, TH, UH>
                where TH: TransportHandler<TlsStream>,
                      UH: TransportHandler<UdpSocket> {
    fn from(tls: TlsTransport<X, TH>) -> Self {
        TlsUdpTransport(TlsUdp::Tls(tls))
    }
}
                
impl<X, TH, UH> From<UdpTransport<X, UH>> for TlsUdpTransport<X, TH, UH>
                where TH: TransportHandler<TlsStream>,
                      UH: TransportHandler<UdpSocket> {
    fn from(udp: UdpTransport<X, UH>) -> Self {
        TlsUdpTransport(TlsUdp::Udp(udp))
    }
}


//--- Machine

impl<X, TH, UH> Machine for TlsUdpTransport<X, TH, UH>
                where TH: TransportHandler<TlsStream>,
                      UH: TransportHandler<UdpSocket> {
    type Context = X;
    type Seed = TlsUdp<(TlsStream, TH::Seed), (UdpSocket, UH::Seed)>;

    fn create(seed: Self::Seed, scope: &mut Scope<X>)
              -> Response<Self, Void> {
        match seed {
            TlsUdp::Tls(seed) => {
                TlsTransport::create(seed, scope)
                             .map_self(TlsUdpTransport::from)
            }
            TlsUdp::Udp(seed) => {
                UdpTransport::create(seed, scope)
                             .map_self(TlsUdpTransport::from)
            }
        }
    }

    fn ready(self, events: EventSet, scope: &mut Scope<X>)
             -> Response<Self, Self::Seed> {
        match self.0 {
            TlsUdp::Tls(tls) => {
                tls.ready(events, scope)
                    .map(TlsUdpTransport::from, TlsUdp::Tls)
            }
            TlsUdp::Udp(udp) => {
                udp.ready(events, scope)
                    .map(TlsUdpTransport::from, TlsUdp::Udp)
            }
        }
    }

    fn spawned(self, scope: &mut Scope<X>) -> Response<Self, Self::Seed> {
        match self.0 {
            TlsUdp::Tls(tls) => {
                tls.spawned(scope).map(TlsUdpTransport::from, TlsUdp::Tls)
            }
            TlsUdp::Udp(udp) => {
                udp.spawned(scope).map(TlsUdpTransport::from, TlsUdp::Udp)
            }
        }
    }

    fn timeout(self, scope: &mut Scope<Self::Context>)
               -> Response<Self, Self::Seed> {
        match self.0 {
            TlsUdp::Tls(tls) => {
                tls.timeout(scope).map(TlsUdpTransport::from, TlsUdp::Tls)
            }
            TlsUdp::Udp(udp) => {
                udp.timeout(scope).map(TlsUdpTransport::from, TlsUdp::Udp)
            }
        }
    }

    fn wakeup(self, scope: &mut Scope<Self::Context>)
              -> Response<Self, Self::Seed> {
        match self.0 {
            TlsUdp::Tls(tls) => {
                tls.wakeup(scope).map(TlsUdpTransport::from, TlsUdp::Tls)
            }
            TlsUdp::Udp(udp) => {
                udp.wakeup(scope).map(TlsUdpTransport::from, TlsUdp::Udp)
            }
        }
    }
}


//============ Server Machines ===============================================

//------------ TlsServer -----------------------------------------------------

pub struct TlsServer<X, H>(ServerMachine<X, TlsListener, H>)
           where H: AcceptHandler<TlsStream>;

impl<X, H: AcceptHandler<TlsStream>> TlsServer<X, H> {
    pub fn new<S: GenericScope>(sock: TlsListener, handler: H, scope: &mut S)
                                -> (Response<Self, Void>, TriggerSender) {
        let (m, t) = ServerMachine::new(sock, handler, scope);
        (m.map_self(TlsServer), t)
    }
}

impl<X, H: AcceptHandler<TlsStream>> Machine for TlsServer<X, H> {
    type Context = X;
    type Seed = <ServerMachine<X, TlsListener, H> as Machine>::Seed;

    wrapped_machine!(ServerMachine, TlsServer);
}


//------------ StartTlsServer -----------------------------------------------

pub struct StartTlsServer<X, H>(ServerMachine<X, StartTlsListener, H>)
           where H: AcceptHandler<StartTlsStream>;

impl<X, H: AcceptHandler<StartTlsStream>> StartTlsServer<X, H> {
    pub fn new<S>(sock: StartTlsListener, handler: H, scope: &mut S)
                  -> (Response<Self, Void>, TriggerSender)
               where S: GenericScope {
        let (m, t) = ServerMachine::new(sock, handler, scope);
        (m.map_self(StartTlsServer), t)
    }
}

impl<X, H: AcceptHandler<StartTlsStream>> Machine for StartTlsServer<X, H> {
    type Context = X;
    type Seed = <ServerMachine<X, StartTlsListener, H> as Machine>::Seed;

    wrapped_machine!(ServerMachine, StartTlsServer);
}


//------------ TlsTcpServer -------------------------------------------------

pub struct TlsTcpServer<X, SH, CH>(Compose2<TlsServer<X, SH>,
                                            TcpServer<X, CH>>)
    where SH: AcceptHandler<TlsStream>,
          CH: AcceptHandler<TcpStream>;

impl<X, SH, CH> TlsTcpServer<X, SH, CH>
                where SH: AcceptHandler<TlsStream>,
                      CH: AcceptHandler<TcpStream> {
    pub fn new_tls<S>(sock: TlsListener, handler: SH, scope: &mut S)
                      -> (Response<Self, Void>, TriggerSender)
                   where S: GenericScope {
        let (m, t) = TlsServer::new(sock, handler, scope);
        (m.map_self(|m| TlsTcpServer((Compose2::A(m)))), t)
    }

    pub fn new_tcp<S>(sock: TcpListener, handler: CH, scope: &mut S)
                      -> (Response<Self, Void>, TriggerSender)
                   where S: GenericScope {
        let (m, t) = TcpServer::new(sock, handler, scope);
        (m.map_self(|m| TlsTcpServer(Compose2::B(m))), t)
    }
}

impl<X, SH, CH> Machine for TlsTcpServer<X, SH, CH>
                where SH: AcceptHandler<TlsStream>,
                      CH: AcceptHandler<TcpStream> {
    type Context = X;
    type Seed = <Compose2<TlsServer<X, SH>,
                          TcpServer<X, CH>> as Machine>::Seed;

    wrapped_machine!(Compose2, TlsTcpServer);
}


//------------ TlsUdpServer -------------------------------------------------

pub struct TlsUdpServer<X, AH, UH>(Compose2<TlsServer<X, AH>,
                                            UdpTransport<X, UH>>)
           where AH: AcceptHandler<TlsStream>,
                 UH: TransportHandler<UdpSocket>;

impl<X, AH, UH> TlsUdpServer<X, AH, UH>
                where AH: AcceptHandler<TlsStream>,
                      UH: TransportHandler<UdpSocket> {
    pub fn new_tls<S>(sock: TlsListener, handler: AH, scope: &mut S)
                      -> (Response<Self, Void>, TriggerSender)
                   where S: GenericScope {
        let (m, t) = TlsServer::new(sock, handler, scope);
        (m.map_self(|m| TlsUdpServer((Compose2::A(m)))), t)
    }

    pub fn new_udp<S: GenericScope>(sock: UdpSocket, seed: UH::Seed,
                                    scope: &mut S) -> Response<Self, Void> {
        UdpTransport::new(sock, seed, scope)
                  .map_self(|m| TlsUdpServer(Compose2::B(m)))
    }
}
                
impl<X, AH, UH> Machine for TlsUdpServer<X, AH, UH>
                where AH: AcceptHandler<TlsStream>,
                      UH: TransportHandler<UdpSocket> {
    type Context = X;
    type Seed = <Compose2<TlsServer<X, AH>,
                          UdpTransport<X, UH>> as Machine>::Seed;

    wrapped_machine!(Compose2, TlsUdpServer);
}


//------------ StartTlsUdpServer --------------------------------------------

pub struct StartTlsUdpServer<X, AH, UH>(Compose2<StartTlsServer<X, AH>,
                                            UdpTransport<X, UH>>)
           where AH: AcceptHandler<StartTlsStream>,
                 UH: TransportHandler<UdpSocket>;

impl<X, AH, UH> StartTlsUdpServer<X, AH, UH>
                where AH: AcceptHandler<StartTlsStream>,
                      UH: TransportHandler<UdpSocket> {
    pub fn new_tls<S>(sock: StartTlsListener, handler: AH, scope: &mut S)
                      -> (Response<Self, Void>, TriggerSender)
                   where S: GenericScope {
        let (m, t) = StartTlsServer::new(sock, handler, scope);
        (m.map_self(|m| StartTlsUdpServer((Compose2::A(m)))), t)
    }

    pub fn new_udp<S: GenericScope>(sock: UdpSocket, seed: UH::Seed,
                                    scope: &mut S) -> Response<Self, Void> {
        UdpTransport::new(sock, seed, scope)
                  .map_self(|m| StartTlsUdpServer(Compose2::B(m)))
    }
}
                
impl<X, AH, UH> Machine for StartTlsUdpServer<X, AH, UH>
                where AH: AcceptHandler<StartTlsStream>,
                      UH: TransportHandler<UdpSocket> {
    type Context = X;
    type Seed = <Compose2<StartTlsServer<X, AH>,
                          UdpTransport<X, UH>> as Machine>::Seed;

    wrapped_machine!(Compose2, StartTlsUdpServer);
}


//------------ TlsTcpUdpServer -----------------------------------------------

pub struct TlsTcpUdpServer<X, SH, CH, UH>(Compose3<TlsServer<X, SH>,
                                                   TcpServer<X, CH>,
                                                   UdpTransport<X, UH>>)
    where SH: AcceptHandler<TlsStream>,
          CH: AcceptHandler<TcpStream>,
          UH: TransportHandler<UdpSocket>;

impl<X, SH, CH, UH> TlsTcpUdpServer<X, SH, CH, UH>
                    where SH: AcceptHandler<TlsStream>,
                          CH: AcceptHandler<TcpStream>,
                          UH: TransportHandler<UdpSocket> {
    pub fn new_tls<S>(sock: TlsListener, handler: SH, scope: &mut S)
                      -> (Response<Self, Void>, TriggerSender)
                   where S: GenericScope {
        let (m, t) = TlsServer::new(sock, handler, scope);
        (m.map_self(|m| TlsTcpUdpServer((Compose3::A(m)))), t)
    }

    pub fn new_tcp<S>(sock: TcpListener, handler: CH, scope: &mut S)
                      -> (Response<Self, Void>, TriggerSender)
                   where S: GenericScope {
        let (m, t) = TcpServer::new(sock, handler, scope);
        (m.map_self(|m| TlsTcpUdpServer(Compose3::B(m))), t)
    }

    pub fn new_udp<S: GenericScope>(sock: UdpSocket, seed: UH::Seed,
                                    scope: &mut S) -> Response<Self, Void> {
        UdpTransport::new(sock, seed, scope)
                  .map_self(|m| TlsTcpUdpServer(Compose3::C(m)))
    }
}

impl<X, SH, CH, UH> Machine for TlsTcpUdpServer<X, SH, CH, UH>
                    where SH: AcceptHandler<TlsStream>,
                          CH: AcceptHandler<TcpStream>,
                          UH: TransportHandler<UdpSocket> {
    type Context = X;
    type Seed = <Compose3<TlsServer<X, SH>, TcpServer<X, CH>,
                          UdpTransport<X, UH>> as Machine>::Seed;

    wrapped_machine!(Compose3, TlsTcpUdpServer);
}


//============ Client Machines ===============================================

//------------ TlsClient -----------------------------------------------------

pub struct TlsClient<X, RH, TH>(RequestMachine<X, TlsTransport<X, TH>, RH,
                                               TlsFactory<TH::Seed>>)
    where RH: RequestHandler<Output=(SocketAddr, TH::Seed)>,
          TH: TransportHandler<TlsStream>;

impl<X, RH, TH> TlsClient<X, RH, TH>
                where RH: RequestHandler<Output=(SocketAddr, TH::Seed)>,
                      TH: TransportHandler<TlsStream> {
    pub fn new<S>(handler: RH, ctx: SslContext, scope: &mut S)
                  -> (Response<Self, Void>, DuctSender<RH::Request>)
               where S: GenericScope {
        let (m, tx) = RequestMachine::new(handler, TlsFactory::new(ctx),
                                          scope);
        (m.map_self(TlsClient), tx)
    }
}

impl<X, RH, TH> Machine for TlsClient<X, RH, TH>
                where RH: RequestHandler<Output=(SocketAddr, TH::Seed)>,
                      TH: TransportHandler<TlsStream> {
    type Context = X;
    type Seed = (TlsStream, TH::Seed);

    wrapped_machine!(RequestMachine, TlsClient);
}


//------------ StartTlsClient -----------------------------------------------

pub struct StartTlsClient<X, RH, TH>(RequestMachine<X,
                                                    StartTlsTransport<X, TH>,
                                                    RH,
                                                    StartTlsFactory<TH::Seed>>)
    where RH: RequestHandler<Output=(SocketAddr, TH::Seed)>,
          TH: TransportHandler<StartTlsStream>;

impl<X, RH, TH> StartTlsClient<X, RH, TH>
                where RH: RequestHandler<Output=(SocketAddr, TH::Seed)>,
                      TH: TransportHandler<StartTlsStream> {
    pub fn new<S>(handler: RH, ctx: SslContext, scope: &mut S)
                  -> (Response<Self, Void>, DuctSender<RH::Request>)
               where S: GenericScope {
        let (m, tx) = RequestMachine::new(handler, StartTlsFactory::new(ctx),
                                          scope);
        (m.map_self(StartTlsClient), tx)
    }
}

impl<X, RH, TH> Machine for StartTlsClient<X, RH, TH>
                where RH: RequestHandler<Output=(SocketAddr, TH::Seed)>,
                      TH: TransportHandler<StartTlsStream> {
    type Context = X;
    type Seed = (StartTlsStream, TH::Seed);
    wrapped_machine!(RequestMachine, StartTlsClient);
}


//------------ TlsTcpClient -------------------------------------------------

pub struct TlsTcpClient<X, RH, SH, CH>(
    RequestMachine<X, TlsTcpTransport<X, SH, CH>, RH,
                   TlsTcpFactory<SH::Seed, CH::Seed>>
) where RH: RequestHandler<Output=TlsTcp<(SocketAddr, SH::Seed),
                                         (SocketAddr, CH::Seed)>>,
        SH: TransportHandler<TlsStream>,
        CH: TransportHandler<TcpStream>;

impl<X, RH, SH, CH> TlsTcpClient<X, RH, SH, CH>
            where RH: RequestHandler<Output=TlsTcp<(SocketAddr, SH::Seed),
                                                   (SocketAddr, CH::Seed)>>,
                  SH: TransportHandler<TlsStream>,
                  CH: TransportHandler<TcpStream> {
    pub fn new<S>(handler: RH, ctx: SslContext, scope: &mut S)
                  -> (Response<Self, Void>, DuctSender<RH::Request>)
               where S: GenericScope {
        let (m, tx) = RequestMachine::new(handler, TlsTcpFactory::new(ctx),
                                          scope);
        (m.map_self(TlsTcpClient), tx)
    }
}

impl<X, RH, SH, CH> Machine for TlsTcpClient<X, RH, SH, CH>
            where RH: RequestHandler<Output=TlsTcp<(SocketAddr, SH::Seed),
                                                   (SocketAddr, CH::Seed)>>,
                  SH: TransportHandler<TlsStream>,
                  CH: TransportHandler<TcpStream> {
    type Context = X;
    type Seed = TlsTcp<(TlsStream, SH::Seed), (TcpStream, CH::Seed)>;

    wrapped_machine!(RequestMachine, TlsTcpClient);
}


//------------ TlsUdpClient -------------------------------------------------

pub struct TlsUdpClient<X, RH, TH, UH>(
    RequestMachine<X, TlsUdpTransport<X, TH, UH>, RH,
                   TlsUdpFactory<TH::Seed, UH::Seed>>
) where RH: RequestHandler<Output=TlsUdp<(SocketAddr, TH::Seed),
                                         (SocketAddr, UH::Seed)>>,
        TH: TransportHandler<TlsStream>,
        UH: TransportHandler<UdpSocket>;

impl<X, RH, TH, UH> TlsUdpClient<X, RH, TH, UH>
          where RH: RequestHandler<Output=TlsUdp<(SocketAddr, TH::Seed),
                                                 (SocketAddr, UH::Seed)>>,
                TH: TransportHandler<TlsStream>,
                UH: TransportHandler<UdpSocket> {
    pub fn new<S>(handler: RH, ctx: SslContext, scope: &mut S)
                  -> (Response<Self, Void>, DuctSender<RH::Request>)
               where S: GenericScope {
        let (m, tx) = RequestMachine::new(handler, TlsUdpFactory::new(ctx),
                                          scope);
        (m.map_self(TlsUdpClient), tx)
    }
}

impl<X, RH, TH, UH> Machine for TlsUdpClient<X, RH, TH, UH>
          where RH: RequestHandler<Output=TlsUdp<(SocketAddr, TH::Seed),
                                                 (SocketAddr, UH::Seed)>>,
                TH: TransportHandler<TlsStream>,
                UH: TransportHandler<UdpSocket> {
    type Context = X;
    type Seed = TlsUdp<(TlsStream, TH::Seed), (UdpSocket, UH::Seed)>;

    wrapped_machine!(RequestMachine, TlsUdpClient);
}


//============ Socket Factories ==============================================

//------------ TlsFactory ----------------------------------------------------

struct TlsFactory<S> {
    ctx: SslContext,
    marker: PhantomData<S>
}

impl<S> TlsFactory<S> {
    fn new(ctx: SslContext) -> Self {
        TlsFactory { ctx: ctx, marker: PhantomData }
    }
}

impl<S> SeedFactory<(SocketAddr, S), (TlsStream, S)> for TlsFactory<S> {
    fn translate(&self, output: (SocketAddr, S))
                 -> Result<(TlsStream, S), TranslateError<(SocketAddr, S)>> {
        let (addr, seed) = output;
        match TlsStream::connect(&addr, &self.ctx) {
            Ok(sock) => Ok((sock, seed)),
            Err(err) => Err(TranslateError((addr, seed), err.into()))
        }
    }
}


//------------ StartTlsFactory -----------------------------------------------

struct StartTlsFactory<S> {
    ctx: SslContext,
    marker: PhantomData<S>
}

impl<S> StartTlsFactory<S> {
    fn new(ctx: SslContext) -> Self {
        StartTlsFactory { ctx: ctx, marker: PhantomData }
    }
}

impl<S> SeedFactory<(SocketAddr, S), (StartTlsStream, S)>
        for StartTlsFactory<S> {
    fn translate(&self, output: (SocketAddr, S))
                 -> Result<(StartTlsStream, S),
                           TranslateError<(SocketAddr, S)>> {
        let (addr, seed) = output;
        match StartTlsStream::connect(&addr, self.ctx.clone()) {
            Ok(sock) => Ok((sock, seed)),
            Err(err) => Err(TranslateError((addr, seed), err.into()))
        }
    }
}


//------------ TlsTcpFactory -------------------------------------------------

struct TlsTcpFactory<S, C> {
    ctx: SslContext,
    marker: PhantomData<(S, C)>
}

impl<S, C> TlsTcpFactory<S, C> {
    fn new(ctx: SslContext) -> Self {
        TlsTcpFactory { ctx: ctx, marker: PhantomData }
    }
}

impl<S, C> SeedFactory<TlsTcp<(SocketAddr, S), (SocketAddr, C)>,
                       TlsTcp<(TlsStream, S), (TcpStream, C)>>
           for TlsTcpFactory<S, C> {
    fn translate(&self, output: TlsTcp<(SocketAddr, S), (SocketAddr, C)>)
                 -> Result<TlsTcp<(TlsStream, S), (TcpStream, C)>,
                           TranslateError<TlsTcp<(SocketAddr, S),
                                                 (SocketAddr, C)>>> {
        use self::TlsTcp::*;

        match output {
            Tls((addr, seed)) => {
                match TlsStream::connect(&addr, &self.ctx) {
                    Ok(sock) => Ok(Tls((sock, seed))),
                    Err(err) => Err(TranslateError(Tls((addr, seed)),
                                                   err.into()))
                }
            }
            Tcp((addr, seed)) => {
                match TcpStream::connect(&addr) {
                    Ok(sock) => Ok(Tcp((sock, seed))),
                    Err(err) => Err(TranslateError(Tcp((addr, seed)),
                                                   err.into()))
                }
            }
        }
    }
}


//------------ TlsUdpFactory -------------------------------------------------

struct TlsUdpFactory<T, U> {
    ctx: SslContext,
    marker: PhantomData<(T, U)>
}

impl<T, U> TlsUdpFactory<T, U> {
    fn new(ctx: SslContext) -> Self {
        TlsUdpFactory { ctx: ctx, marker: PhantomData }
    }
}

impl<T, U> SeedFactory<TlsUdp<(SocketAddr, T), (SocketAddr, U)>,
                       TlsUdp<(TlsStream, T), (UdpSocket, U)>>
           for TlsUdpFactory<T, U> {
    fn translate(&self, output: TlsUdp<(SocketAddr, T), (SocketAddr, U)>)
                 -> Result<TlsUdp<(TlsStream, T), (UdpSocket, U)>,
                           TranslateError<TlsUdp<(SocketAddr, T),
                                                 (SocketAddr, U)>>> {
        use self::TlsUdp::*;

        match output {
            Tls((addr, seed)) => {
                match TlsStream::connect(&addr, &self.ctx) {
                    Ok(sock) => Ok(Tls((sock, seed))),
                    Err(err) => Err(TranslateError(Tls((addr, seed)),
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

pub enum TlsTcp<S, C> {
    Tls(S),
    Tcp(C)
}

pub enum TlsUdp<T, U> {
    Tls(T),
    Udp(U)
}

pub enum TlsTcpOrUdp<S, C, U> {
    Tls(S),
    Tcp(C),
    Udp(U)
}
