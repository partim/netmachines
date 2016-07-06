//! Socket traits and implementations for standard.
//!
//! Traits are used to assign sockets to differing categories according to
//! their socket type and encryption status:
//!
//!    ClearStream, SecureStream, HybridStream,
//!    CleadDgram, SecureDgram.
//!
//! These traits are implemented for both actual sockets and mock sockets.
//!
//! The traits are also used to define for which socket types a handler is
//! usable by implementing `TransportHandler<S>` for these socket types.

use std::io::{self, Read, Write};
use std::net::SocketAddr;
use rotor::mio::Evented;
use rotor::mio::tcp::{TcpListener, TcpStream};
use rotor::mio::udp::UdpSocket;
use ::error::Result;

pub trait Accept {
    type Output;

    fn accept(&self) -> Result<Option<(Self::Output, SocketAddr)>>;
}

impl Accept for TcpListener {
    type Output = TcpStream;

    fn accept(&self) -> Result<Option<(Self::Output, SocketAddr)>> {
        Ok(try!(self.accept()))
    }
}


pub trait ClearStream: Read + Write + Evented { }

impl ClearStream for TcpStream { }


pub trait SecureStream: Read + Write + Evented { }



pub trait ClearDgram: Evented {
    fn recv_from(&self, buf: &mut [u8])
                 -> io::Result<Option<(usize, SocketAddr)>>;
    fn send_to(&self, buf: &[u8], target: &SocketAddr)
               -> io::Result<Option<usize>>;
}

impl ClearDgram for UdpSocket {
    fn recv_from(&self, buf: &mut [u8])
                 -> io::Result<Option<(usize, SocketAddr)>> {
        self.recv_from(buf)
    }

    fn send_to(&self, buf: &[u8], target: &SocketAddr)
               -> io::Result<Option<usize>> {
        self.send_to(buf, target)
    }
}


