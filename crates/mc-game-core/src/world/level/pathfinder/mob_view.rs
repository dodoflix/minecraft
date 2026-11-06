//! The narrow view of a `Mob` that the node evaluators and the path finder need (vanilla passes the `Mob` itself).
//!
//! [`EntityMob`] adapts a live `&mut dyn Entity` (+ the level it is in); tests (and the oracle goldens) implement the
//! trait for a plain profile struct.

use super::path_type::PathType;
use crate::server::level::ServerLevel;
use crate::world::entity::{Entity, EntityId};
use crate::world::level::collision::CollisionContext;
use mc_core::phys::{AABB, Vec3};
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::fluid::FluidState;

pub trait PathfindingMob {
    fn id(&self) -> EntityId;
    fn position(&self) -> Vec3;
    fn x(&self) -> f64 {
        self.position().x
    }
    fn y(&self) -> f64 {
        self.position().y
    }
    fn z(&self) -> f64 {
        self.position().z
    }
    /// `blockPosition()`.
    fn block_position(&self) -> BlockPos;
    /// `getBlockY()`.
    fn get_block_y(&self) -> i32 {
        self.block_position().y()
    }
    /// `getBoundingBox()`.
    fn bounding_box(&self) -> AABB;
    /// `getBbWidth()`.
    fn bb_width(&self) -> f32;
    /// `getBbHeight()`.
    fn bb_height(&self) -> f32;
    fn on_ground(&self) -> bool;
    fn is_in_water(&self) -> bool;
    fn is_in_liquid(&self) -> bool;
    fn is_in_floatable_fluid(&self) -> bool;
    /// `canStandOnFluid(fluidState)`.
    fn can_stand_on_fluid(&self, fluid: FluidState) -> bool;
    /// `getPathfindingMalus(type)`.
    fn get_pathfinding_malus(&self, ty: PathType) -> f32;
    /// `setPathfindingMalus(type, cost)`.
    fn set_pathfinding_malus(&mut self, ty: PathType, cost: f32);
    /// `maxUpStep()`.
    fn max_up_step(&self) -> f32;
    /// `getMaxFallDistance()`.
    fn get_max_fall_distance(&self) -> i32;
    fn on_pathfinding_start(&mut self);
    fn on_pathfinding_done(&mut self);
    /// `getRandom()`.
    fn random(&mut self) -> &mut dyn RandomSource;
    /// `CollisionContext.of(mob)`.
    fn collision_context(&self) -> CollisionContext;
    /// The live entity behind this view (for mob-specific node evaluators, e.g. the creaking's home), if any.
    fn entity(&self) -> Option<&dyn Entity> {
        None
    }
}

/// A live mob in `level`.
pub struct EntityMob<'a> {
    pub entity: &'a mut dyn Entity,
    pub level: &'a ServerLevel,
}

impl<'a> EntityMob<'a> {
    pub fn new(entity: &'a mut dyn Entity, level: &'a ServerLevel) -> Self {
        EntityMob { entity, level }
    }
}

impl PathfindingMob for EntityMob<'_> {
    fn id(&self) -> EntityId {
        self.entity.id()
    }
    fn position(&self) -> Vec3 {
        self.entity.base().position()
    }
    fn block_position(&self) -> BlockPos {
        self.entity.base().block_position()
    }
    fn bounding_box(&self) -> AABB {
        self.entity.base().bounding_box()
    }
    fn bb_width(&self) -> f32 {
        self.entity.base().get_bb_width()
    }
    fn bb_height(&self) -> f32 {
        self.entity.base().get_bb_height()
    }
    fn on_ground(&self) -> bool {
        self.entity.base().on_ground()
    }
    fn is_in_water(&self) -> bool {
        self.entity.is_in_water()
    }
    fn is_in_liquid(&self) -> bool {
        self.entity.is_in_liquid()
    }
    fn is_in_floatable_fluid(&self) -> bool {
        self.entity.is_in_floatable_fluid()
    }
    fn can_stand_on_fluid(&self, fluid: FluidState) -> bool {
        self.entity
            .can_stand_on_fluid_fn()
            .is_some_and(|f| f(fluid.fluid()))
    }
    fn get_pathfinding_malus(&self, ty: PathType) -> f32 {
        self.entity.get_pathfinding_malus(self.level, ty)
    }
    fn set_pathfinding_malus(&mut self, ty: PathType, cost: f32) {
        self.entity.set_pathfinding_malus(ty, cost);
    }
    fn max_up_step(&self) -> f32 {
        self.entity.max_up_step()
    }
    fn get_max_fall_distance(&self) -> i32 {
        self.entity.get_max_fall_distance(self.level)
    }
    fn on_pathfinding_start(&mut self) {
        self.entity.on_pathfinding_start();
    }
    fn on_pathfinding_done(&mut self) {
        self.entity.on_pathfinding_done();
    }
    fn random(&mut self) -> &mut dyn RandomSource {
        &mut self.entity.base_mut().random
    }
    fn collision_context(&self) -> CollisionContext {
        crate::world::entity::movement::collision_context(&*self.entity)
    }
    fn entity(&self) -> Option<&dyn Entity> {
        Some(&*self.entity)
    }
}
