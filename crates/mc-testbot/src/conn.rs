//! Connection plumbing: typed packet send/receive with logging, over `mc_protocol::net`.
use mc_protocol::codec::EncodeResult;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use mc_protocol::net::{NetError, PacketReader, PacketWriter};
use mc_protocol::packet::{ConnectionState, PacketFlow};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::TcpStream;

use crate::error::BotError;
use crate::log::{Dir, PacketLog};

pub type SharedLog = Arc<Mutex<PacketLog>>;

/// A `packet_enum!` type usable on the wire.
pub trait WirePacket: std::fmt::Debug + Sized {
    const STATE: ConnectionState;
    const FLOW: PacketFlow;
    fn decode_payload(payload: &[u8]) -> Result<Self, mc_protocol::codec::DecodeError>;
    fn encode_payload(&self, out: &mut Vec<u8>);
    fn packet_id(&self) -> i32;
    fn packet_name(&self) -> &'static str;
}

/// Implements [`WirePacket`] for a `packet_enum!` enum.
#[macro_export]
macro_rules! wire_packet {
    ($ty:ty, $state:ident, $flow:ident) => {
        impl $crate::conn::WirePacket for $ty {
            const STATE: ::mc_protocol::packet::ConnectionState = ::mc_protocol::packet::ConnectionState::$state;
            const FLOW: ::mc_protocol::packet::PacketFlow = ::mc_protocol::packet::PacketFlow::$flow;
            fn decode_payload(payload: &[u8]) -> Result<Self, ::mc_protocol::codec::DecodeError> {
                <$ty>::decode_payload(payload)
            }
            fn encode_payload(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
                <$ty>::encode_payload(self, out)?;
                Ok(())
            }
            fn packet_id(&self) -> i32 {
                self.id()
            }
            fn packet_name(&self) -> &'static str {
                self.name()
            }
        }
    };
}

use mc_protocol::packets::play::{ClientboundPlayPacket, ServerboundPlayPacket};
use mc_protocol::packets::{configuration::*, handshake::*, login::*, status::*};
wire_packet!(ServerboundHandshakePacket, Handshake, Serverbound);
wire_packet!(ServerboundStatusPacket, Status, Serverbound);
wire_packet!(ClientboundStatusPacket, Status, Clientbound);
wire_packet!(ServerboundLoginPacket, Login, Serverbound);
wire_packet!(ClientboundLoginPacket, Login, Clientbound);
wire_packet!(ServerboundConfigurationPacket, Configuration, Serverbound);
wire_packet!(ClientboundConfigurationPacket, Configuration, Clientbound);

pub fn short_name(name: &str) -> &str {
    name.strip_prefix("minecraft:").unwrap_or(name)
}

/// Record one packet (either direction) into the shared log.
pub fn log_packet<P: WirePacket>(log: &SharedLog, dir: Dir, p: &P, payload: &[u8]) {
    if let Ok(mut l) = log.lock() {
        l.record(dir, P::STATE.id(), p.packet_id(), short_name(p.packet_name()), format!("{p:?}"), payload);
    }
}

/// Record a payload that failed to decode (name from `packets.json`, debug = the error).
pub fn log_undecodable(log: &SharedLog, state: ConnectionState, flow: PacketFlow, payload: &[u8], err: &str) {
    let id = mc_protocol::codec::read_varint(&mut &payload[..]).unwrap_or(-1);
    let name = mc_protocol::packet::packet_name(state, flow, id).map(short_name).map(str::to_string).unwrap_or(format!("?{id}"));
    if let Ok(mut l) = log.lock() {
        let dir = if flow == PacketFlow::Clientbound { Dir::In } else { Dir::Out };
        l.record(dir, state.id(), id, &name, format!("<decode error: {err}>"), payload);
    }
}

pub fn encode<P: WirePacket>(p: &P) -> Vec<u8> {
    let mut buf = Vec::new();
    p.encode_payload(&mut buf);
    buf
}

/// Blocking-style connection used for the handshake/login/configuration phases.
pub struct Conn {
    pub reader: PacketReader<OwnedReadHalf>,
    pub writer: PacketWriter<OwnedWriteHalf>,
    pub log: SharedLog,
    pub timeout: Duration,
}

impl Conn {
    pub async fn connect(host: &str, port: u16, timeout: Duration, log: SharedLog) -> Result<Conn, BotError> {
        let stream = tokio::time::timeout(timeout, TcpStream::connect((host, port)))
            .await
            .map_err(|_| BotError::Timeout(format!("connecting to {host}:{port}")))??;
        stream.set_nodelay(true)?;
        let (r, w) = stream.into_split();
        Ok(Conn { reader: PacketReader::new(r), writer: PacketWriter::new(w), log, timeout })
    }

    pub async fn send<P: WirePacket>(&mut self, p: &P) -> Result<(), BotError> {
        let payload = encode(p);
        log_packet(&self.log, Dir::Out, p, &payload);
        self.writer.write_payload(&payload).await?;
        self.writer.flush().await?;
        Ok(())
    }

    pub async fn recv<P: WirePacket>(&mut self) -> Result<P, BotError> {
        let payload = tokio::time::timeout(self.timeout, self.reader.read_payload())
            .await
            .map_err(|_| BotError::Timeout(format!("waiting for a {} packet", P::STATE.id())))?
            .map_err(|e| match e {
                NetError::Eof => BotError::Closed,
                e => e.into(),
            })?;
        match P::decode_payload(&payload) {
            Ok(p) => {
                log_packet(&self.log, Dir::In, &p, &payload);
                Ok(p)
            }
            Err(e) => {
                log_undecodable(&self.log, P::STATE, P::FLOW, &payload, &e.to_string());
                Err(e.into())
            }
        }
    }
}
wire_packet!(ClientboundPlayPacket, Play, Clientbound);
wire_packet!(ServerboundPlayPacket, Play, Serverbound);
