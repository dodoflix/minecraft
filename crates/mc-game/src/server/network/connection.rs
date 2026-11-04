//! Send side of a client connection, usable from any thread (clone-able). The tokio writer task of the connection
//! owns the socket (framing, compression, encryption — `mc_protocol::net`); everything the game thread sends goes
//! through this handle as pre-encoded payloads (`VarInt id + body`).
//!
//! Skeleton owned by the NetAgent slice (it adds the writer task, state-dependent disconnect encoding, flush
//! policy, bundle handling, closing, connection metrics).

use mc_protocol::packet::Packet;
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;

/// What the connection's writer task is asked to do.
pub enum Outbound {
    /// One packet payload (`VarInt id + body`), shared so a broadcast encodes once.
    Payload(Arc<[u8]>),
    /// Flush, then close the connection (after a disconnect packet was queued).
    Close,
}

#[derive(Clone)]
pub struct ConnectionSender {
    tx: UnboundedSender<Outbound>,
}

impl ConnectionSender {
    pub fn new(tx: UnboundedSender<Outbound>) -> Self {
        Self { tx }
    }

    /// `Connection.send(packet)`: encodes `VarInt id + body` and queues it. A closed connection drops the packet.
    pub fn send<P: Packet>(&self, packet: &P) {
        let mut buf = Vec::with_capacity(64);
        mc_protocol::codec::write_varint(&mut buf, P::ID);
        packet.encode(&mut buf);
        self.send_payload(buf.into());
    }

    pub fn send_payload(&self, payload: Arc<[u8]>) {
        let _ = self.tx.send(Outbound::Payload(payload));
    }

    pub fn close(&self) {
        let _ = self.tx.send(Outbound::Close);
    }
}
