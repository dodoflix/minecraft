//! `ServerGamePacketListenerImpl`: per-player play-state listener *state*. It lives inside the player
//! (`ServerPlayer.connection`); the behavior (`handle*`, `tick`) is in [`super::game_packets`] as methods of
//! [`PlayCtx`](super::game_packets::PlayCtx), the transient borrow of `(server, level, player)` that exists while a
//! packet or a tick is handled (the player is taken out of the level's entity storage meanwhile, see
//! `ARCHITECTURE.md`).

use super::common_listener::{CommonListenerCookie, ServerCommonListener};
use super::player_chunk_sender::PlayerChunkSender;
pub use super::game_packets::{PlayCtx, process_queued_packets, send_next_chunks_all, tick_connection};
use super::{ConnectionSender, InboundPacket};
use crate::util::tick_throttler::TickThrottler;
use crate::world::entity::EntityId;
use mc_core::phys::Vec3;
use crate::world::entity::{Entity, ServerPlayer};
use mc_protocol::packet::Packet;
use mc_protocol::packets::play::{ClientboundPlayerPositionPacket, PositionMoveRotation, Relatives};
use crate::server::chat::signed_message::{Decoder, UnsignedDecoder};
use crate::server::chat::{OutgoingChatState, RemoteChatSession};
use mc_text::Component;
use parking_lot::Mutex;
use std::sync::Arc;
use std::sync::mpsc::Receiver;

/// `ServerGamePacketListenerImpl.CLIENT_LOADED_TIMEOUT_TIME`.
pub const CLIENT_LOADED_TIMEOUT_TIME: i32 = 60;
pub(crate) const NO_BLOCK_UPDATES_TO_ACK: i32 = -1;
pub(crate) const MAXIMUM_FLYING_TICKS: i32 = 80;
pub(crate) const MIN_SUPPORTED_BLOCKS_RESEND_TICKS: i32 = 200;
pub(crate) const INVALID_POSITION_CORRECTION_MAX_INTERVAL_TICKS: i32 = 20;
pub(crate) const MAX_DISTANCE_TO_VALID_POSITION: f64 = 1.0;

/// The mutable fields of vanilla's `ServerGamePacketListenerImpl` (`player` and `server` are the [`PlayCtx`]).
pub struct ServerGamePacketListenerImpl {
    pub(crate) common: ServerCommonListener,
    pub(crate) sender: ConnectionSender,
    pub chunk_sender: PlayerChunkSender,
    /// The connection's reader task output (vanilla's `PacketProcessor` queue), drained at the start of every tick.
    pub(crate) inbound: Receiver<InboundPacket>,
    pub(crate) tick_count: i32,
    pub(crate) ack_block_changes_up_to: i32,
    pub(crate) drop_spam_throttler: TickThrottler,
    pub(crate) chat_spam_throttler: TickThrottler,
    pub(crate) command_spam_throttler: TickThrottler,
    pub(crate) first_good_x: f64,
    pub(crate) first_good_y: f64,
    pub(crate) first_good_z: f64,
    pub(crate) last_good_x: f64,
    pub(crate) last_good_y: f64,
    pub(crate) last_good_z: f64,
    pub(crate) last_vehicle: Option<EntityId>,
    pub(crate) vehicle_first_good_x: f64,
    pub(crate) vehicle_first_good_y: f64,
    pub(crate) vehicle_first_good_z: f64,
    pub(crate) vehicle_last_good_x: f64,
    pub(crate) vehicle_last_good_y: f64,
    pub(crate) vehicle_last_good_z: f64,
    pub(crate) awaiting_position_from_client: Option<Vec3>,
    pub(crate) awaiting_teleport: i32,
    pub(crate) awaiting_teleport_time: i32,
    pub(crate) client_is_floating: bool,
    pub(crate) above_ground_tick_count: i32,
    pub(crate) client_vehicle_is_floating: bool,
    pub(crate) above_ground_vehicle_tick_count: i32,
    pub(crate) received_move_packet_count: i32,
    pub(crate) known_move_packet_count: i32,
    pub(crate) received_movement_this_tick: bool,
    pub(crate) received_position_this_tick: bool,
    pub(crate) waiting_for_switch_to_config: bool,
    pub(crate) waiting_for_respawn: bool,
    pub(crate) client_loaded_timeout_timer: i32,
    pub(crate) last_supported_blocks_send: i32,
    pub(crate) vehicle_position_last_reset_at: i32,
    /// Set once the connection's final `Disconnected` event was seen (`Connection.disconnectionHandled` source).
    pub(crate) pending_disconnect: Option<mc_text::Component>,
    pub(crate) disconnection_handled: bool,
    /// `PERFORM_RESPAWN` requested while the player was taken out of its level: `PlayerList.respawn(keepAll)` runs
    /// right after the packet batch (`Some(won_game)`).
    pub(crate) pending_respawn: Option<bool>,
    /// `server.execute(..)` tasks queued by packet handlers while the player was taken out of its level (commands
    /// must see the player in the level and every level in the server): run right after the packet batch.
    pub(crate) deferred_tasks: Vec<Box<dyn FnOnce(&mut crate::server::MinecraftServer)>>,
    /// `chatSession`, `signedMessageDecoder`.
    pub(crate) chat_session: Option<RemoteChatSession>,
    pub(crate) signed_message_decoder: Box<dyn Decoder>,
    /// The player handle's outgoing chat state (`lastSeenMessages`, `messageSignatureCache`, `nextChatIndex`); vanilla
    /// synchronizes on `lastSeenMessages`, the handle shares it with `PlayerList` broadcasts.
    pub(crate) chat_out: Arc<Mutex<OutgoingChatState>>,
    /// `ServerCommandSuggestionsProvider`: the latest unanswered request `(client id, command)` and the per-tick budget.
    pub(crate) suggestion_request: Option<(i32, String)>,
    pub(crate) suggestion_budget: bool,
}

