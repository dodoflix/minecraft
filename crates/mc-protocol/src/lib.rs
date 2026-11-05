//! Minecraft Java Edition 26.3 (protocol 777) wire protocol: codecs, framing, compression,
//! encryption, authentication helpers and the handshake/status/login/configuration packets.

// Lets `#[derive(Encode, Decode, Packet)]` (which names `::mc_protocol`) work inside this crate.
extern crate self as mc_protocol;

pub mod codec;
pub mod components;
pub mod dialog;
pub mod registry_context;
pub mod packet;

/// Packet id constants per `(state, flow)`, generated from vanilla's `packets.json`.
#[allow(dead_code)]
pub mod ids {
    include!(concat!(env!("OUT_DIR"), "/ids.rs"));
}

pub use mc_protocol_macros::{Decode, Encode, Packet};

/// Vanilla protocol version (`SharedConstants.getProtocolVersion`).
pub const PROTOCOL_VERSION: i32 = 777;
pub mod auth;
pub mod net;
pub use codec::{Decode, Encode};
pub use packet::Packet;
pub mod packets;
