//! Encrypted and combined machines using OpenSSL.

use rotor::{EventSet, GenericScope, Machine, Response, Scope, Void};
use rotor::mio::tcp::TcpStream;
use rotor::mio::udp::UdpSocket;
use ::sockets::openssl::{TlsListener, TlsStream, StartTlsListener,
                         StartTlsStream};
use super::machines::{ClientMachine, ServerMachine, TransportMachine};
use super::clear::{UdpServer, TcpServer};
use ::compose::{Compose2, Compose3};
use ::handlers::{AcceptHandler, RequestHandler, TransportHandler};
use ::machines::RequestMachine;
use ::utils::ResponseExt;
use ::sync::Sender;


//------------ TlsServer ----------------------------------------------------

pub struct TlsServer<X, H>(ServerMachine<X, TlsListener, H>)
           where H: AcceptHandler<TlsStream>;

impl<X, H: AcceptHandler<TlsStream>> TlsServer<X, H> {
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
}

impl<X, H: AcceptHandler<StartTlsStream>> Machine for StartTlsServer<X, H> {
    type Context = X;
    type Seed = <ServerMachine<X, StartTlsListener, H> as Machine>::Seed;

    wrapped_machine!(ServerMachine, StartTlsServer);
}


//------------ TlsClient ----------------------------------------------------

pub struct TlsClient<X, RH, TH>(ClientMachine<X, TlsStream, RH, TH>)
                     where RH: RequestHandler<TlsStream>,
                           TH: TransportHandler<TlsStream, Seed=RH::Seed>;

impl<X, RH, TH> TlsClient<X, RH, TH>
                where RH: RequestHandler<TlsStream>,
                      TH: TransportHandler<TlsStream, Seed=RH::Seed> {
    pub fn new<S: GenericScope>(handler: RH, scope: &mut S)
               -> (Self, Sender<RH::Request>) {
        let (m, f) = ClientMachine::new(handler, scope);
        (TlsClient(m), f)
    }
}

impl<X, RH, TH> Machine for TlsClient<X, RH, TH>
                where RH: RequestHandler<TlsStream>,
                      TH: TransportHandler<TlsStream, Seed=RH::Seed> {
    type Context = X;
    type Seed = <ClientMachine<X, TlsStream, RH, TH> as Machine>::Seed;

    wrapped_machine!(ClientMachine, TlsClient);
}


//------------ StartTlsClient -----------------------------------------------

pub struct StartTlsClient<X, RH, TH>(ClientMachine<X, StartTlsStream, RH, TH>)
                     where RH: RequestHandler<StartTlsStream>,
                           TH: TransportHandler<StartTlsStream, Seed=RH::Seed>;

impl<X, RH, TH> StartTlsClient<X, RH, TH>
                where RH: RequestHandler<StartTlsStream>,
                      TH: TransportHandler<StartTlsStream, Seed=RH::Seed> {
    pub fn new<S: GenericScope>(handler: RH, scope: &mut S)
               -> (Self, Sender<RH::Request>) {
        let (m, f) = ClientMachine::new(handler, scope);
        (StartTlsClient(m), f)
    }
}

impl<X, RH, TH> Machine for StartTlsClient<X, RH, TH>
                where RH: RequestHandler<StartTlsStream>,
                      TH: TransportHandler<StartTlsStream, Seed=RH::Seed> {
    type Context = X;
    type Seed = <ClientMachine<X, StartTlsStream, RH, TH> as Machine>::Seed;

    wrapped_machine!(ClientMachine, StartTlsClient);
}


//------------ TlsTcpServer -------------------------------------------------

pub struct TlsTcpServer<X, SH, CH>(Compose2<TlsServer<X, SH>,
                                            TcpServer<X, CH>>)
    where SH: AcceptHandler<TlsStream>,
          CH: AcceptHandler<TcpStream>;

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
                                            UdpServer<X, UH>>)
           where AH: AcceptHandler<TlsStream>,
                 UH: TransportHandler<UdpSocket>;

impl<X, AH, UH> TlsUdpServer<X, AH, UH>
                where AH: AcceptHandler<TlsStream>,
                      UH: TransportHandler<UdpSocket> {
}
                
