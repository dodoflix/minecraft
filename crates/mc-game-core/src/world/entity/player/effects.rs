//! `ServerPlayer.onEffectAdded/Updated/Removed`: the effect packets, the levitation start point and the
//! `EFFECTS_CHANGED` advancement trigger.

use super::ServerPlayer;
use crate::advancements::triggers::CriteriaTriggers;
use crate::server::level::ServerLevel;
use crate::server::players::join::mob_effect_packet;
use crate::world::entity::effect::MobEffectInstance;
use crate::world::entity::living_entity::effects as living_effects;
use crate::world::entity::{Entity, EntityId};
use mc_data::MobEffect;
use mc_protocol::packets::play::ClientboundRemoveMobEffectPacket;

/// `CriteriaTriggers.EFFECTS_CHANGED.trigger(player, source)`: the source is looked up in the level (or is the player itself).
fn trigger_effects_changed(player: &mut ServerPlayer, level: &mut ServerLevel, source: Option<EntityId>) {
    let source: Option<&dyn Entity> = match source {
        Some(id) if id == player.base_id() => Some(&*player),
        Some(id) => level.get_entity(id),
        None => None,
    };
    let matches = CriteriaTriggers::EFFECTS_CHANGED.matches(&*level, &*player, source);
    matches.award(level, player);
}


impl ServerPlayer {
    /// `ServerPlayer.onEffectAdded(effect, source)`.
    pub(super) fn server_player_on_effect_added(&mut self, level: &mut ServerLevel, effect: &MobEffectInstance, source: Option<EntityId>) {
        living_effects::on_effect_added(self, level, effect, source);
        self.send(&mob_effect_packet(self.base_id(), effect, true));
        if effect.effect == MobEffect::LEVITATION {
            self.levitation_start_time = self.base().tick_count;
            self.levitation_start_pos = Some(self.base().position());
        }
        trigger_effects_changed(self, level, source);
    }

    /// `ServerPlayer.onEffectUpdated(effect, doRefreshAttributes, source)`.
    pub(super) fn server_player_on_effect_updated(&mut self, level: &mut ServerLevel, effect: &MobEffectInstance, do_refresh_attributes: bool, source: Option<EntityId>) {
        living_effects::on_effect_updated(self, level, effect, do_refresh_attributes, source);
        self.send(&mob_effect_packet(self.base_id(), effect, false));
        trigger_effects_changed(self, level, source);
    }

    /// `ServerPlayer.onEffectsRemoved(effects)`.
    pub(super) fn server_player_on_effects_removed(&mut self, level: &mut ServerLevel, effects: &[MobEffectInstance]) {
        living_effects::on_effects_removed(self, level, effects);
        for effect in effects {
            self.send(&ClientboundRemoveMobEffectPacket { entity_id: self.base_id(), effect: effect.effect });
            if effect.effect == MobEffect::LEVITATION {
                self.levitation_start_pos = None;
            }
        }
        trigger_effects_changed(self, level, None);
    }
}
