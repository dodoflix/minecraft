//! `net.minecraft.world.level.block.GrowingPlantBlock` (base of the kelp / vines head and body blocks).
//!
//! Java's constructor fields and virtual methods are the [`GrowingPlant`] trait, implemented by every concrete
//! subclass; [`can_survive`] is the shared `canSurvive`.
use super::prelude::*;

pub trait GrowingPlant {
    /// `growthDirection`.
    fn growth_direction(&self) -> Direction;
    /// `scheduleFluidTicks`.
    fn schedule_fluid_ticks(&self) -> bool;
    /// `getHeadBlock()`.
    fn head_block(&self) -> Block;
    /// `getBodyBlock()`.
    fn body_block(&self) -> Block;
    /// `canAttachTo(state)`.
    fn can_attach_to(&self, _state: BlockState) -> bool {
        true
    }
}

/// `GrowingPlantBlock.canSurvive`.
pub fn can_survive(this: &dyn GrowingPlant, level: &dyn LevelReader, pos: BlockPos) -> bool {
    let attached_to_pos = pos.relative(this.growth_direction().get_opposite());
    let attached_to_state = level.get_block_state(attached_to_pos);
    if !this.can_attach_to(attached_to_state) {
        false
    } else {
        attached_to_state.is_block(this.head_block())
            || attached_to_state.is_block(this.body_block())
            || attached_to_state.is_face_sturdy_at(level, attached_to_pos, this.growth_direction())
    }
}