/// What `new ServerGamePacketListenerImpl(server, connection, player, cookie)` reads from the server.
pub struct ListenerSettings {
    /// `server.getCommandSpamThresholdSeconds()` / `getChatSpamThresholdSeconds()`.
    pub command_spam_threshold_seconds: i32,
    pub chat_spam_threshold_seconds: i32,
    /// `connection.isMemoryConnection()`.
    pub memory_connection: bool,
    /// `server::enforceSecureProfile`.
    pub enforce_secure_profile: Arc<dyn Fn() -> bool + Send + Sync>,
}

impl ServerGamePacketListenerImpl {
    /// `new ServerGamePacketListenerImpl(server, connection, player, cookie)`; `now_ms` = `Util.getMillis()`;
    /// `chat_out` is the player handle's `chat_out`.
    pub fn new(
        sender: ConnectionSender,
        inbound: Receiver<InboundPacket>,
        cookie: &CommonListenerCookie,
        chat_out: Arc<Mutex<OutgoingChatState>>,
        settings: ListenerSettings,
        now_ms: i64,
    ) -> Self {
        let profile_id = cookie.profile.id;
        super::game_packets::start_services_keys_fetch();
        let ListenerSettings { command_spam_threshold_seconds, chat_spam_threshold_seconds, memory_connection, enforce_secure_profile } = settings;
        Self {
            common: ServerCommonListener::new(cookie, now_ms),
            chunk_sender: PlayerChunkSender::new(sender.clone(), memory_connection),
            sender,
            inbound,
            tick_count: 0,
            ack_block_changes_up_to: NO_BLOCK_UPDATES_TO_ACK,
            drop_spam_throttler: TickThrottler::new(20, 1480),
            chat_spam_throttler: TickThrottler::new(20, 20 * chat_spam_threshold_seconds),
            command_spam_throttler: TickThrottler::new(20, 20 * command_spam_threshold_seconds),
            first_good_x: 0.0,
            first_good_y: 0.0,
            first_good_z: 0.0,
            last_good_x: 0.0,
            last_good_y: 0.0,
            last_good_z: 0.0,
            last_vehicle: None,
            vehicle_first_good_x: 0.0,
            vehicle_first_good_y: 0.0,
            vehicle_first_good_z: 0.0,
            vehicle_last_good_x: 0.0,
            vehicle_last_good_y: 0.0,
            vehicle_last_good_z: 0.0,
            awaiting_position_from_client: None,
            awaiting_teleport: 0,
            awaiting_teleport_time: 0,
            client_is_floating: false,
            above_ground_tick_count: 0,
            client_vehicle_is_floating: false,
            above_ground_vehicle_tick_count: 0,
            received_move_packet_count: 0,
            known_move_packet_count: 0,
            received_movement_this_tick: false,
            received_position_this_tick: false,
            waiting_for_switch_to_config: false,
            // `restartClientLoadTimerAfterRespawn()`
            waiting_for_respawn: false,
            client_loaded_timeout_timer: CLIENT_LOADED_TIMEOUT_TIME,
            last_supported_blocks_send: 0,
            vehicle_position_last_reset_at: 0,
            pending_disconnect: None,
            disconnection_handled: false,
            pending_respawn: None,
            deferred_tasks: Vec::new(),
            chat_session: None,
            signed_message_decoder: Box::new(UnsignedDecoder::new(profile_id, enforce_secure_profile)),
            chat_out,
            suggestion_request: None,
            suggestion_budget: false,
        }
    }

