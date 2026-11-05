//! `PositionTracker` and its two vanilla implementations, `BlockPosTracker` and `EntityTracker` (the values of
//! `LOOK_TARGET` and the target of a `WalkTarget`).

use crate::server::level::ServerLevel;
use crate::world::entity::EntityId;
use crate::world::entity::ai::memory::memory_module_type::NEAREST_VISIBLE_LIVING_ENTITIES;
use crate::world::entity::entity::Entity;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use std::cell::Cell;

/// `BlockPosTracker`.
#[derive(Clone, PartialEq, Debug)]
pub struct BlockPosTracker {
    block_pos: BlockPos,
    center_position: Vec3,
}

impl BlockPosTracker {
    /// `new BlockPosTracker(BlockPos)`.
    pub fn new(block_pos: BlockPos) -> Self {
        Self { block_pos, center_position: Vec3::at_center_of(&block_pos) }
    }

    /// `new BlockPosTracker(Vec3)`.
    pub fn from_vec3(vec: Vec3) -> Self {
        Self { block_pos: BlockPos::containing(vec.x, vec.y, vec.z), center_position: vec }
    }
}

/// `EntityTracker`. Vanilla keeps the entity object; this holds its id and the last position it could resolve
/// (an entity that was removed or is currently taken out of its slot keeps reporting where it was).
#[derive(Clone, Debug)]
pub struct EntityTracker {
    entity: EntityId,
    track_eye_height: bool,
    target_eye_height: bool,
    /// `entity instanceof LivingEntity`.
    living: bool,
    last: Cell<(Vec3, BlockPos)>,
}

impl PartialEq for EntityTracker {
    fn eq(&self, other: &Self) -> bool {
        self.entity == other.entity && self.track_eye_height == other.track_eye_height && self.target_eye_height == other.target_eye_height
    }
}

impl EntityTracker {
    /// `new EntityTracker(entity, trackEyeHeight)`.
    pub fn new(entity: &dyn Entity, track_eye_height: bool) -> Self {
        Self::new_full(entity, track_eye_height, false)
    }

    /// `new EntityTracker(entity, trackEyeHeight, targetEyeHeight)`.
    pub fn new_full(entity: &dyn Entity, track_eye_height: bool, target_eye_height: bool) -> Self {
        let tracker = Self { entity: entity.id(), track_eye_height, target_eye_height, living: entity.living().is_some(), last: Cell::new((Vec3::ZERO, BlockPos::ZERO)) };
        tracker.remember(entity);
        tracker
    }

    fn compute(&self, entity: &dyn Entity) -> (Vec3, BlockPos) {
        let pos = if self.track_eye_height {
            let p = entity.position();
            Vec3::new(p.x, p.y + f64::from(entity.base().eye_height), p.z)
        } else {
            entity.position()
        };
        let block = if self.target_eye_height {
            let eye = entity.base().get_eye_position();
            BlockPos::containing(eye.x, eye.y, eye.z)
        } else {
            entity.block_position()
        };
        (pos, block)
    }

    fn remember(&self, entity: &dyn Entity) -> (Vec3, BlockPos) {
        let value = self.compute(entity);
        self.last.set(value);
        value
    }

    /// `getEntity()` (as an id).
    pub fn get_entity(&self) -> EntityId {
        self.entity
    }
}

/// `PositionTracker`.
#[derive(Clone, PartialEq, Debug)]
pub enum PositionTracker {
    Block(BlockPosTracker),
    Entity(EntityTracker),
}

impl PositionTracker {
    /// `currentPosition()`.
    pub fn current_position(&self, level: &ServerLevel) -> Vec3 {
        match self {
            PositionTracker::Block(t) => t.center_position,
            PositionTracker::Entity(t) => match level.get_entity(t.entity) {
                Some(e) => t.remember(e).0,
                None => t.last.get().0,
            },
        }
    }

    /// `currentBlockPosition()`.
    pub fn current_block_position(&self, level: &ServerLevel) -> BlockPos {
        match self {
            PositionTracker::Block(t) => t.block_pos,
            PositionTracker::Entity(t) => match level.get_entity(t.entity) {
                Some(e) => t.remember(e).1,
                None => t.last.get().1,
            },
        }
    }

    /// `isVisibleBy(body)`.
    pub fn is_visible_by(&self, level: &ServerLevel, body: &dyn Entity) -> bool {
        match self {
            PositionTracker::Block(_) => true,
            PositionTracker::Entity(t) => match level.get_entity(t.entity) {
                Some(e) if e.living().is_some() => {
                    if !e.is_alive() {
                        return false;
                    }
                    match body.brain().get_memory(&NEAREST_VISIBLE_LIVING_ENTITIES) {
                        Some(visible) => visible.contains(level, body, t.entity),
                        None => false,
                    }
                }
                Some(_) => true,
                // gone from the level: a removed living entity is not alive
                None => !t.living,
            },
        }
    }
}