impl<X, AH, UH> Machine for TlsUdpServer<X, AH, UH>
                where AH: AcceptHandler<TlsStream>,
                      UH: TransportHandler<UdpSocket> {
    type Context = X;
    type Seed = <Compose2<TlsServer<X, AH>,
                          UdpServer<X, UH>> as Machine>::Seed;

    wrapped_machine!(Compose2, TlsUdpServer);
}


//------------ StartTlsUdpServer --------------------------------------------

pub struct StartTlsUdpServer<X, AH, UH>(Compose2<StartTlsServer<X, AH>,
                                            UdpServer<X, UH>>)
           where AH: AcceptHandler<StartTlsStream>,
                 UH: TransportHandler<UdpSocket>;

impl<X, AH, UH> StartTlsUdpServer<X, AH, UH>
                where AH: AcceptHandler<StartTlsStream>,
                      UH: TransportHandler<UdpSocket> {
}
                
impl<X, AH, UH> Machine for StartTlsUdpServer<X, AH, UH>
                where AH: AcceptHandler<StartTlsStream>,
                      UH: TransportHandler<UdpSocket> {
    type Context = X;
    type Seed = <Compose2<StartTlsServer<X, AH>,
                          UdpServer<X, UH>> as Machine>::Seed;

    wrapped_machine!(Compose2, StartTlsUdpServer);
}


//------------ TlsTcpUdpServer -----------------------------------------------

pub struct TlsTcpUdpServer<X, SH, CH, UH>(Compose3<TlsServer<X, SH>,
                                                   TcpServer<X, CH>,
                                                   UdpServer<X, UH>>)
    where SH: AcceptHandler<TlsStream>,
          CH: AcceptHandler<TcpStream>,
          UH: TransportHandler<UdpSocket>;

impl<X, SH, CH, UH> Machine for TlsTcpUdpServer<X, SH, CH, UH>
                    where SH: AcceptHandler<TlsStream>,
                          CH: AcceptHandler<TcpStream>,
                          UH: TransportHandler<UdpSocket> {
    type Context = X;
    type Seed = <Compose3<TlsServer<X, SH>, TcpServer<X, CH>,
                          UdpServer<X, UH>> as Machine>::Seed;

    wrapped_machine!(Compose3, TlsTcpUdpServer);
}


//------------ TlsUdpClient -------------------------------------------------

pub struct TlsUdpClient<X, RH, TH, UH>(
    RTU<RequestMachine<X, TlsOrUdp, RH>,
        TransportMachine<X, TlsStream, TH>,
        TransportMachine<X, UdpSocket, UH>>)
    where RH: RequestHandler<TlsOrUdp>,
          TH: TransportHandler<TlsStream, Seed=RH::Seed>,
          UH: TransportHandler<UdpSocket, Seed=RH::Seed>;

enum RTU<R, T, U> {
    Req(R),
    Tls(T),
    Udp(U)
}

impl<X, RH, TH, UH> TlsUdpClient<X, RH, TH, UH>
                    where RH: RequestHandler<TlsOrUdp>,
                          TH: TransportHandler<TlsStream, Seed=RH::Seed>,
                          UH: TransportHandler<UdpSocket, Seed=RH::Seed> {
    fn req(req: RequestMachine<X, TlsOrUdp, RH>) -> Self {
        TlsUdpClient(RTU::Req(req))
    }

    fn tcp(tcp: TransportMachine<X, TlsStream, TH>) -> Self {
        TlsUdpClient(RTU::Tls(tcp))
    }

    fn udp(udp: TransportMachine<X, UdpSocket, UH>) -> Self {
        TlsUdpClient(RTU::Udp(udp))
    }
}

