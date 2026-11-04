//! `handshake` state (serverbound only).
use crate::{packet_enum, Decode, Encode, Packet};

/// `ClientIntent` (ids 1..=3 on the wire).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[mc(error = "Unknown connection intent: {}")]
pub enum ClientIntent {
    #[mc(id = 1)]
    Status,
    Login,
    Transfer,
}

/// `ClientIntentionPacket`
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = handshake, flow = serverbound, name = "intention")]
pub struct ClientIntentionPacket {
    #[mc(var)]
    pub protocol_version: i32,
    #[mc(max = 255)]
    pub host_name: String,
    pub port: u16,
    pub intention: ClientIntent,
}

packet_enum! {
    /// All serverbound handshake packets.
    pub enum ServerboundHandshakePacket {
        ClientIntention(ClientIntentionPacket),
    }
}
