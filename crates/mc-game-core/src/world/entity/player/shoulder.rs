//! `ServerPlayer` shoulder entities (`ShoulderEntityLeft/Right`): `handleShoulderEntities`, `setEntityOnShoulder`,
//! `removeEntitiesOnShoulder`.
//!

use super::{DATA_SHOULDER_PARROT_LEFT, DATA_SHOULDER_PARROT_RIGHT, ServerPlayer};
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::animal::parrot::parrot::ParrotVariant;
use crate::world::entity::factory::{EntitySpawnRequest, create_and_load};
use crate::world::entity::mob::{MobKind, tamable};
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
            self.set_shoulder_entity_left(entity_tag);
        } else if self.shoulder_entity_right.is_empty() {
            self.set_shoulder_entity_right(entity_tag);
        } else {
            return false;
        }
        self.time_entity_sat_on_shoulder = level.game_time();
        true
    }

    /// `removeEntitiesOnShoulder()`: respawns the shoulder entities next to the player (not within 20 ticks of sitting down).
    pub fn remove_entities_on_shoulder(&mut self, level: &mut ServerLevel) {
        if self.time_entity_sat_on_shoulder + 20 < level.game_time() {
            let left = self.shoulder_entity_left.clone();
            self.respawn_entity_on_shoulder(level, &left);
            self.set_shoulder_entity_left(CompoundTag::new());
            let right = self.shoulder_entity_right.clone();
            self.respawn_entity_on_shoulder(level, &right);
            self.set_shoulder_entity_right(CompoundTag::new());
        }
    }

    /// `setShoulderEntityLeft(tag)`: also publishes the parrot variant (`setShoulderParrotLeft`).
    pub fn set_shoulder_entity_left(&mut self, tag: CompoundTag) {
        let variant = extract_parrot_variant(&tag);
        self.shoulder_entity_left = tag;
        self.base_mut()
            .entity_data
            .set(DATA_SHOULDER_PARROT_LEFT, variant);
    }

    /// `setShoulderEntityRight(tag)`: also publishes the parrot variant (`setShoulderParrotRight`).
    pub fn set_shoulder_entity_right(&mut self, tag: CompoundTag) {
        let variant = extract_parrot_variant(&tag);
        self.shoulder_entity_right = tag;
        self.base_mut()
            .entity_data
            .set(DATA_SHOULDER_PARROT_RIGHT, variant);
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
            if entity
                .mob()
                .is_some_and(|m| m.kind == MobKind::TamableAnimal)
            {
                tamable::set_owner_reference(entity.as_mut(), Some(self.uuid()));
            }
            let b = self.base();
            entity.set_pos(b.x(), b.y() + f64::from(0.7f32), b.z());
            level.add_fresh_entity(entity);
        }
    }
}

/// `Player.extractParrotVariant(tag)`: the `Variant` of a parrot tag (clamped like `Parrot.Variant.byId`).
fn extract_parrot_variant(tag: &CompoundTag) -> Option<i32> {
    if tag.get_string("id") == Some("minecraft:parrot") {
        return tag
            .get_int("Variant")
            .map(|v| ParrotVariant::by_id(v) as i32);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::entity::Entity;
    use mc_core::ident::Identifier;
    use mc_protocol::codec::GameProfile;

    fn parrot_tag() -> CompoundTag {
        let mut tag = CompoundTag::new();
        tag.put_string("id", "minecraft:parrot");
        tag
    }

    /// `Player.aiStep` ends with `handleShoulderEntities()`: a fall beyond half a block drops the shoulder entities.
    #[test]
    fn ai_step_drops_the_shoulder_entities_of_a_falling_player() {
        let mut level = ServerLevel::detached(Identifier::parse("minecraft:overworld").unwrap());
        level.set_game_time(100);
        let mut p = ServerPlayer::new(
            1,
            Identifier::parse("minecraft:overworld").unwrap(),
            GameProfile {
                id: uuid::Uuid::from_u128(1),
                name: "A".into(),
                properties: vec![],
            },
            Default::default(),
        );
        p.set_pos(0.0, 100.0, 0.0);
        p.shoulder_entity_left = parrot_tag();
        p.do_tick(&mut level);
        assert!(!p.shoulder_entity_left.is_empty(), "standing: stays");
        p.base_mut().fall_distance = 1.0;
        p.do_tick(&mut level);
        assert!(p.shoulder_entity_left.is_empty());
    }

    /// `setShoulderEntityLeft/Right` publish the parrot variant (clamped) in the synched data, `None` for anything else.
    #[test]
    fn shoulder_parrot_variant_is_synched() {
        let mut p = ServerPlayer::new(
            1,
            Identifier::parse("minecraft:overworld").unwrap(),
            GameProfile {
                id: uuid::Uuid::from_u128(1),
                name: "A".into(),
                properties: vec![],
            },
            Default::default(),
        );
        let mut tag = parrot_tag();
        tag.put_byte("Variant", 9);
        p.set_shoulder_entity_left(tag);
        p.set_shoulder_entity_right(parrot_tag());
        assert_eq!(p.base().entity_data.get(DATA_SHOULDER_PARROT_LEFT), Some(4));
        assert_eq!(p.base().entity_data.get(DATA_SHOULDER_PARROT_RIGHT), None);
        p.set_shoulder_entity_left(CompoundTag::new());
        assert_eq!(p.base().entity_data.get(DATA_SHOULDER_PARROT_LEFT), None);
    }
}
