//! `net.minecraft.world.entity.boss.enderdragon.EnderDragonPart`: a hitbox of the ender dragon. A part is an entity of the
//! dragon's type with its own id (taken right after the dragon's) that is never tracked, saved or ticked: the level keeps
//! it in its part map (`ServerLevel.dragonParts`) while the dragon is tracked, the dragon moves it, and an attack on it is
//! the dragon's hurt for that part.

use super::EnderDragon;
use crate::server::level::ServerLevel;
use crate::world::damagesource::DamageSource;
use crate::world::entity::EntityId;
use crate::world::entity::dimensions::EntityDimensions;
use crate::world::entity::entity::{self, Entity, EntityData};
use mc_data::EntityType;
use mc_data::enums::Pose;
use mc_nbt::CompoundTag;
use std::any::Any;

/// The parts of an ender dragon in vanilla's `subEntities` order: name, width, height.
pub const DRAGON_PARTS: [(&str, f32, f32); 8] = [
    ("head", 1.0, 1.0),
    ("neck", 3.0, 3.0),
    ("body", 5.0, 3.0),
    ("tail", 2.0, 2.0),
    ("tail", 2.0, 2.0),
    ("tail", 2.0, 2.0),
    ("wing", 4.0, 2.0),
    ("wing", 4.0, 2.0),
];

/// Indexes into [`DRAGON_PARTS`].
pub const HEAD: usize = 0;
pub const NECK: usize = 1;
pub const BODY: usize = 2;
pub const TAIL_1: usize = 3;
pub const WING_1: usize = 6;
pub const WING_2: usize = 7;

pub struct EnderDragonPart {
    base: EntityData,
    /// `parentMob`.
    pub parent: EntityId,
    /// The part's index in [`DRAGON_PARTS`].
    pub index: usize,
    /// `name`.
    pub name: &'static str,
    /// `size`.
    size: EntityDimensions,
}

impl EnderDragonPart {
    /// `new EnderDragonPart(parentMob, name, w, h)`: an entity of the parent's type (`id` is the part's own), sized by
    /// `refreshDimensions()`.
    pub fn new(parent_type: EntityType, id: EntityId, parent: EntityId, index: usize) -> Self {
        let (name, w, h) = DRAGON_PARTS[index];
        let mut part = Self {
            base: EntityData::new(parent_type, id, entity::TOTAL_AIR_SUPPLY, |_| {}),
            parent,
            index,
            name,
            size: EntityDimensions::scalable(w, h),
        };
        part.refresh_dimensions();
        part
    }
}

impl Entity for EnderDragonPart {
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

    fn is_ender_dragon_part(&self) -> bool {
        true
    }

    fn is_pickable(&self) -> bool {
        true
    }

    fn get_dimensions(&self, _pose: Pose) -> EntityDimensions {
        self.size.clone()
    }

    /// `hurtServer(level, source, damage)`: the parent's `hurt(level, part, source, damage)` unless the part is invulnerable
    /// to the source.
    fn hurt_server(&mut self, level: &mut ServerLevel, source: &DamageSource, amount: f32) -> bool {
        if self.is_invulnerable_to_base(level, source) {
            return false;
        }
        let index = self.index;
        level
            .with_entity(self.parent, |level, parent| {
                parent
                    .as_any_mut()
                    .downcast_mut::<EnderDragon>()
                    .is_some_and(|dragon| dragon.hurt_part(level, index, source, amount))
            })
            .unwrap_or(false)
    }

    fn get_add_entity_packet(
        &self,
        _server_entity: &crate::server::level::ServerEntity,
    ) -> mc_protocol::packets::play::ClientboundAddEntityPacket {
        unreachable!("an ender dragon part is never tracked")
    }

    fn should_be_saved_ignoring_riding(&self) -> bool {
        false
    }

    fn read_additional_save_data(&mut self, _level: Option<&ServerLevel>, _input: &CompoundTag) {}

    fn add_additional_save_data(&self, _out: &mut CompoundTag) {}
}
