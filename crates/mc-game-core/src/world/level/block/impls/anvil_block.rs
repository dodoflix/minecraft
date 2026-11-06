//! `net.minecraft.world.level.block.AnvilBlock` (extends `FallingBlock`).
use super::falling_block;
use super::prelude::*;
use crate::server::level::ServerLevel;
use crate::world::damagesource::DamageSource;
use crate::world::entity::Entity;
use crate::world::entity::falling_block_entity::FallingBlockEntity;

pub struct AnvilBlock;

/// `AnvilBlock.damage(state)`: the next damage stage, `None` for a damaged anvil.
pub fn damage(state: BlockState) -> Option<BlockState> {
    let facing = state.get(props::HORIZONTAL_FACING);
    if state.is_block(Block::ANVIL) {
        Some(
            Block::CHIPPED_ANVIL
                .default_state()
                .set(props::HORIZONTAL_FACING, facing),
        )
    } else if state.is_block(Block::CHIPPED_ANVIL) {
        Some(
            Block::DAMAGED_ANVIL
                .default_state()
                .set(props::HORIZONTAL_FACING, facing),
        )
    } else {
        None
    }
}

impl BlockBehaviour for AnvilBlock {
    update_shape!(
        |self, state, level, pos, direction, neighbour_pos, neighbour_state| {
            falling_block::update_shape(state, level, pos, 2)
        }
    );
    fn on_place(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        _old_state: BlockState,
        _moved_by_piston: bool,
    ) {
        falling_block::on_place(state, level, pos, 2);
    }
    fn tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        falling_block::tick(state, level, pos);
    }
    fn falling(&self, entity: &mut FallingBlockEntity) {
        entity.set_hurts_entities(2.0, 40);
    }
    fn on_land(
        &self,
        level: &mut ServerLevel,
        pos: BlockPos,
        _state: BlockState,
        _replaced: BlockState,
        entity: &FallingBlockEntity,
    ) {
        if !entity.base().is_silent() {
            level.level_event(None, 1031, pos, 0);
        }
    }
    fn on_broken_after_fall(
        &self,
        level: &mut ServerLevel,
        pos: BlockPos,
        entity: &FallingBlockEntity,
    ) {
        if !entity.base().is_silent() {
            level.level_event(None, 1029, pos, 0);
        }
    }
    fn get_fall_damage_source(
        &self,
        level: &ServerLevel,
        entity: &FallingBlockEntity,
    ) -> DamageSource {
        level.damage_sources().anvil(entity)
    }
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
}

register_block_class!("AnvilBlock", |_| Box::new(AnvilBlock));
