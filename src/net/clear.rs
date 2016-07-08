//! Unencrypted machines.

use rotor::{Compose2, EventSet, GenericScope, Machine, Response, Scope, Void};
use rotor::mio::tcp::{TcpListener, TcpStream};
use rotor::mio::udp::UdpSocket;
use super::machines::{ClientMachine, ServerMachine, TransportMachine};
use ::handlers::{AcceptHandler, RequestHandler, TransportHandler};
use ::machines::RequestMachine;
use ::utils::ResponseExt;
use ::sync::Funnel;


//------------ TcpServer -----------------------------------------------------

pub struct TcpServer<X, H>(ServerMachine<X, TcpListener, H>)
           where H: AcceptHandler<TcpStream>;

impl<X, H: AcceptHandler<TcpStream>> TcpServer<X, H> {
}

impl<X, H: AcceptHandler<TcpStream>> Machine for TcpServer<X, H> {
    type Context = X;
    type Seed = <ServerMachine<X, TcpListener, H> as Machine>::Seed;

    wrapped_machine!(ServerMachine, TcpServer);
}


//------------ TcpClient ----------------------------------------------------

pub struct TcpClient<X, RH, TH>(ClientMachine<X, TcpStream, RH, TH>)
                     where RH: RequestHandler<TcpStream>,
                           TH: TransportHandler<TcpStream, Seed=RH::Seed>;

impl<X, RH, TH> TcpClient<X, RH, TH>
                where RH: RequestHandler<TcpStream>,
                      TH: TransportHandler<TcpStream, Seed=RH::Seed> {
    pub fn new<S: GenericScope>(handler: RH, scope: &mut S)
               -> (Self, Funnel<RH::Request>) {
        let (m, f) = ClientMachine::new(handler, scope);
        (TcpClient(m), f)
    }
}

impl<X, RH, TH> Machine for TcpClient<X, RH, TH>
                where RH: RequestHandler<TcpStream>,
                      TH: TransportHandler<TcpStream, Seed=RH::Seed> {
    type Context = X;
    type Seed = <ClientMachine<X, TcpStream, RH, TH> as Machine>::Seed;

    wrapped_machine!(ClientMachine, TcpClient);
}


//------------ UdpServer ----------------------------------------------------

pub struct UdpServer<X, H>(TransportMachine<X, UdpSocket, H>)
           where H: TransportHandler<UdpSocket>;

impl<X, H: TransportHandler<UdpSocket>> UdpServer<X, H> {
}

impl<X, H: TransportHandler<UdpSocket>> Machine for UdpServer<X, H> {
    type Context = X;
    type Seed = (UdpSocket, H::Seed);

    wrapped_machine!(TransportMachine, UdpServer);
}


//------------ UdpClient ----------------------------------------------------

pub struct UdpClient<X, RH, TH>(ClientMachine<X, UdpSocket, RH, TH>)
                     where RH: RequestHandler<UdpSocket>,
                           TH: TransportHandler<UdpSocket, Seed=RH::Seed>;

impl<X, RH, TH> UdpClient<X, RH, TH>
                where RH: RequestHandler<UdpSocket>,
                      TH: TransportHandler<UdpSocket, Seed=RH::Seed> {
    pub fn new<S: GenericScope>(handler: RH, scope: &mut S)
               -> (Self, Funnel<RH::Request>) {
        let (m, f) = ClientMachine::new(handler, scope);
        (UdpClient(m), f)
    }
}

impl<X, RH, TH> Machine for UdpClient<X, RH, TH>
                where RH: RequestHandler<UdpSocket>,
                      TH: TransportHandler<UdpSocket, Seed=RH::Seed> {
    type Context = X;
    type Seed = <ClientMachine<X, UdpSocket, RH, TH> as Machine>::Seed;

    wrapped_machine!(ClientMachine, UdpClient);
}


//------------ TcpUdpServer -------------------------------------------------

pub struct TcpUdpServer<X, AH, UH>(Compose2<TcpServer<X, AH>,
                                            UdpServer<X, UH>>)
           where AH: AcceptHandler<TcpStream>,
                 UH: TransportHandler<UdpSocket>;

impl<X, AH, UH> TcpUdpServer<X, AH, UH>
                where AH: AcceptHandler<TcpStream>,
                      UH: TransportHandler<UdpSocket> {
}
                
impl<X, AH, UH> Machine for TcpUdpServer<X, AH, UH>
                where AH: AcceptHandler<TcpStream>,
                      UH: TransportHandler<UdpSocket> {
    type Context = X;
    type Seed = <Compose2<TcpServer<X, AH>,
                          UdpServer<X, UH>> as Machine>::Seed;

    wrapped_machine!(Compose2, TcpUdpServer);
}


//------------ TcpUdpClient -------------------------------------------------

