//! `Packet` trait, connection states, and the `packet_enum!` dispatcher.
use crate::codec::{Decode, Encode};

/// Vanilla `ConnectionProtocol`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConnectionState {
    Handshake,
    Status,
    Login,
    Configuration,
    Play,
}

impl ConnectionState {
    /// Vanilla `ConnectionProtocol.id()`.
    pub fn id(self) -> &'static str {
        match self {
            Self::Handshake => "handshake",
            Self::Status => "status",
            Self::Login => "login",
            Self::Configuration => "configuration",
            Self::Play => "play",
        }
    }
}

/// Vanilla `PacketFlow`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PacketFlow {
    Serverbound,
    Clientbound,
}

impl PacketFlow {
    pub fn id(self) -> &'static str {
        match self {
            Self::Serverbound => "serverbound",
            Self::Clientbound => "clientbound",
        }
    }
}

/// A packet body (without its id). `ID` comes from vanilla's `packets.json`.
pub trait Packet: Encode + Decode {
    const ID: i32;
    /// Resource name, e.g. `minecraft:keep_alive`.
    const NAME: &'static str;
    const STATE: ConnectionState;
    const FLOW: PacketFlow;
}

/// Every `(id, resource name)` of a state/direction, sorted by id.
pub fn packet_names(state: ConnectionState, flow: PacketFlow) -> &'static [(i32, &'static str)] {
    use crate::ids;
    use ConnectionState::*;
    use PacketFlow::*;
    match (state, flow) {
        (Handshake, Serverbound) => ids::handshake::serverbound::NAMES,
        (Handshake, Clientbound) => &[],
        (Status, Serverbound) => ids::status::serverbound::NAMES,
        (Status, Clientbound) => ids::status::clientbound::NAMES,
        (Login, Serverbound) => ids::login::serverbound::NAMES,
        (Login, Clientbound) => ids::login::clientbound::NAMES,
        (Configuration, Serverbound) => ids::configuration::serverbound::NAMES,
        (Configuration, Clientbound) => ids::configuration::clientbound::NAMES,
        (Play, Serverbound) => ids::play::serverbound::NAMES,
        (Play, Clientbound) => ids::play::clientbound::NAMES,
    }
}

/// Resource name of packet `id`, if any.
pub fn packet_name(state: ConnectionState, flow: PacketFlow, id: i32) -> Option<&'static str> {
    packet_names(state, flow).iter().find(|(i, _)| *i == id).map(|(_, n)| *n)
}

/// Declares an enum with one variant per packet of a `(state, flow)` plus id-dispatched
/// encode/decode (vanilla `IdDispatchCodec`).
#[macro_export]
macro_rules! packet_enum {
    ($(#[$m:meta])* $vis:vis enum $name:ident { $($variant:ident($ty:ty)),* $(,)? }) => {
        $(#[$m])*
        #[derive(Debug, Clone, PartialEq)]
        #[allow(clippy::large_enum_variant)]
        $vis enum $name { $($variant($ty)),* }

        impl $name {
            /// `(id, resource name)` of every packet in this enum, in declaration order.
            pub const PACKETS: &'static [(i32, &'static str)] =
                &[$((<$ty as $crate::packet::Packet>::ID, <$ty as $crate::packet::Packet>::NAME)),*];

            /// Packet id on the wire.
            pub fn id(&self) -> i32 {
                match self { $(Self::$variant(_) => <$ty as $crate::packet::Packet>::ID),* }
            }

            /// Resource name, e.g. `minecraft:hello`.
            pub fn name(&self) -> &'static str {
                match self { $(Self::$variant(_) => <$ty as $crate::packet::Packet>::NAME),* }
            }

            pub fn state(&self) -> $crate::packet::ConnectionState {
                match self { $(Self::$variant(_) => <$ty as $crate::packet::Packet>::STATE),* }
            }

            pub fn flow(&self) -> $crate::packet::PacketFlow {
                match self { $(Self::$variant(_) => <$ty as $crate::packet::Packet>::FLOW),* }
            }

            /// Append the packet body (no id).
            pub fn encode_body(&self, out: &mut ::std::vec::Vec<u8>) {
                match self { $(Self::$variant(p) => $crate::codec::Encode::encode(p, out)),* }
            }

            /// Append `VarInt id` + body: the uncompressed, unframed packet payload.
            pub fn encode_payload(&self, out: &mut ::std::vec::Vec<u8>) {
                $crate::codec::write_varint(out, self.id());
                self.encode_body(out);
            }

            /// Decode a body whose id is already known.
            pub fn decode_body(id: i32, input: &mut &[u8]) -> ::std::result::Result<Self, $crate::codec::DecodeError> {
                $(
                    if id == <$ty as $crate::packet::Packet>::ID {
                        return <$ty as $crate::codec::Decode>::decode(input).map(Self::$variant).map_err(|e| {
                            $crate::codec::DecodeError::Message(::std::format!(
                                "Failed to decode packet '{}/{}': {}",
                                <$ty as $crate::packet::Packet>::FLOW.id(),
                                <$ty as $crate::packet::Packet>::NAME,
                                e
                            ))
                        });
                    }
                )*
                ::std::result::Result::Err($crate::codec::DecodeError::Message(::std::format!("Received unknown packet id {}", id)))
            }

            /// Decode a full payload (`VarInt id` + body), rejecting trailing bytes like vanilla's `PacketDecoder`.
            pub fn decode_payload(mut payload: &[u8]) -> ::std::result::Result<Self, $crate::codec::DecodeError> {
                let id = $crate::codec::read_varint(&mut payload)?;
                let packet = Self::decode_body(id, &mut payload)?;
                if !payload.is_empty() {
                    return ::std::result::Result::Err($crate::codec::DecodeError::Message(::std::format!(
                        "Packet {}/{}/{} was larger than I expected, found {} bytes extra whilst reading packet {}",
                        packet.state().id(),
                        packet.flow().id(), packet.name(), payload.len(), packet.name()
                    )));
                }
                Ok(packet)
            }
        }

        $(impl ::std::convert::From<$ty> for $name {
            fn from(p: $ty) -> Self { Self::$variant(p) }
        })*
    };
}
