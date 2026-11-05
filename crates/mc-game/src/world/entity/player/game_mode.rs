//! `ServerPlayer.setGameMode` + `ServerPlayerGameMode.changeGameModeForPlayer` and `ServerPlayer.onUpdateAbilities`.

use super::ServerPlayer;
use crate::server::level::ServerLevel;
use crate::world::entity::{Entity, movement};
use mc_data::enums::GameType;
use mc_protocol::packets::play::{
    ClientboundGameEventPacket, ClientboundPlayerAbilitiesPacket, ClientboundPlayerInfoUpdateAction, ClientboundPlayerInfoUpdatePacket,
};

/// `GameType.getId()`.
pub fn game_type_id(game_type: GameType) -> i32 {
    match game_type {
        GameType::Survival => 0,
        GameType::Creative => 1,
        GameType::Adventure => 2,
        GameType::Spectator => 3,
    }
}

impl ServerPlayer {
    /// `ServerPlayer.onUpdateAbilities()`: the abilities packet, then `updateInvisibilityStatus()` (spectators are invisible).
    pub fn on_update_abilities(&mut self) {
        if self.connection.is_none() {
            return;
        }
        let a = &self.player.abilities;
        let packet = ClientboundPlayerAbilitiesPacket {
            invulnerable: a.invulnerable,
            is_flying: a.flying,
            can_fly: a.mayfly,
            instabuild: a.instabuild,
            flying_speed: a.get_flying_speed(),
            walking_speed: a.get_walking_speed(),
        };
        self.send(&packet);
        // EcEffects follow-up: `updateInvisibilityStatus()` (spectators are invisible)
    }

    /// `ServerPlayerGameMode.isInRangeOfGround()`.
    fn is_in_range_of_ground(&self, level: &ServerLevel) -> bool {
        movement::collect_all_colliders(self, level, &self.base().bounding_box()).is_empty() && movement::get_available_space_below(self, level, 1.0) < 1.0
    }

    /// `ServerPlayerGameMode.changeGameModeForPlayer(mode)`: `false` when the mode does not change.
    pub fn change_game_mode_for_player(&mut self, level: &mut ServerLevel, mode: GameType) -> bool {
        let old = self.game_mode.game_mode_for_player;
        if mode == old {
            return false;
        }
        self.set_game_mode_for_player(mode, Some(old));
        if self.player.abilities.flying && mode != GameType::Spectator && self.is_in_range_of_ground(level) {
            self.player.abilities.flying = false;
        }
        self.on_update_abilities();
        self.sync_handle();
        if let Some(handle) = &self.handle {
            level.broadcast_to_all_players(&ClientboundPlayerInfoUpdatePacket {
                actions: [ClientboundPlayerInfoUpdateAction::UpdateGameMode].into_iter().collect(),
                entries: vec![handle.info_entry()],
            });
        }
        level.update_sleeping_player_list();
        if mode == GameType::Creative {
            self.reset_current_impulse_context();
        }
        true
    }

    /// `LivingEntity.resetCurrentImpulseContext()`.
    pub fn reset_current_impulse_context(&mut self) {
        self.player.living.current_impulse_context_reset_grace_time = 0;
        self.player.living.current_impulse_impact_pos = None;
    }

    /// `ServerPlayer.setGameMode(mode)`: `false` when the player already is in `mode`.
    pub fn set_game_mode(&mut self, level: &mut ServerLevel, mode: GameType) -> bool {
        let was_spectator = self.is_spectator();
        if !self.change_game_mode_for_player(level, mode) {
            return false;
        }
        self.send(&ClientboundGameEventPacket { event: ClientboundGameEventPacket::CHANGE_GAME_MODE, param: game_type_id(mode) as f32 });
        if mode == GameType::Spectator {
            self.remove_entities_on_shoulder(level);
            level.stop_riding(self);
            self.stop_using_item(level);
            // SEAM(enchant): `EnchantmentHelper.stopLocationBasedEffects(this)`
        } else {
            // SEAM(level): `setCamera(this)` (spectator camera)
            if was_spectator {
                // SEAM(enchant): `EnchantmentHelper.runLocationChangedEffects(level, this)`
            }
        }
        self.on_update_abilities();
        self.player.living.effects_dirty = true; // `updateEffectVisibility()`
        true
    }
}