impl<X, RH, TH, UH> Machine for TlsUdpClient<X, RH, TH, UH>
                    where RH: RequestHandler<TlsOrUdp>,
                          TH: TransportHandler<TlsStream, Seed=RH::Seed>,
                          UH: TransportHandler<UdpSocket, Seed=RH::Seed> {
    type Context = X;
    type Seed = (TlsOrUdp, RH::Seed);

    fn create(seed: Self::Seed, scope: &mut Scope<X>)
              -> Response<Self, Void> {
        match seed.0 {
            TlsOrUdp::Tls(tcp) => {
                TransportMachine::create((tcp, seed.1), scope)
                                 .map_self(TlsUdpClient::tcp)
            }
            TlsOrUdp::Udp(udp) => {
                TransportMachine::create((udp, seed.1), scope)
                                 .map_self(TlsUdpClient::udp)
            }
        }
    }            

    fn ready(self, events: EventSet, scope: &mut Scope<X>)
             -> Response<Self, Self::Seed> {
        match self.0 {
            RTU::Req(req) => {
                req.ready(events, scope).map_self(TlsUdpClient::req)
            }
            RTU::Tls(tcp) => {
                tcp.ready(events, scope).map(TlsUdpClient::tcp,
                                             TlsOrUdp::tcp)
            }
            RTU::Udp(udp) => {
                udp.ready(events, scope).map(TlsUdpClient::udp, TlsOrUdp::udp)
            }
        }
    }

    fn spawned(self, scope: &mut Scope<X>) -> Response<Self, Self::Seed> {
        match self.0 {
            RTU::Req(req) => {
                req.spawned(scope).map_self(TlsUdpClient::req)
            }
            RTU::Tls(tcp) => {
                tcp.spawned(scope).map(TlsUdpClient::tcp, TlsOrUdp::tcp)
            }
            RTU::Udp(udp) => {
                udp.spawned(scope).map(TlsUdpClient::udp, TlsOrUdp::udp)
            }
        }
    }

    fn timeout(self, scope: &mut Scope<X>) -> Response<Self, Self::Seed> {
        match self.0 {
            RTU::Req(req) => {
                req.timeout(scope).map_self(TlsUdpClient::req)
            }
            RTU::Tls(tcp) => {
                tcp.timeout(scope).map(TlsUdpClient::tcp, TlsOrUdp::tcp)
            }
            RTU::Udp(udp) => {
                udp.timeout(scope).map(TlsUdpClient::udp, TlsOrUdp::udp)
            }
        }
    }

    fn wakeup(self, scope: &mut Scope<X>) -> Response<Self, Self::Seed> {
        match self.0 {
            RTU::Req(req) => {
                req.wakeup(scope).map_self(TlsUdpClient::req)
            }
            RTU::Tls(tcp) => {
                tcp.wakeup(scope).map(TlsUdpClient::tcp, TlsOrUdp::tcp)
            }
            RTU::Udp(udp) => {
                udp.wakeup(scope).map(TlsUdpClient::udp, TlsOrUdp::udp)
            }
        }
    }
}


//------------ TlsOrUdp ------------------------------------------------------

pub enum TlsOrUdp {
    Tls(TlsStream),
    Udp(UdpSocket)
}


impl TlsOrUdp {
    fn tcp<T>(seed: (TlsStream, T)) -> (Self, T) {
        (TlsOrUdp::Tls(seed.0), seed.1)
    }

    fn udp<T>(seed: (UdpSocket, T)) -> (Self, T) {
        (TlsOrUdp::Udp(seed.0), seed.1)
    }
}


//------------ StartTlsUdpClient --------------------------------------------

pub struct StartTlsUdpClient<X, RH, TH, UH>(
    RSU<RequestMachine<X, StartTlsOrUdp, RH>,
        TransportMachine<X, StartTlsStream, TH>,
        TransportMachine<X, UdpSocket, UH>>)
    where RH: RequestHandler<StartTlsOrUdp>,
          TH: TransportHandler<StartTlsStream, Seed=RH::Seed>,
          UH: TransportHandler<UdpSocket, Seed=RH::Seed>;

enum RSU<R, T, U> {
    Req(R),
    StartTls(T),
    Udp(U)
}

impl<X, RH, TH, UH> StartTlsUdpClient<X, RH, TH, UH>
                    where RH: RequestHandler<StartTlsOrUdp>,
                          TH: TransportHandler<StartTlsStream, Seed=RH::Seed>,
                          UH: TransportHandler<UdpSocket, Seed=RH::Seed> {
    fn req(req: RequestMachine<X, StartTlsOrUdp, RH>) -> Self {
        StartTlsUdpClient(RSU::Req(req))
    }

    fn tcp(tcp: TransportMachine<X, StartTlsStream, TH>) -> Self {
        StartTlsUdpClient(RSU::StartTls(tcp))
    }

    fn udp(udp: TransportMachine<X, UdpSocket, UH>) -> Self {
        StartTlsUdpClient(RSU::Udp(udp))
    }
}

