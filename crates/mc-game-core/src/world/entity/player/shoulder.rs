//! `ServerPlayer` shoulder entities (`ShoulderEntityLeft/Right`): `handleShoulderEntities`, `setEntityOnShoulder`,
//! `removeEntitiesOnShoulder`.
//!

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
        self.play_shoulder_entity_ambient_sound(level, false);
        self.play_shoulder_entity_ambient_sound(level, true);
        let b = self.base();
        // `isInPowderSnow` is tracked by the entity base (`EntityData.is_in_powder_snow`)
        if b.fall_distance > 0.5
            || self.is_in_water()
            || self.player.abilities.flying
            || self.player.living.is_sleeping()
            || self.base().is_in_powder_snow
        {
            self.remove_entities_on_shoulder(level);
        }
    }

    fn play_shoulder_entity_ambient_sound(&mut self, level: &mut ServerLevel, right: bool) {
        let tag = if right {
            &self.shoulder_entity_right
        } else {
            &self.shoulder_entity_left
        };
        if !tag.is_empty()
            && !tag.get_boolean_or("Silent", false)
            && self.base_mut().random.next_int_bounded(200) == 0
        {
            let is_parrot = (if right {
                &self.shoulder_entity_right
            } else {
                &self.shoulder_entity_left
            })
            .get_string("id")
                == Some("minecraft:parrot");
            if is_parrot
                && !crate::world::entity::animal::parrot::parrot::imitate_nearby_mobs(level, self)
            {
                let sound = crate::world::entity::animal::parrot::parrot::get_ambient(
                    level.difficulty(),
                    &mut self.base_mut().random,
                );
                let pitch = crate::world::entity::animal::parrot::parrot::get_pitch(
                    &mut self.base_mut().random,
                );
                let pos = self.position();
                if let Some(sound) = sound {
                    level.play_sound_at(
                        None,
                        pos.x,
                        pos.y,
                        pos.z,
                        sound,
                        self.get_sound_source(),
                        1.0,
                        pitch,
                    );
                }
            }
        }
    }

    /// `setEntityOnShoulder(entityTag)`: whether the entity found a free shoulder.
    pub fn set_entity_on_shoulder(&mut self, level: &ServerLevel, entity_tag: CompoundTag) -> bool {
        if self.base().is_passenger()
            || !self.base().on_ground()
            || self.is_in_water()
            || self.base().is_in_powder_snow
        {
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
        let request = EntitySpawnRequest {
            reason: EntitySpawnReason::Load,
            ignore_checks: false,
        };
        if let Some(mut entity) = create_and_load(tag, level, request) {
            let b = self.base();
            entity.set_pos(b.x(), b.y() + f64::from(0.7f32), b.z());
            level.add_fresh_entity(entity);
        }
    }
}
