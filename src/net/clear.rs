//! Unencrypted machines.

use rotor::{Compose2, EventSet, Machine, Response, Scope, Void};
use rotor::mio::tcp::{TcpListener, TcpStream};
use rotor::mio::udp::UdpSocket;
use super::machines::{ClientMachine, ServerMachine, TcpTransport,
                      UdpTransport};
use ::handlers::{AcceptHandler, CreateHandler};
use ::utils::ResponseExt;


//------------ TcpServer -----------------------------------------------------

pub struct TcpServer<X, H>(ServerMachine<X, TcpListener, H,
                                            TcpTransport<X, H::Output>>)
           where H: AcceptHandler<X, TcpStream>;

impl<X, H: AcceptHandler<X, TcpStream>> TcpServer<X, H> {
}

impl<X, H> Machine for TcpServer<X, H>
           where H: AcceptHandler<X, TcpStream> {
    type Context = X;
    type Seed = <ServerMachine<X, TcpListener, H,
                 TcpTransport<X, H::Output>> as Machine>::Seed;

    wrapped_machine!(ServerMachine, TcpServer);
}


//------------ TcpClient ----------------------------------------------------

pub struct TcpClient<X, S, H>(ClientMachine<X, S, TcpStream, H,
                                            TcpTransport<X, H::Output>>)
                     where S: Send, H: CreateHandler<S, TcpStream>;

impl<X, S: Send, H: CreateHandler<S, TcpStream>> TcpClient<X, S, H> {
}

impl<X, S, H> Machine for TcpClient<X, S, H>
              where S: Send, H: CreateHandler<S, TcpStream> {
    type Context = X;
    type Seed = <ClientMachine<X, S, TcpStream, H,
                               TcpTransport<X, H::Output>> as Machine>::Seed;

    wrapped_machine!(ClientMachine, TcpClient);
}


//------------ UdpClient ----------------------------------------------------

pub struct UdpClient<X, S, H>(ClientMachine<X, S, UdpSocket, H,
                                            UdpTransport<X, H::Output>>)
                     where S: Send, H: CreateHandler<S, UdpSocket>;

impl<X, S: Send, H: CreateHandler<S, UdpSocket>> UdpClient<X, S, H> {
}

impl<X, S, H> Machine for UdpClient<X, S, H>
              where S: Send, H: CreateHandler<S, UdpSocket> {
    type Context = X;
    type Seed = <ClientMachine<X, S, UdpSocket, H,
                               UdpTransport<X, H::Output>> as Machine>::Seed;

    wrapped_machine!(ClientMachine, UdpClient);
}


//------------ TcpUdpServer -------------------------------------------------

pub struct TcpUdpServer<X, T, S, U>(Compose2<TcpServer<X, T>,
                                             UdpClient<X, S, U>>)
           where T: AcceptHandler<X, TcpStream>,
                 S: Send, U: CreateHandler<S, UdpSocket>;

impl<X, T, S, U> TcpUdpServer<X, T, S, U>
                 where T: AcceptHandler<X, TcpStream>,
                       S: Send, U: CreateHandler<S, UdpSocket> {
}

impl<X, T, S, U> Machine for TcpUdpServer<X, T, S, U>
                 where T: AcceptHandler<X, TcpStream>,
                       S: Send, U: CreateHandler<S, UdpSocket> {
    type Context = X;
    type Seed = <Compose2<TcpServer<X, T>,
                          UdpClient<X, S, U>> as Machine>::Seed;

    wrapped_machine!(Compose2, TcpUdpServer);
}


//------------ TcpUdpClient -------------------------------------------------

pub struct TcpUdpClient<X, TS, US, TH, UH>(Compose2<TcpClient<X, TS, TH>,
                                                    UdpClient<X, US, UH>>)
           where TS: Send, TH: CreateHandler<TS, TcpStream>,
                 US: Send, UH: CreateHandler<US, UdpSocket>;

impl<X, TS, US, TH, UH> TcpUdpClient<X, TS, US, TH, UH>
                        where TS: Send, TH: CreateHandler<TS, TcpStream>,
                              US: Send, UH: CreateHandler<US, UdpSocket> {
}

impl<X, TS, US, TH, UH> Machine for TcpUdpClient<X, TS, US, TH, UH>
                        where TS: Send, TH: CreateHandler<TS, TcpStream>,
                              US: Send, UH: CreateHandler<US, UdpSocket> {
    type Context = X;
    type Seed = <Compose2<TcpClient<X, TS, TH>,
                          UdpClient<X, US, UH>> as Machine>::Seed;

    wrapped_machine!(Compose2, TcpUdpClient);
}

