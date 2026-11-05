//! `ServerPlayer` shoulder entities (`ShoulderEntityLeft/Right`): `handleShoulderEntities`, `setEntityOnShoulder`,
//! `removeEntitiesOnShoulder`.
//!
//! Gap (parrot class owner): `playShoulderEntityAmbientSound` draws the vanilla `random.nextInt(200)` but a parrot's
//! `Parrot.imitateNearbyMobs/getAmbient/getPitch` sound is not played.

use super::ServerPlayer;
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::factory::{EntitySpawnRequest, create_and_load};
use mc_core::random::RandomSource;
use mc_data::enums::EntitySpawnReason;
use mc_nbt::CompoundTag;

impl ServerPlayer {
    /// `handleShoulderEntities()` (the `aiStep` tail).
    pub fn handle_shoulder_entities(&mut self, level: &mut ServerLevel) {
        self.play_shoulder_entity_ambient_sound(false);
        self.play_shoulder_entity_ambient_sound(true);
        let b = self.base();
        // `isInPowderSnow` is tracked by the entity base (`EntityData.is_in_powder_snow`)
        if b.fall_distance > 0.5 || self.is_in_water() || self.player.abilities.flying || self.player.living.is_sleeping() || self.base().is_in_powder_snow {
            self.remove_entities_on_shoulder(level);
        }
    }

    fn play_shoulder_entity_ambient_sound(&mut self, right: bool) {
        let tag = if right { &self.shoulder_entity_right } else { &self.shoulder_entity_left };
        if !tag.is_empty() && !tag.get_boolean_or("Silent", false) {
            let _ = self.base_mut().random.next_int_bounded(200);
        }
    }

    /// `setEntityOnShoulder(entityTag)`: whether the entity found a free shoulder.
    pub fn set_entity_on_shoulder(&mut self, level: &ServerLevel, entity_tag: CompoundTag) -> bool {
        if self.base().is_passenger() || !self.base().on_ground() || self.is_in_water() || self.base().is_in_powder_snow {
            return false;
        }
        if self.shoulder_entity_left.is_empty() {
            self.shoulder_entity_left = entity_tag;
        } else if self.shoulder_entity_right.is_empty() {
            self.shoulder_entity_right = entity_tag;
        } else {
            return false;
        }
        self.time_entity_sat_on_shoulder = level.game_time();
        true
    }

    /// `removeEntitiesOnShoulder()`: respawns the shoulder entities next to the player (not within 20 ticks of sitting down).
    pub fn remove_entities_on_shoulder(&mut self, level: &mut ServerLevel) {
        if self.time_entity_sat_on_shoulder + 20 < level.game_time() {
            let left = std::mem::take(&mut self.shoulder_entity_left);
            self.respawn_entity_on_shoulder(level, &left);
            let right = std::mem::take(&mut self.shoulder_entity_right);
            self.respawn_entity_on_shoulder(level, &right);
        }
    }

    fn respawn_entity_on_shoulder(&self, level: &mut ServerLevel, tag: &CompoundTag) {
        if tag.is_empty() {
            return;
        }
        let request = EntitySpawnRequest { reason: EntitySpawnReason::Load, ignore_checks: false };
        if let Some(mut entity) = create_and_load(tag, level, request) {
            let b = self.base();
            entity.set_pos(b.x(), b.y() + f64::from(0.7f32), b.z());
            level.add_fresh_entity(entity);
        }
    }
}
