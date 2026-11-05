//! `net.minecraft.world.level.block.LeavesBlock` (also the tinted / untinted particle variants, whose extra state is
//! client-only: `animateTick` particles) and `MangroveLeavesBlock` (bone meal grows a hanging propagule).
use super::prelude::*;
use crate::server::level::ServerLevel;
use crate::world::item::BlockPlaceContext;
use crate::world::level::block::bonemeal::{BonemealSource, BonemealType};

pub struct LeavesBlock;
pub struct MangroveLeavesBlock;

/// `LeavesBlock.getOptionalDistanceAt`.
pub fn get_optional_distance_at(state: BlockState) -> Option<i32> {
    if state.is_tag("prevents_nearby_leaf_decay") { Some(0) } else { state.try_get(props::DISTANCE) }
}

fn get_distance_at(state: BlockState) -> i32 {
    get_optional_distance_at(state).unwrap_or(7)
}

/// `LeavesBlock.updateDistance`.
pub fn update_distance(state: BlockState, level: &dyn BlockGetter, pos: BlockPos) -> BlockState {
    let mut new_distance = 7;
    for direction in Direction::ALL {
        new_distance = new_distance.min(get_distance_at(level.get_block_state(pos.relative(direction))) + 1);
        if new_distance == 1 {
            break;
        }
    }
    state.set(props::DISTANCE, new_distance)
}

/// `LeavesBlock.decaying`.
fn decaying(state: BlockState) -> bool {
    !state.get(props::PERSISTENT) && state.get(props::DISTANCE) == 7
}

fn update_shape(state: BlockState, level: &mut dyn LevelAccessor, pos: BlockPos, neighbour_state: BlockState) -> BlockState {
    if state.get(props::WATERLOGGED) {
        helpers::schedule_water_tick(level, pos);
    }
    let distance_from_neighbor = get_distance_at(neighbour_state) + 1;
    if distance_from_neighbor != 1 || state.get(props::DISTANCE) != distance_from_neighbor {
        helpers::schedule_block_tick(level, pos, state.block(), 1);
    }
    state
}

fn random_tick(state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
    if decaying(state) {
        helpers::drop_resources(level, state, pos, None);
        level.remove_block(pos, false);
    }
}

fn tick(state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
    level.set_block_and_update(pos, update_distance(state, &*level, pos));
}

fn get_state_for_placement(block: Block, level: &mut ServerLevel, context: &BlockPlaceContext) -> Option<BlockState> {
    let pos = context.get_clicked_pos();
    let replaced_fluid_state = level.get_fluid_state(pos);
    let state = block
        .default_state()
        .set(props::PERSISTENT, true)
        .set(props::WATERLOGGED, replaced_fluid_state.fluid() == mc_data::fluid::Fluid::WATER);
    Some(update_distance(state, &*level, pos))
}

impl BlockBehaviour for LeavesBlock {
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| { update_shape(state, level, pos, neighbour_state) });
    fn random_tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        random_tick(state, level, pos);
    }
    fn tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        tick(state, level, pos);
    }
    fn get_state_for_placement(&self, block: Block, level: &mut ServerLevel, context: &BlockPlaceContext) -> Option<BlockState> {
        get_state_for_placement(block, level, context)
    }
}

impl BlockBehaviour for MangroveLeavesBlock {
    crate::simple_waterlogged!();
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| { update_shape(state, level, pos, neighbour_state) });
    fn random_tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        random_tick(state, level, pos);
    }
    fn tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        tick(state, level, pos);
    }
    fn get_state_for_placement(&self, block: Block, level: &mut ServerLevel, context: &BlockPlaceContext) -> Option<BlockState> {
        get_state_for_placement(block, level, context)
    }

    fn is_bonemealable(&self) -> bool {
        true
    }
    fn is_valid_bonemeal_target(&self, level: &dyn LevelReader, pos: BlockPos, _state: BlockState, _source: BonemealSource) -> bool {
        level.get_block_state(pos.below()).is_air()
    }
    fn perform_bonemeal(&self, level: &mut ServerLevel, pos: BlockPos, _state: BlockState, _source: BonemealSource) {
        level.set_block(pos.below(), super::mangrove_propagule_block::create_new_hanging_propagule(0), 2, 512);
    }
    fn get_bonemeal_type(&self) -> BonemealType {
        BonemealType::Grower
    }
    fn get_bonemeal_particle_pos(&self, pos: BlockPos) -> BlockPos {
        pos.below()
    }
}

register_block_class!("LeavesBlock", |_| Box::new(LeavesBlock));
register_block_class!("MangroveLeavesBlock", |_| Box::new(MangroveLeavesBlock));
