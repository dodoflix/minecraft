//! Generic entity for types without an implementation: all base `Entity` state is modelled, everything else the
//! entity was saved with is kept as raw NBT and written back unchanged (lossless world round trip).

use super::entity::{self, Entity, EntityData};
use super::EntityId;
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
            entity::TAG_ID, entity::TAG_UUID, entity::TAG_PASSENGERS, entity::TAG_DATA, entity::TAG_POS, entity::TAG_MOTION, entity::TAG_ROTATION,
            "fall_distance", "Fire", "Air", "OnGround", "Invulnerable", "PortalCooldown", "invulnerable_time", "CustomName", "CustomNameVisible",
            "Silent", "NoGravity", "Glowing", "TicksFrozen", "HasVisualFire", "Tags", "Team",
        ]
    })
}

impl PlainEntity {
    pub fn new(ty: EntityType, id: EntityId) -> Self {
        Self { base: EntityData::new(ty, id, entity::TOTAL_AIR_SUPPLY, |_| {}), extra: CompoundTag::new() }
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
