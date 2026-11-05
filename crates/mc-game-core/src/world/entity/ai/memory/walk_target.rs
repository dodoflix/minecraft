//! `WalkTarget`: where `MoveToTargetSink` should walk, how fast, and how close is close enough.

use crate::world::entity::EntityId;
use crate::world::entity::ai::behavior::position_tracker::{BlockPosTracker, EntityTracker, PositionTracker};
use crate::world::entity::entity::Entity;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;

#[derive(Clone, PartialEq, Debug)]
pub struct WalkTarget {
    target: PositionTracker,
    speed_modifier: f32,
    close_enough_dist: i32,
}

impl WalkTarget {
    /// `new WalkTarget(BlockPos, speed, closeEnough)`.
    pub fn from_block_pos(target: BlockPos, speed_modifier: f32, close_enough_dist: i32) -> Self {
        Self::new(PositionTracker::Block(BlockPosTracker::new(target)), speed_modifier, close_enough_dist)
    }

    /// `new WalkTarget(Vec3, speed, closeEnough)`: tracks `BlockPos.containing(target)` (its center).
    pub fn from_vec3(target: Vec3, speed_modifier: f32, close_enough_dist: i32) -> Self {
        Self::new(PositionTracker::Block(BlockPosTracker::new(BlockPos::containing(target.x, target.y, target.z))), speed_modifier, close_enough_dist)
    }

    /// `new WalkTarget(Entity, speed, closeEnough)`: `new EntityTracker(target, false)`.
    pub fn from_entity(target: &dyn Entity, speed_modifier: f32, close_enough_dist: i32) -> Self {
        Self::new(PositionTracker::Entity(EntityTracker::new(target, false)), speed_modifier, close_enough_dist)
    }

    /// `new WalkTarget(PositionTracker, speed, closeEnough)`.
    pub fn new(target: PositionTracker, speed_modifier: f32, close_enough_dist: i32) -> Self {
        Self { target, speed_modifier, close_enough_dist }
    }

    pub fn get_target(&self) -> &PositionTracker {
        &self.target
    }

    pub fn get_speed_modifier(&self) -> f32 {
        self.speed_modifier
    }

    pub fn get_close_enough_dist(&self) -> i32 {
        self.close_enough_dist
    }

    /// The tracked entity if the target is an [`EntityTracker`].
    pub fn tracked_entity(&self) -> Option<EntityId> {
        match &self.target {
            PositionTracker::Entity(t) => Some(t.get_entity()),
            PositionTracker::Block(_) => None,
        }
    }
}
