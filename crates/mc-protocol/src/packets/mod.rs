//! Packet structs per connection state, named after the vanilla classes.
//!
//! Packets shared between states (`common`, `cookie`) are stamped out per state by macro so that
//! each state gets its own type carrying the right [`Packet::ID`](crate::packet::Packet::ID).

pub mod common;
pub mod configuration;
pub mod handshake;
pub mod login;
pub mod play;
pub mod status;