pub struct TcpUdpClient<X, RH, TH, UH>(
    RTU<RequestMachine<X, TcpOrUdp, RH>,
        TransportMachine<X, TcpStream, TH>,
        TransportMachine<X, UdpSocket, UH>>)
    where RH: RequestHandler<TcpOrUdp>,
          TH: TransportHandler<TcpStream, Seed=RH::Seed>,
          UH: TransportHandler<UdpSocket, Seed=RH::Seed>;

enum RTU<R, T, U> {
    Req(R),
    Tcp(T),
    Udp(U)
}

impl<X, RH, TH, UH> TcpUdpClient<X, RH, TH, UH>
                    where RH: RequestHandler<TcpOrUdp>,
                          TH: TransportHandler<TcpStream, Seed=RH::Seed>,
                          UH: TransportHandler<UdpSocket, Seed=RH::Seed> {
    fn req(req: RequestMachine<X, TcpOrUdp, RH>) -> Self {
        TcpUdpClient(RTU::Req(req))
    }

    fn tcp(tcp: TransportMachine<X, TcpStream, TH>) -> Self {
        TcpUdpClient(RTU::Tcp(tcp))
    }

    fn udp(udp: TransportMachine<X, UdpSocket, UH>) -> Self {
        TcpUdpClient(RTU::Udp(udp))
    }
}

impl<X, RH, TH, UH> Machine for TcpUdpClient<X, RH, TH, UH>
                    where RH: RequestHandler<TcpOrUdp>,
                          TH: TransportHandler<TcpStream, Seed=RH::Seed>,
                          UH: TransportHandler<UdpSocket, Seed=RH::Seed> {
    type Context = X;
    type Seed = (TcpOrUdp, RH::Seed);

    fn create(seed: Self::Seed, scope: &mut Scope<X>)
              -> Response<Self, Void> {
        match seed.0 {
            TcpOrUdp::Tcp(tcp) => {
                TransportMachine::create((tcp, seed.1), scope)
                                 .map_self(TcpUdpClient::tcp)
            }
            TcpOrUdp::Udp(udp) => {
                TransportMachine::create((udp, seed.1), scope)
                                 .map_self(TcpUdpClient::udp)
            }
        }
    }            

    fn ready(self, events: EventSet, scope: &mut Scope<X>)
             -> Response<Self, Self::Seed> {
        match self.0 {
            RTU::Req(req) => {
                req.ready(events, scope).map_self(TcpUdpClient::req)
            }
            RTU::Tcp(tcp) => {
                tcp.ready(events, scope).map(TcpUdpClient::tcp,
                                             TcpOrUdp::tcp)
            }
            RTU::Udp(udp) => {
                udp.ready(events, scope).map(TcpUdpClient::udp, TcpOrUdp::udp)
            }
        }
    }

    fn spawned(self, scope: &mut Scope<X>) -> Response<Self, Self::Seed> {
        match self.0 {
            RTU::Req(req) => {
                req.spawned(scope).map_self(TcpUdpClient::req)
            }
            RTU::Tcp(tcp) => {
                tcp.spawned(scope).map(TcpUdpClient::tcp, TcpOrUdp::tcp)
            }
            RTU::Udp(udp) => {
                udp.spawned(scope).map(TcpUdpClient::udp, TcpOrUdp::udp)
            }
        }
    }

    fn timeout(self, scope: &mut Scope<X>) -> Response<Self, Self::Seed> {
        match self.0 {
            RTU::Req(req) => {
                req.timeout(scope).map_self(TcpUdpClient::req)
            }
            RTU::Tcp(tcp) => {
                tcp.timeout(scope).map(TcpUdpClient::tcp, TcpOrUdp::tcp)
            }
            RTU::Udp(udp) => {
                udp.timeout(scope).map(TcpUdpClient::udp, TcpOrUdp::udp)
            }
        }
    }

    fn wakeup(self, scope: &mut Scope<X>) -> Response<Self, Self::Seed> {
        match self.0 {
            RTU::Req(req) => {
                req.wakeup(scope).map_self(TcpUdpClient::req)
            }
            RTU::Tcp(tcp) => {
                tcp.wakeup(scope).map(TcpUdpClient::tcp, TcpOrUdp::tcp)
            }
            RTU::Udp(udp) => {
                udp.wakeup(scope).map(TcpUdpClient::udp, TcpOrUdp::udp)
            }
        }
    }
}


//------------ TcpOrUdp ------------------------------------------------------

pub enum TcpOrUdp {
    Tcp(TcpStream),
    Udp(UdpSocket)
}


impl TcpOrUdp {
    fn tcp<T>(seed: (TcpStream, T)) -> (Self, T) {
        (TcpOrUdp::Tcp(seed.0), seed.1)
    }

    fn udp<T>(seed: (UdpSocket, T)) -> (Self, T) {
        (TcpOrUdp::Udp(seed.0), seed.1)
    }
}

