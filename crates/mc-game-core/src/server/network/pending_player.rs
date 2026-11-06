//! The hand-off from the configuration phase to play: what `PlayerList.placeNewPlayer` receives, and the events the
//! connection's reader task feeds the play listener.

use super::common_listener::CommonListenerCookie;
use super::connection::{ConnectionSender, DisconnectionDetails};
use mc_protocol::packets::play::ServerboundPlayPacket;
use std::sync::mpsc::Receiver;

/// What the reader task of a connection in the play protocol sends to the game thread.
pub enum InboundPacket {
    /// A decoded serverbound play packet. `received_at_ms` is `Util.getMillis()` at decode time (keep-alive latency).
    Packet {
        packet: Box<ServerboundPlayPacket>,
        received_at_ms: i64,
    },
    /// The connection ended (always the last event): end of stream, timeout, a decode error
    /// (`Internal Exception: ...`) or the reason of a local `disconnect`.
    Disconnected(DisconnectionDetails),
}

/// `placeNewPlayer(connection, player, cookie)` minus the player entity (built by the spawn task): the connection of a
/// client that finished configuration.
pub struct PendingPlayer {
    /// Profile, latency, client information and the transfer flag carried over from the configuration listener.
    pub cookie: CommonListenerCookie,
    pub sender: ConnectionSender,
    /// Events of the reader task; owned and drained by the play listener at the start of every tick.
    pub inbound: Receiver<InboundPacket>,
}
