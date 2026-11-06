//! Generic entity for types without an implementation: all base `Entity` state is modelled, everything else the
//! entity was saved with is kept as raw NBT and written back unchanged (lossless world round trip).

use super::EntityId;
use super::entity::{self, Entity, EntityData};
use mc_data::EntityType;
use mc_nbt::CompoundTag;
use std::any::Any;
use std::sync::OnceLock;

pub struct PlainEntity {
    base: EntityData,
    /// Saved keys that `Entity.load` does not consume (type-specific data), restored verbatim by `save`.
    extra: CompoundTag,
}

/// Keys written/read by the base `Entity` save/load code.
fn base_keys() -> &'static [&'static str] {
    static KEYS: OnceLock<Vec<&'static str>> = OnceLock::new();
    KEYS.get_or_init(|| {
        vec![
            entity::TAG_ID,
            entity::TAG_UUID,
            entity::TAG_PASSENGERS,
            entity::TAG_DATA,
            entity::TAG_POS,
            entity::TAG_MOTION,
            entity::TAG_ROTATION,
            "fall_distance",
            "Fire",
            "Air",
            "OnGround",
            "Invulnerable",
            "PortalCooldown",
            "invulnerable_time",
            "CustomName",
            "CustomNameVisible",
            "Silent",
            "NoGravity",
            "Glowing",
            "TicksFrozen",
            "HasVisualFire",
            "Tags",
            "Team",
        ]
    })
}

impl PlainEntity {
    pub fn new(ty: EntityType, id: EntityId) -> Self {
        Self {
            base: EntityData::new(ty, id, entity::TOTAL_AIR_SUPPLY, |_| {}),
            extra: CompoundTag::new(),
        }
    }
}

impl Entity for PlainEntity {
    fn base(&self) -> &EntityData {
        &self.base
    }
    fn base_mut(&mut self) -> &mut EntityData {
        &mut self.base
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn as_dyn(&self) -> &dyn Entity {
        self
    }
    fn as_dyn_mut(&mut self) -> &mut dyn Entity {
        self
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        self.extra = CompoundTag::new();
        for (k, v) in input.iter() {
            if !base_keys().contains(&k.as_str()) {
                self.extra.put(k.clone(), v.clone());
            }
        }
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        for (k, v) in self.extra.iter() {
            out.put(k.clone(), v.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::level::ServerLevel;
    use crate::world::entity::factory;
    use mc_nbt::{ListTag, Tag, compound};

    fn pig_tag() -> CompoundTag {
        let mut tag = CompoundTag::new();
        tag.put_string("id", "minecraft:pig");
        let mut pos = ListTag::new();
        for v in [10.5, 64.0, -3.25] {
            pos.push(Tag::double(v));
        }
        tag.put("Pos", pos);
        let mut motion = ListTag::new();
        for v in [0.0, -0.0784, 0.0] {
            motion.push(Tag::double(v));
        }
        tag.put("Motion", motion);
        let mut rot = ListTag::new();
        rot.push(Tag::float(90.0));
        rot.push(Tag::float(10.0));
        tag.put("Rotation", rot);
        tag.put_double("fall_distance", 0.0);
        tag.put_short("Fire", -1);
        tag.put_short("Air", 300);
        tag.put_boolean("OnGround", true);
        tag.put_boolean("Invulnerable", false);
        tag.put_int("PortalCooldown", 0);
        tag.put_int_array("UUID", vec![1, 2, 3, 4]);
        tag.put_float("Health", 10.0);
        tag.put(
            "Tags",
            ListTag::from_iter([Tag::String("a".into()), Tag::String("b".into())]),
        );
        tag.put("attributes", Tag::Compound(compound! {}));
        tag
    }

    #[test]
    fn plain_entity_round_trips_nbt() {
        let level = ServerLevel::detached(
            mc_core::ident::Identifier::parse("minecraft:overworld").unwrap(),
        );
        let tag = pig_tag();
        let loaded = factory::load_entities_recursive(
            std::slice::from_ref(&tag),
            &level,
            mc_data::enums::EntitySpawnReason::Load,
        );
        assert_eq!(loaded.len(), 1);
        let none = |_| None;
        let saved = crate::world::entity::entity::save(loaded[0].as_ref(), &none).unwrap();
        assert_eq!(saved, tag);
    }

    #[test]
    fn unknown_id_and_player_are_skipped() {
        let level = ServerLevel::detached(
            mc_core::ident::Identifier::parse("minecraft:overworld").unwrap(),
        );
        let mut bad = pig_tag();
        bad.put_string("id", "minecraft:nope");
        let mut player = pig_tag();
        player.put_string("id", "minecraft:player");
        let loaded = factory::load_entities_recursive(
            &[bad, player],
            &level,
            mc_data::enums::EntitySpawnReason::Load,
        );
        assert!(loaded.is_empty());
    }
}

#[cfg(test)]
mod teleport_tests {
    use super::*;
    use mc_core::phys::Vec3;
    use mc_data::EntityType;
    use mc_protocol::packets::play::{PositionMoveRotation, Relative, Relatives};

    #[test]
    fn teleport_set_position_applies_relatives_and_resets_old_pos() {
        let mut e = PlainEntity::new(EntityType::PIG, 1);
        e.set_pos(10.0, 64.0, 10.0);
        e.base_mut().set_y_rot(90.0);
        let dest = PositionMoveRotation {
            position: Vec3::new(1.0, 2.0, 3.0),
            delta_movement: Vec3::ZERO,
            y_rot: 45.0,
            x_rot: 100.0,
        };
        e.teleport_set_position(dest, Relatives::NONE.with(Relative::X).with(Relative::YRot));
        assert_eq!(e.base().position(), Vec3::new(11.0, 2.0, 3.0));
        assert_eq!(e.base().y_rot(), 135.0);
        assert_eq!(e.base().x_rot(), 90.0, "pitch is clamped");
        assert_eq!(e.base().old_position(), Vec3::new(11.0, 2.0, 3.0));
    }
}
