//! Connection/session level handlers: keep-alive & ping, client information, client commands (respawn, stats, game
//! rule values), abilities, difficulty / game mode changes, chunk batch acknowledgements, resource pack responses,
//! configuration switch and disconnection.

use super::PlayCtx;
use crate::server::network::common_listener::disconnect_unexpected_query;
use mc_protocol::packets::common::ClientInformation;
use mc_protocol::packets::play::{
    ClientboundPongResponsePacket, ClientboundStartConfigurationPacket, ServerboundChangeDifficultyPacket, ServerboundChangeGameModePacket,
    ServerboundChunkBatchReceivedPacket, ServerboundClientCommandAction, ServerboundClientCommandPacket, ServerboundClientInformationPacket,
    ServerboundConfigurationAcknowledgedPacket, ServerboundCookieResponsePacket, ServerboundCustomClickActionPacket, ServerboundCustomPayloadPacket,
    ServerboundKeepAlivePacket, ServerboundLockDifficultyPacket, ServerboundPingRequestPacket, ServerboundPlayerAbilitiesPacket, ServerboundPongPacket,
    ServerboundResourcePackPacket,
};
use mc_protocol::packets::common::ResourcePackAction;
use mc_text::{ChatFormatting, Component};

impl PlayCtx<'_> {
    /// `handleKeepAlive` (`ServerCommonPacketListenerImpl`); `received_at_ms` is `Util.getMillis()` at packet arrival.
    pub fn handle_keep_alive(&mut self, packet: &ServerboundKeepAlivePacket, received_at_ms: i64) {
        if self.lis().common.handle_keep_alive(packet.id, received_at_ms) {
            self.disconnect(crate::server::network::common_listener::timeout_disconnection_message());
        }
    }

    /// `handlePong`.
    pub fn handle_pong(&mut self, _packet: &ServerboundPongPacket) {}

    /// `handleCustomPayload`: ignored by the play listener (the brand is read in the configuration phase).
    pub fn handle_custom_payload(&mut self, _packet: &ServerboundCustomPayloadPacket) {}

    /// `handleCustomClickAction`.
    pub fn handle_custom_click_action(&mut self, packet: &ServerboundCustomClickActionPacket) {
        self.server.handle_custom_click_action(&packet.id, &packet.payload);
    }

    /// `handleResourcePackResponse`.
    pub fn handle_resource_pack_response(&mut self, packet: &ServerboundResourcePackPacket) {
        if packet.action == ResourcePackAction::Declined && self.server.is_resource_pack_required() {
            log::info!("Disconnecting {} due to resource pack {} rejection", self.player.get_profile().name, packet.id);
            self.disconnect(Component::translatable("multiplayer.requiredTexturePrompt.disconnect", vec![]));
        }
    }

    /// `handleCookieResponse`.
    pub fn handle_cookie_response(&mut self, _packet: &ServerboundCookieResponsePacket) {
        self.disconnect(disconnect_unexpected_query());
    }

    /// `handlePingRequest`.
    pub fn handle_ping_request(&mut self, packet: &ServerboundPingRequestPacket) {
        self.send(&ClientboundPongResponsePacket { time: packet.time });
    }

    /// `handleChunkBatchReceived`.
    pub fn handle_chunk_batch_received(&mut self, packet: &ServerboundChunkBatchReceivedPacket) {
        self.lis().chunk_sender.on_chunk_batch_received_by_client(packet.desired_chunks_per_tick);
    }

    /// `handleClientInformation`.
    pub fn handle_client_information(&mut self, packet: &ServerboundClientInformationPacket) {
        let was_hat_shown = self.player.is_model_part_shown_hat();
        let information: &ClientInformation = &packet.information;
        self.player.update_options(information);
        if self.player.is_model_part_shown_hat() != was_hat_shown {
            let update = crate::server::players::player_info_update(mc_protocol::packets::play::PlayerInfoAction::UpdateHat, &*self.player);
            crate::server::players::player_list::broadcast_all(self.server, &update);
        }
    }

    /// `handlePlayerAbilities`.
    pub fn handle_player_abilities(&mut self, packet: &ServerboundPlayerAbilitiesPacket) {
        let abilities = self.player.get_abilities_mut();
        abilities.flying = packet.is_flying && abilities.mayfly;
    }

    /// `handleChangeDifficulty`.
    pub fn handle_change_difficulty(&mut self, packet: &ServerboundChangeDifficultyPacket) {
        if !self.player.has_gamemaster_permission() && !self.is_singleplayer_owner() {
            log::warn!(
                "Player {} tried to change difficulty to {} without required permissions",
                self.player.get_profile().name,
                packet.difficulty.get_display_name().get_string()
            );
        } else {
            self.server.set_difficulty(packet.difficulty, false);
        }
    }

    /// `handleChangeGameMode`.
    pub fn handle_change_game_mode(&mut self, packet: &ServerboundChangeGameModePacket) {
        if !crate::server::commands::game_mode::permission_check(self.player) {
            log::warn!(
                "Player {} tried to change game mode to {} without required permissions",
                self.player.get_profile().name,
                packet.mode.get_short_display_name().get_string()
            );
        } else {
            crate::server::commands::game_mode::set_game_mode(self.server, self.level, self.player, packet.mode);
        }
    }

    /// `handleLockDifficulty`.
    pub fn handle_lock_difficulty(&mut self, packet: &ServerboundLockDifficultyPacket) {
        if self.player.has_gamemaster_permission() || self.is_singleplayer_owner() {
            self.server.set_difficulty_locked(packet.locked);
        }
    }

    /// `handleClientCommand`.
    pub fn handle_client_command(&mut self, packet: &ServerboundClientCommandPacket) {
        self.player.reset_last_action_time();
        match packet.action {
            ServerboundClientCommandAction::PerformRespawn => {
                if self.player.won_game {
                    self.player.won_game = false;
                    self.respawn_player(true);
                } else {
                    if self.player.get_health() > 0.0 {
                        return;
                    }
                    self.respawn_player(false);
                }
            }
            ServerboundClientCommandAction::RequestStats => self.player.send_stats(),
            ServerboundClientCommandAction::RequestGameruleValues => self.send_game_rule_values(),
        }
    }

    /// The `PERFORM_RESPAWN` arms: `this.player = playerList.respawn(player, keepAll, reason)`, then
    /// `resetPosition()`, `restartClientLoadTimerAfterRespawn()`, and either the end-credits `CHANGED_DIMENSION`
    /// trigger or the hardcore spectator switch. `PlayerList.respawn` moves the listener to the new player.
    fn respawn_player(&mut self, won_game: bool) {
        let reason = if won_game { crate::world::entity::RemovalReason::ChangedDimension } else { crate::world::entity::RemovalReason::Killed };
        let hardcore = self.server.is_hardcore();
        crate::server::players::player_list::respawn(self.server, self.level, self.player, won_game, reason, move |server, new_player| {
            let pos = new_player.position();
            let lis = new_player.listener.as_mut().expect("respawned player keeps its listener");
            lis.reset_position(pos);
            lis.restart_client_load_timer_after_respawn();
            if won_game {
                new_player.trigger_changed_dimension(server, crate::world::level::DimensionKey::END, crate::world::level::DimensionKey::OVERWORLD);
            } else if hardcore {
                new_player.set_game_mode(server, mc_protocol::packets::play::GameType::Spectator);
            }
        });
    }

    /// `sendGameRuleValues`.
    fn send_game_rule_values(&mut self) {
        if !self.player.has_gamemaster_permission() {
            log::warn!("Player {} tried to request game rule values without required permissions", self.player.get_profile().name);
            return;
        }
        let values = self.level.game_rules().serialized_available_rules();
        self.send(&mc_protocol::packets::play::ClientboundGameRuleValuesPacket { values });
    }

    /// `handleConfigurationAcknowledged`.
    pub fn handle_configuration_acknowledged(&mut self, _packet: &ServerboundConfigurationAcknowledgedPacket) {
        if !self.lis().waiting_for_switch_to_config {
            panic!("Client acknowledged config, but none was requested");
        }
        // Hand the connection over to a new `ServerConfigurationPacketListenerImpl` (GameNet owns that side).
        let cookie = crate::server::network::common_listener::CommonListenerCookie {
            profile: self.player.get_profile().clone(),
            latency: self.lis().latency(),
            client_information: self.player.client_information(),
            transferred: self.lis().common.transferred(),
        };
        crate::server::network::pending_player::switch_to_configuration(self.server, self.player, cookie);
    }

    /// `switchToConfig()`.
    pub fn switch_to_config(&mut self) {
        self.lis().waiting_for_switch_to_config = true;
        self.remove_player_from_world();
        self.send(&ClientboundStartConfigurationPacket);
        self.lis().sender.setup_outbound_configuration();
    }

    /// `onDisconnect(details)` / `Connection.handleDisconnection`: runs once the connection is closed.
    pub(super) fn handle_disconnection_if_closed(&mut self) {
        let lis = self.lis();
        if lis.disconnection_handled {
            return;
        }
        if let Some(reason) = lis.pending_disconnect.take() {
            lis.disconnection_handled = true;
            self.on_disconnect(&reason);
        }
    }

    /// `onDisconnect`.
    fn on_disconnect(&mut self, reason: &Component) {
        log::info!("{} lost connection: {}", self.player.get_plain_text_name(), reason.get_string());
        self.remove_player_from_world();
    }

    /// `removePlayerFromWorld`.
    fn remove_player_from_world(&mut self) {
        self.server.invalidate_status();
        let message = Component::translatable("multiplayer.player.left", vec![self.player.get_display_name().into()]).with_format(ChatFormatting::Yellow);
        crate::server::players::player_list::broadcast_system_message(self.server, message, false);
        self.player.disconnect(self.level);
        crate::server::players::player_list::remove(self.server, self.level, self.player);
    }
}