    /// `send(packet)`.
    pub fn send<P: Packet>(&self, packet: &P) {
        self.sender.send(packet);
    }

    pub fn sender(&self) -> &ConnectionSender {
        &self.sender
    }

    /// `disconnect(reason)`: queue `ClientboundDisconnectPacket`, flush and close; the connection's final inbound
    /// event then runs `onDisconnect` from `tick_connection`.
    pub fn disconnect(&self, reason: Component) {
        self.sender.disconnect(reason);
    }

    /// `latency()` (read by `PlayerList.tick` for `UPDATE_LATENCY`).
    pub fn latency(&self) -> i32 {
        self.common.latency()
    }

    /// `isAcceptingMessages()`.
    pub fn is_accepting_messages(&self) -> bool {
        self.sender.is_connected() && !self.waiting_for_switch_to_config
    }

    /// `ackBlockChangesUpTo(packetSequenceNr)`.
    pub fn ack_block_changes_up_to(&mut self, packet_sequence_nr: i32) {
        // vanilla throws IllegalArgumentException for a negative sequence (a decoded VarInt can be negative):
        // the packet processor reports it via `onPacketError`.
        assert!(packet_sequence_nr >= 0, "Expected packet sequence nr >= 0");
        self.ack_block_changes_up_to = packet_sequence_nr.max(self.ack_block_changes_up_to);
    }

    /// `resetFlyingTicks()`.
    pub fn reset_flying_ticks(&mut self) {
        self.above_ground_tick_count = 0;
        self.above_ground_vehicle_tick_count = 0;
    }

    /// `resetPosition()` given the player's current position.
    pub fn reset_position(&mut self, pos: Vec3) {
        self.first_good_x = pos.x;
        self.first_good_y = pos.y;
        self.first_good_z = pos.z;
        self.last_good_x = pos.x;
        self.last_good_y = pos.y;
        self.last_good_z = pos.z;
    }

    /// `hasClientLoaded()`.
    pub fn has_client_loaded(&self) -> bool {
        !self.waiting_for_respawn && self.client_loaded_timeout_timer <= 0
    }

    /// `tickClientLoadTimeout()` (called from `ServerPlayer.doTick`).
    pub fn tick_client_load_timeout(&mut self) {
        if self.client_loaded_timeout_timer > 0 {
            self.client_loaded_timeout_timer -= 1;
        }
    }

    /// `markClientLoaded()`.
    pub(crate) fn mark_client_loaded(&mut self) {
        self.client_loaded_timeout_timer = 0;
    }

    /// `markClientUnloadedAfterDeath()`.
    pub fn mark_client_unloaded_after_death(&mut self) {
        self.waiting_for_respawn = true;
    }

    /// `restartClientLoadTimerAfterRespawn()`.
    pub fn restart_client_load_timer_after_respawn(&mut self) {
        self.waiting_for_respawn = false;
        self.client_loaded_timeout_timer = CLIENT_LOADED_TIMEOUT_TIME;
    }
}

impl ServerPlayer {
    /// `player.connection.teleport(x, y, z, yRot, xRot)`.
    pub fn connection_teleport(&mut self, x: f64, y: f64, z: f64, y_rot: f32, x_rot: f32) {
        self.connection_teleport_pmr(PositionMoveRotation { position: Vec3::new(x, y, z), delta_movement: Vec3::ZERO, y_rot, x_rot }, Relatives::NONE);
    }

    /// `player.connection.teleport(destination, relatives)`: bumps the teleport counter, moves the player and asks
    /// the client to confirm (`awaitingPositionFromClient`).
    pub fn connection_teleport_pmr(&mut self, destination: PositionMoveRotation, relatives: Relatives) {
        let lis = self.listener.as_mut().expect("teleport of a player without a listener");
        lis.awaiting_teleport_time = lis.tick_count;
        lis.awaiting_teleport += 1;
        if lis.awaiting_teleport == i32::MAX {
            lis.awaiting_teleport = 0;
        }
        let id = lis.awaiting_teleport;
        self.teleport_set_position(destination, relatives);
        let position = self.position();
        let lis = self.listener.as_mut().expect("checked above");
        lis.awaiting_position_from_client = Some(position);
        lis.send(&ClientboundPlayerPositionPacket { id, change: destination, relatives });
    }
}