impl<X, RH, TH, UH> Machine for StartTlsUdpClient<X, RH, TH, UH>
                    where RH: RequestHandler<StartTlsOrUdp>,
                          TH: TransportHandler<StartTlsStream, Seed=RH::Seed>,
                          UH: TransportHandler<UdpSocket, Seed=RH::Seed> {
    type Context = X;
    type Seed = (StartTlsOrUdp, RH::Seed);

    fn create(seed: Self::Seed, scope: &mut Scope<X>)
              -> Response<Self, Void> {
        match seed.0 {
            StartTlsOrUdp::StartTls(tcp) => {
                TransportMachine::create((tcp, seed.1), scope)
                                 .map_self(StartTlsUdpClient::tcp)
            }
            StartTlsOrUdp::Udp(udp) => {
                TransportMachine::create((udp, seed.1), scope)
                                 .map_self(StartTlsUdpClient::udp)
            }
        }
    }            

    fn ready(self, events: EventSet, scope: &mut Scope<X>)
             -> Response<Self, Self::Seed> {
        match self.0 {
            RSU::Req(req) => {
                req.ready(events, scope).map_self(StartTlsUdpClient::req)
            }
            RSU::StartTls(tcp) => {
                tcp.ready(events, scope).map(StartTlsUdpClient::tcp,
                                             StartTlsOrUdp::tcp)
            }
            RSU::Udp(udp) => {
                udp.ready(events, scope).map(StartTlsUdpClient::udp,
                                             StartTlsOrUdp::udp)
            }
        }
    }

    fn spawned(self, scope: &mut Scope<X>) -> Response<Self, Self::Seed> {
        match self.0 {
            RSU::Req(req) => {
                req.spawned(scope).map_self(StartTlsUdpClient::req)
            }
            RSU::StartTls(tcp) => {
                tcp.spawned(scope).map(StartTlsUdpClient::tcp,
                                       StartTlsOrUdp::tcp)
            }
            RSU::Udp(udp) => {
                udp.spawned(scope).map(StartTlsUdpClient::udp,
                                       StartTlsOrUdp::udp)
            }
        }
    }

    fn timeout(self, scope: &mut Scope<X>) -> Response<Self, Self::Seed> {
        match self.0 {
            RSU::Req(req) => {
                req.timeout(scope).map_self(StartTlsUdpClient::req)
            }
            RSU::StartTls(tcp) => {
                tcp.timeout(scope).map(StartTlsUdpClient::tcp,
                                       StartTlsOrUdp::tcp)
            }
            RSU::Udp(udp) => {
                udp.timeout(scope).map(StartTlsUdpClient::udp,
                                       StartTlsOrUdp::udp)
            }
        }
    }

    fn wakeup(self, scope: &mut Scope<X>) -> Response<Self, Self::Seed> {
        match self.0 {
            RSU::Req(req) => {
                req.wakeup(scope).map_self(StartTlsUdpClient::req)
            }
            RSU::StartTls(tcp) => {
                tcp.wakeup(scope).map(StartTlsUdpClient::tcp,
                                      StartTlsOrUdp::tcp)
            }
            RSU::Udp(udp) => {
                udp.wakeup(scope).map(StartTlsUdpClient::udp,
                                      StartTlsOrUdp::udp)
            }
        }
    }
}


//------------ StartTlsOrUdp -------------------------------------------------

pub enum StartTlsOrUdp {
    StartTls(StartTlsStream),
    Udp(UdpSocket)
}


impl StartTlsOrUdp {
    fn tcp<T>(seed: (StartTlsStream, T)) -> (Self, T) {
        (StartTlsOrUdp::StartTls(seed.0), seed.1)
    }

    fn udp<T>(seed: (UdpSocket, T)) -> (Self, T) {
        (StartTlsOrUdp::Udp(seed.0), seed.1)
    }
}

