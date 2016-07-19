//! An introduction to netmachines.
//!
//! <i>This module does not contain any code, only a lot of text.</i>
//!
//! This module introduces the reader to the *netmachines* crate. Since it
//! is built on a stack of other crates and makes no attempt in hiding the
//! fact, this introduction starts out with short introduction to these
//! other building blocks: we will start by introducing [mio] and [rotor]
//! before moving on to *netmachines* proper.
//!
//!
//! # Contents
//!
//! * [MIO](#mio)
//! * [Rotor](#rotor)
//! * [Netmachines](#netmachines)
//!
//!
//! # MIO
//!
//! [MIO](https://github.com/carllerche/mio) is the de-facto standard library
//! for doing asynchronous IO in Rust. It provides a thin layer, unified layer
//! over whatever the operating system provides for this purpose. While this
//! means it introduces very little overhead, its usage is a little arkane.
//! This is why MIO is, more often, used as a building block for higher-level,
//! more specialized libraries. Since we are using this approach as well,
//! there is no need to get into the details of MIO too much, however,
//! understanding the basic concepts will be helpful.
//!
//! MIO monitors events happening on certain entitites, called
//! [Evented][Evented]s. Network sockets for TCP and UDP are avilable as
//! eventeds. On Unix systems, there additionally are pipes, Unix sockets,
//! and everything else that has a file descriptor. These eventeds can be
//! monitored for typical IO events: whether they are readable or writable
//! and whether an error or hangup happened. These events are represented by
//! the type [EventSet].
//!
//! Like most other asynchronous frameworks, monitoring happens in the form
//! of an event loop to which control is being transfered. Eventeds and the
//! set of interesting events are registered with this loop. Instead of
//! associating events with callback functions, which would be difficult with
//! Rust’s ownership model, MIO uses a simple integer type called [Token].
//! When registering for events, the user chooses a specific token value which
//! is returned whenever events resulting from the registration are emitted.
//! That is, the event loop doesn’t own the eventeds but leaves them in the
//! hands of the user.
//!
//! When waiting for events, the event loop distinguishes between two modes:
//! level-triggered and edge-triggered events. If an event is registered to
//! be level-triggered, the event loop will continously report the event for
//! as long as the condition for event persists. If, for instance, a readable
//! event is registered for a socket, the event will be triggered as long as
//! there is data available for reading on the socket.
//!
//! With edge-triggered events, a notification will only happen when the
//! status changes. With readability, an event is reported only once when new
//! data for reading arrives, even if not all data is being read in response
//! to the event.
//!
//! Normally, event registrations survive the triggering of an event. You do
//! not need to re-register for readability after having received a readable
//! event. This behaviour can be changed using the one-shot option. If it is
//! given when registering for an event, the registration expires as soon as
//! the first event is triggered.
//!
//! All these options are given during registration using the [PollOpt] type.
//!
//! There are two more things the event loop provides: a synchronizaiton
//! channel that can be used to send data through the event loop so that
//! it is being woken up, and a facility for registering timeouts.
//!
//! All of this is provided to the user through a handler, a user-provided
//! type implementing the [Handler] trait over which the [EventLoop] is
//! generic.
//!
//!
//! # Rotor
//!
//! # Netmachines
//!
//!
//! [Evented]: ../../mio/trait.Evented.html
//! [EventLoop]: ../../mio/struct.EventLoop.html
//! [EventSet]: ../../mio/struct.EventSet.html
//! [Handler]: ../../mio/trait.Handler.html
//! [PollOpt]: ../../mio/struct.PollOpt.html
//! [Token]: ../../mio/struct.Token.html

