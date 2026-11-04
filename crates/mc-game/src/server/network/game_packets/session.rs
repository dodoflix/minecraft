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
use mc_core::ident::Identifier;
use mc_text::{ChatFormatting, Component};
use crate::world::entity::Entity;

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
        // `server.handleCustomClickAction(id, payload)`: dialog click actions are not handled by any slice yet
        log::debug!("Unhandled custom click action {} from {}", packet.id, self.player.get_plain_text_name());
    }

    /// `handleResourcePackResponse`.
    pub fn handle_resource_pack_response(&mut self, packet: &ServerboundResourcePackPacket) {
        if packet.action == ResourcePackAction::Declined && self.server.properties().server_resource_pack_info.as_ref().is_some_and(|p| p.is_required) {
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
        self.player.update_options(information.clone());
        self.player.sync_handle();
        if self.player.is_model_part_shown_hat() != was_hat_shown
            && let Some(handle) = self.player.handle.clone()
        {
            use mc_protocol::packets::play::ClientboundPlayerInfoUpdateAction;
            let update = crate::server::players::player_handle::create_info_update([ClientboundPlayerInfoUpdateAction::UpdateHat].into_iter().collect(), [&handle]);
            self.server.player_list.broadcast_all(&update);
        }
    }

    /// `handlePlayerAbilities`.
    pub fn handle_player_abilities(&mut self, packet: &ServerboundPlayerAbilitiesPacket) {
        let abilities = &mut self.player.player.abilities;
        abilities.flying = packet.is_flying && abilities.mayfly;
    }

    /// `handleChangeDifficulty`.
    pub fn handle_change_difficulty(&mut self, packet: &ServerboundChangeDifficultyPacket) {
        use crate::util::game_modes::DifficultyExt;
        let difficulty = mc_world::level_storage::Difficulty::by_id(packet.difficulty as i32);
        if !self.has_gamemaster_permission() && !self.is_singleplayer_owner() {
            log::warn!(
                "Player {} tried to change difficulty to {} without required permissions",
                self.player.get_profile().name,
                difficulty.get_display_name().get_string()
            );
        } else {
            self.server.set_difficulty(difficulty, false);
        }
    }

    /// `handleChangeGameMode`: `GameModeCommand.PERMISSION_CHECK` (gamemaster) then `GameModeCommand.setGameMode`.
    pub fn handle_change_game_mode(&mut self, packet: &ServerboundChangeGameModePacket) {
        use crate::util::game_modes::GameTypeExt;
        let mode = mc_world::level_storage::GameType::by_id(packet.mode as i32);
        if !self.has_gamemaster_permission() {
            log::warn!(
                "Player {} tried to change game mode to {} without required permissions",
                self.player.get_profile().name,
                mode.get_short_display_name().get_string()
            );
            return;
        }
        if self.player.change_game_mode_for_player(match packet.mode {
            mc_protocol::packets::play::GameType::Survival => mc_data::enums::GameType::Survival,
            mc_protocol::packets::play::GameType::Creative => mc_data::enums::GameType::Creative,
            mc_protocol::packets::play::GameType::Adventure => mc_data::enums::GameType::Adventure,
            mc_protocol::packets::play::GameType::Spectator => mc_data::enums::GameType::Spectator,
        }) {
            self.player.sync_handle();
            if let Some(handle) = self.player.handle.clone() {
                use mc_protocol::packets::play::ClientboundPlayerInfoUpdateAction;
                let update = crate::server::players::player_handle::create_info_update([ClientboundPlayerInfoUpdateAction::UpdateGameMode].into_iter().collect(), [&handle]);
                self.server.player_list.broadcast_all(&update);
            }
            // `logGamemodeChange`: `source.sendSuccess(commands.gamemode.success.self)`
            if self.server.game_rule(crate::world::level::gamerules::GameRules::SEND_COMMAND_FEEDBACK) {
                let name = Component::translatable(format!("gameMode.{}", mode.get_name()), vec![]);
                self.player.send_system_message(Component::translatable("commands.gamemode.success.self", vec![name.into()]));
            }
        }
    }

    /// `handleLockDifficulty`.
    pub fn handle_lock_difficulty(&mut self, packet: &ServerboundLockDifficultyPacket) {
        if self.has_gamemaster_permission() || self.is_singleplayer_owner() {
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
            ServerboundClientCommandAction::RequestStats => self.sys().send_stats(self),
            ServerboundClientCommandAction::RequestGameruleValues => self.send_game_rule_values(),
        }
    }

    /// The `PERFORM_RESPAWN` arms: `this.player = playerList.respawn(player, keepAll, reason)` (the new player keeps
    /// this listener: `resetPosition()`, `restartClientLoadTimerAfterRespawn()`, the end-credits `CHANGED_DIMENSION`
    /// trigger or the hardcore spectator switch happen on it). `PlayerList.respawn` is implemented by the
    /// player-list slice through [`PlayerSystems::respawn_player`].
    fn respawn_player(&mut self, won_game: bool) {
        use crate::world::entity::RemovalReason;
        let hardcore = self.server.world_data.lock().settings.difficulty_settings.hardcore;
        // the player is taken out of its level while this packet is handled: the respawn runs right after it
        crate::server::players::respawn::queue_respawn(
            &*self.server,
            self.level.dimension.clone(),
            self.player.base_id(),
            won_game,
            if won_game { RemovalReason::ChangedDimension } else { RemovalReason::Killed },
            move |server, player| {
                let position = player.position();
                if let Some(listener) = player.listener.as_mut() {
                    listener.reset_position(position);
                    listener.restart_client_load_timer_after_respawn();
                }
                if !won_game && hardcore && player.change_game_mode_for_player(mc_data::enums::GameType::Spectator) {
                    player.sync_handle();
                    if let Some(handle) = player.handle.clone() {
                        let packet = crate::server::players::player_handle::create_info_update(
                            [mc_protocol::packets::play::ClientboundPlayerInfoUpdateAction::UpdateGameMode].into_iter().collect(),
                            [&handle],
                        );
                        server.player_list.broadcast_all(&packet);
                    }
                }
            },
        );
    }

    /// `sendGameRuleValues`.
    fn send_game_rule_values(&mut self) {
        if !self.has_gamemaster_permission() {
            log::warn!("Player {} tried to request game rule values without required permissions", self.player.get_profile().name);
            return;
        }
        let rules = self.server.game_rules();
        let values: Vec<(Identifier, String)> =
            crate::world::level::gamerules::AnyGameRule::all().map(|rule| (rule.get_identifier(), rules.get_as_string(rule))).collect();
        self.send(&mc_protocol::packets::play::ClientboundGameRuleValuesPacket { values });
    }

    /// `handleConfigurationAcknowledged`.
    pub fn handle_configuration_acknowledged(&mut self, _packet: &ServerboundConfigurationAcknowledgedPacket) {
        if !self.lis().waiting_for_switch_to_config {
            panic!("Client acknowledged config, but none was requested");
        }
        let cookie = crate::server::network::common_listener::CommonListenerCookie {
            profile: self.player.get_profile().clone(),
            latency: self.lis().latency(),
            client_information: self.player.client_information(),
            transferred: self.lis().common.transferred(),
        };
        // hands the connection to a new configuration listener (GameNet's reader pump)
        self.lis().sender.switch_to_configuration(cookie);
    }

    /// `switchToConfig()`.
    pub fn switch_to_config(&mut self) {
        self.lis().waiting_for_switch_to_config = true;
        self.remove_player_from_world();
        self.send(&ClientboundStartConfigurationPacket);
        self.lis().sender.set_outbound_state(mc_protocol::packet::ConnectionState::Configuration);
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

    /// `removePlayerFromWorld`: leave message, `player.disconnect()` and `PlayerList.remove(player)` (the level removal
    /// is applied when the taken-out player is put back, see `ServerLevel::put_back_entity`).
    fn remove_player_from_world(&mut self) {
        self.server.invalidate_status();
        let message = Component::translatable("multiplayer.player.left", vec![self.player.get_display_name().into()]).with_format(ChatFormatting::Yellow);
        self.server.player_list.broadcast_system_message(&message, false);
        self.player.disconnected = true;
        self.remove_from_player_list();
    }

    /// `PlayerList.remove(player)`.
    fn remove_from_player_list(&mut self) {
        crate::server::players::player_ops::remove(&mut *self.server, &mut *self.level, &mut *self.player);
    }
}
