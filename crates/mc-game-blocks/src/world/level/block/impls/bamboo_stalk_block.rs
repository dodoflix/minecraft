//! `net.minecraft.world.level.block.BambooStalkBlock`.
use super::prelude::*;
use crate::server::level::ServerLevel;
use crate::world::level::block::bonemeal::BonemealSource;
use mc_core::random::RandomSource;
use mc_data::enums::BambooLeaves;

pub struct BambooStalkBlock;

const MAX_HEIGHT: i32 = 16;

/// `getHeightAboveUpToMax`.
fn get_height_above_up_to_max(level: &dyn BlockGetter, pos: BlockPos) -> i32 {
    let mut height = 0;
    while height < MAX_HEIGHT && level.get_block_state(pos.above_n(height + 1)).is_block(Block::BAMBOO) {
        height += 1;
    }
    height
}

/// `getHeightBelowUpToMax`.
fn get_height_below_up_to_max(level: &dyn BlockGetter, pos: BlockPos) -> i32 {
    let mut height = 0;
    while height < MAX_HEIGHT && level.get_block_state(pos.below_n(height + 1)).is_block(Block::BAMBOO) {
        height += 1;
    }
    height
}

/// `growBamboo(state, level, pos, random, height)`.
fn grow_bamboo(state: BlockState, level: &mut ServerLevel, pos: BlockPos, height: i32) {
    let below_state = level.get_block_state(pos.below());
    let two_below_pos = pos.below_n(2);
    let two_below_state = level.get_block_state(two_below_pos);
    let mut leaves = BambooLeaves::None;
    if height >= 1 {
        if !below_state.is_block(Block::BAMBOO) || below_state.get(props::BAMBOO_LEAVES) == BambooLeaves::None {
            leaves = BambooLeaves::Small;
        } else if below_state.is_block(Block::BAMBOO) && below_state.get(props::BAMBOO_LEAVES) != BambooLeaves::None {
            leaves = BambooLeaves::Large;
            if two_below_state.is_block(Block::BAMBOO) {
                level.set_block_and_update(pos.below(), below_state.set(props::BAMBOO_LEAVES, BambooLeaves::Small));
                level.set_block_and_update(two_below_pos, two_below_state.set(props::BAMBOO_LEAVES, BambooLeaves::None));
            }
        }
    }
    let age = if state.get(props::AGE_1) != 1 && !two_below_state.is_block(Block::BAMBOO) { 0 } else { 1 };
    let stage = if (height < 11 || !(level.random().next_float() < 0.25)) && height != 15 { 0 } else { 1 };
    level.set_block_and_update(pos.above(), Block::BAMBOO.default_state().set(props::AGE_1, age).set(props::BAMBOO_LEAVES, leaves).set(props::STAGE, stage));
}

impl BlockBehaviour for BambooStalkBlock {
    fn get_state_for_placement(&self, block: Block, level: &mut ServerLevel, context: &crate::world::item::BlockPlaceContext) -> Option<BlockState> {
        let clicked_pos = context.get_clicked_pos();
        if !level.get_fluid_state(clicked_pos).is_empty() {
            return None;
        }
        let below_state = level.get_block_state(clicked_pos.below());
        if !below_state.is_tag("supports_bamboo") {
            return None;
        }
        if below_state.is_block(Block::BAMBOO_SAPLING) {
            Some(block.default_state().set(props::AGE_1, 0))
        } else if below_state.is_block(Block::BAMBOO) {
            let age = if below_state.get(props::AGE_1) > 0 { 1 } else { 0 };
            Some(block.default_state().set(props::AGE_1, age))
        } else {
            let above_state = level.get_block_state(clicked_pos.above());
            Some(if above_state.is_block(Block::BAMBOO) { block.default_state().set(props::AGE_1, above_state.get(props::AGE_1)) } else { Block::BAMBOO_SAPLING.default_state() })
        }
    }
    fn tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        if !state.can_survive(&*level, pos) {
            level.destroy_block(pos, true, None, 512);
        }
    }
    fn random_tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        if state.get(props::STAGE) == 0
            && level.random().next_int_bounded(3) == 0
            && level.is_empty_block(pos.above())
            && level.get_raw_brightness(pos.above(), 0) >= 9
        {
            let height = get_height_below_up_to_max(&*level, pos) + 1;
            if height < 16 {
                grow_bamboo(state, level, pos, height);
            }
        }
    }
    fn can_survive(&self, _state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        level.get_block_state(pos.below()).is_tag("supports_bamboo")
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if !state.can_survive(&*level, pos) {
            helpers::schedule_block_tick(level, pos, state.block(), 1);
        }
        if direction == Direction::Up && neighbour_state.is_block(Block::BAMBOO) && neighbour_state.get(props::AGE_1) > state.get(props::AGE_1) {
            state.cycle(props::AGE_1)
        } else {
            state
        }
    });
    fn is_bonemealable(&self) -> bool {
        true
    }
    fn is_valid_bonemeal_target(&self, level: &dyn LevelReader, pos: BlockPos, _state: BlockState, _source: BonemealSource) -> bool {
        let height_above = get_height_above_up_to_max(level, pos);
        let height_below = get_height_below_up_to_max(level, pos);
        let growth_pos = pos.above_n(height_above + 1);
        height_above + height_below + 1 < 16
            && level.get_block_state(pos.above_n(height_above)).get(props::STAGE) != 1
            && !level.is_outside_build_height(growth_pos.y())
            && level.is_empty_block(growth_pos)
    }
    fn perform_bonemeal(&self, level: &mut ServerLevel, pos: BlockPos, _state: BlockState, _source: BonemealSource) {
        let mut height_above = get_height_above_up_to_max(&*level, pos);
        let height_below = get_height_below_up_to_max(&*level, pos);
        let mut total_height = height_above + height_below + 1;
        let new_bamboo = 1 + level.random().next_int_bounded(2);
        for _ in 0..new_bamboo {
            let top_pos = pos.above_n(height_above);
            let top_state = level.get_block_state(top_pos);
            let growth_pos = top_pos.above();
            if total_height >= 16 || top_state.get(props::STAGE) == 1 || !level.is_empty_block(growth_pos) || level.is_outside_build_height(growth_pos.y()) {
                return;
            }
            grow_bamboo(top_state, level, top_pos, total_height);
            height_above += 1;
            total_height += 1;
        }
    }
}

register_block_class!("BambooStalkBlock", |_| Box::new(BambooStalkBlock));
