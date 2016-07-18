//! State machines using actual network sockets.
//!
//! There is a multitude of machines available in here. They fall into one
//! of three basic categories: transports, servers, and clients.
//!
//! *Transports* are simple wrappers around a transport socket. They either
//! need to be added to a loop before it is started or, if the machine is
//! combined with other machines, using a seed of a a pair of the socket
//! and the transport handler’s seed during rotor’s `create()` stage.
//!
//! *Servers* react to request coming in from the network. For stream sockets,
//! they combine a listening socket and accept handler with the transports
//! created from accepting incoming streams. Since datagram sockets don’t
//! have connections, there are no servers for them as such. However, there
//! are combined server machines for stream and datagram sockets. With these,
//! the datagram part is really just a transport.
//!
//! *Clients* react to request from within the application itself, typcially
//! by communicating through the network. Clients typically consist of a
//! request machine wrapping a request handler which can add transports to
//! the client on the fly.
//!
//! For network machines, we designate the transports provided by these
//! machines using the name of the transport protocols in question: `Tcp`
//! for unencrypted stream sockets, `Udp` for unencrypted datagram sockets,
//! and `Tls` for encrypted stream sockets. Currently, there is no standard
//! implementation for encrypted datagram sockets (it would be called `Dtls`)
//! as it appears that protocols differ slightly in their use of DTLS. There
//! may, however, eventually be building blocks for DTLS machines once we
//! have some experience with practical implementations.
//!
//! For encryption, there is a choice of different crates: [openssl],
//! [security-framework], and [rustls]. We will likely standardize on the
//! latter once it becomes stable.
//!
//! The actual machines are defined in sub-modules; [clear] for those using
//! only unencrypted sockets and one by the name of the TLS dependency for
//! those also involving encrypted sockets. All machines from the [clear]
//! module are also re-imported into this module for your convenience. We
//! didn’t do that for encrypted machines to make it explicit which TLS
//! dependency you are using.
//!
//! The set of combined machines is not yet complete. If you are missing a
//! particular combination, feel free to open a Github issues or, better yet,
//! provide a pull request.
//!
//! [clear]: clear/index.html
//! [openssl]: https://crates.io/crates/openssl
//! [security-framework]: https://crates.io/crates/security-framework
//! [rustls]: https://github.com/ctz/rustls

pub use self::clear::*;

pub mod clear;
pub mod machines;

#[cfg(feature = "openssl")] pub mod openssl;
