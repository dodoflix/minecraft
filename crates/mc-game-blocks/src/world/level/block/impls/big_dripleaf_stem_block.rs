//! `net.minecraft.world.level.block.BigDripleafStemBlock` (extends `HorizontalDirectionalBlock`).
use super::growing_plant_body_block::get_top_connected_block;
use super::prelude::*;
use crate::server::level::ServerLevel;
use crate::world::level::block::bonemeal::BonemealSource;
use mc_data::fluid::{Fluid, FluidState};

pub struct BigDripleafStemBlock;

/// `BigDripleafStemBlock.place`.
pub fn place(level: &mut dyn LevelAccessor, pos: BlockPos, fluid_state: FluidState, facing: Direction) -> bool {
    let new_state = Block::BIG_DRIPLEAF_STEM
        .default_state()
        .set(props::WATERLOGGED, fluid_state.is_source_of_type(Fluid::WATER))
        .set(props::HORIZONTAL_FACING, facing);
    level.set_block_and_update(pos, new_state)
}

impl BlockBehaviour for BigDripleafStemBlock {
    crate::simple_waterlogged!();
    fn can_survive(&self, state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        let below_state = level.get_block_state(pos.below());
        let above_state = level.get_block_state(pos.above());
        (below_state.is_block(state.block()) || below_state.is_tag("supports_big_dripleaf"))
            && (above_state.is_block(state.block()) || above_state.is_block(Block::BIG_DRIPLEAF))
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if (direction == Direction::Down || direction == Direction::Up) && !state.can_survive(&*level, pos) {
            helpers::schedule_block_tick(level, pos, state.block(), 1);
        }
        if state.get(props::WATERLOGGED) {
            helpers::schedule_water_tick(level, pos);
        }
        state
    });
    fn tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        if !state.can_survive(&*level, pos) {
            level.destroy_block(pos, true, None, 512);
        }
    }
    fn get_clone_item_stack(&self, _state: BlockState, _level: &ServerLevel, _pos: BlockPos, _include_data: bool) -> crate::world::item::ItemStack {
        match Block::BIG_DRIPLEAF.item() {
            Some(item) => crate::world::item::ItemStack::new(item),
            None => crate::world::item::ItemStack::empty(),
        }
    }
    fn is_bonemealable(&self) -> bool {
        true
    }
    fn is_valid_bonemeal_target(&self, level: &dyn LevelReader, pos: BlockPos, state: BlockState, _source: BonemealSource) -> bool {
        get_top_connected_block(level, pos, state.block(), Direction::Up, Block::BIG_DRIPLEAF)
            .is_some_and(|head_pos| super::big_dripleaf_block::can_place_at(level, head_pos.above()))
    }
    fn perform_bonemeal(&self, level: &mut ServerLevel, pos: BlockPos, state: BlockState, _source: BonemealSource) {
        if let Some(head_pos) = get_top_connected_block(&*level, pos, state.block(), Direction::Up, Block::BIG_DRIPLEAF) {
            let place_head_pos = head_pos.above();
            let facing = state.get(props::HORIZONTAL_FACING);
            let fluid = level.get_fluid_state(head_pos);
            place(level, head_pos, fluid, facing);
            let fluid = level.get_fluid_state(place_head_pos);
            super::big_dripleaf_block::place(level, place_head_pos, fluid, facing);
        }
    }
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::HORIZONTAL_FACING, mirror)
    }
}

register_block_class!("BigDripleafStemBlock", |_| Box::new(BigDripleafStemBlock));
