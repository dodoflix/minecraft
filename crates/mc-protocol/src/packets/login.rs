//! `login` state.
use crate::codec::EncodeResult;
use mc_core::ident::Identifier;
use uuid::Uuid;

use super::common::cookie_packets;
use crate::codec::{DecodeResult, GameProfile, JsonComponent, Rest, read_varint, write_varint};
use crate::{Decode, Encode, Packet, packet_enum};

cookie_packets!(login);

/// `ClientboundCustomQueryPacket`. Vanilla only ever reads/writes a payload-less `DiscardedQueryPayload`;
/// `data` carries raw payload bytes (up to 1 MiB) for modded channels.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = login, flow = clientbound, name = "custom_query")]
pub struct ClientboundCustomQueryPacket {
    #[mc(var)]
    pub transaction_id: i32,
    pub id: Identifier,
    pub data: Rest<1048576>,
}

/// `ClientboundHelloPacket`
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = login, flow = clientbound, name = "hello")]
pub struct ClientboundHelloPacket {
    // vanilla: `readUtf(20)` but `writeUtf(serverId)` (default limit)
    #[mc(decode_max = 20)]
    pub server_id: String,
    pub public_key: Vec<u8>,
    pub challenge: Vec<u8>,
    pub should_authenticate: bool,
}

/// `ClientboundLoginCompressionPacket`
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = login, flow = clientbound, name = "login_compression")]
pub struct ClientboundLoginCompressionPacket {
    #[mc(var)]
    pub compression_threshold: i32,
}

/// `ClientboundLoginDisconnectPacket`: the reason travels as a JSON text component string.
#[derive(Debug, Clone, PartialEq, Encode, Decode, Packet)]
#[packet(state = login, flow = clientbound, name = "login_disconnect")]
pub struct ClientboundLoginDisconnectPacket {
    pub reason: JsonComponent,
}

/// `ClientboundLoginFinishedPacket`
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = login, flow = clientbound, name = "login_finished")]
pub struct ClientboundLoginFinishedPacket {
    pub game_profile: GameProfile,
    pub session_id: Uuid,
}

/// `ServerboundCustomQueryAnswerPacket`. Vanilla discards the answer; `payload` is the nullable
/// payload as written by `writeNullable` (`None` = client did not understand the query).
#[derive(Debug, Clone, PartialEq, Eq, Packet)]
#[packet(state = login, flow = serverbound, name = "custom_query_answer")]
pub struct ServerboundCustomQueryAnswerPacket {
    pub transaction_id: i32,
    pub payload: Option<Vec<u8>>,
}

impl Encode for ServerboundCustomQueryAnswerPacket {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        write_varint(out, self.transaction_id);
        match &self.payload {
            Some(p) => {
                out.push(1);
                out.extend_from_slice(p);
            }
            None => out.push(0),
        };
        Ok(())
    }
}
impl Decode for ServerboundCustomQueryAnswerPacket {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        let transaction_id = read_varint(input)?;
        let rest = Rest::<1048576>::decode(input)?.0;
        let payload = match rest.split_first() {
            Some((&flag, data)) if flag != 0 => Some(data.to_vec()),
            _ => None,
        };
        Ok(Self {
            transaction_id,
            payload,
        })
    }
}

/// `ServerboundHelloPacket`
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = login, flow = serverbound, name = "hello")]
pub struct ServerboundHelloPacket {
    #[mc(max = 16)]
    pub name: String,
    pub profile_id: Uuid,
}

/// `ServerboundKeyPacket`: RSA-encrypted shared secret and challenge.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = login, flow = serverbound, name = "key")]
pub struct ServerboundKeyPacket {
    pub keybytes: Vec<u8>,
    pub encrypted_challenge: Vec<u8>,
}

/// `ServerboundLoginAcknowledgedPacket`
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = login, flow = serverbound, name = "login_acknowledged")]
pub struct ServerboundLoginAcknowledgedPacket;

packet_enum! {
    /// Clientbound login packets.
    pub enum ClientboundLoginPacket {
        LoginDisconnect(ClientboundLoginDisconnectPacket),
        Hello(ClientboundHelloPacket),
        LoginFinished(ClientboundLoginFinishedPacket),
        LoginCompression(ClientboundLoginCompressionPacket),
        CustomQuery(ClientboundCustomQueryPacket),
        CookieRequest(ClientboundCookieRequestPacket),
    }
}

packet_enum! {
    /// Serverbound login packets.
    pub enum ServerboundLoginPacket {
        Hello(ServerboundHelloPacket),
        Key(ServerboundKeyPacket),
        CustomQueryAnswer(ServerboundCustomQueryAnswerPacket),
        LoginAcknowledged(ServerboundLoginAcknowledgedPacket),
        CookieResponse(ServerboundCookieResponsePacket),
    }
}
