//! `net.minecraft.world.level.block.PitcherCropBlock` (extends `DoublePlantBlock`).
use mc_game_core::{register_block_class, update_shape};
use mc_game_core::world::level::block::impls::crop_block;
use mc_game_core::world::level::block::impls::double_plant_block;
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::world::level::block::impls::vegetation_block::MayPlaceOn;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::item::BlockPlaceContext;
use mc_game_core::world::level::block::bonemeal::BonemealSource;
use mc_data::enums::DoubleBlockHalf;

pub struct PitcherCropBlock;

fn is_lower(state: BlockState) -> bool {
    state.is_block(Block::PITCHER_CROP) && state.get(props::DOUBLE_BLOCK_HALF) == DoubleBlockHalf::Lower
}

fn is_double(age: i32) -> bool {
    age >= 3
}

impl MayPlaceOn for PitcherCropBlock {
    fn may_place_on(&self, state: BlockState, _level: &dyn BlockGetter, _pos: BlockPos) -> bool {
        state.is_tag("supports_crops")
    }
}

/// `isMaxAge`.
fn is_max_age(state: BlockState) -> bool {
    state.get(props::AGE_4) >= 4
}

/// `canGrowInto`.
fn can_grow_into(level: &dyn LevelReader, pos: BlockPos) -> bool {
    let state = level.get_block_state(pos);
    state.is_air() || state.is_block(Block::PITCHER_CROP)
}

/// `canGrow(level, lowerPos, lowerState, newAge)`.
fn can_grow(level: &dyn LevelReader, lower_pos: BlockPos, lower_state: BlockState, new_age: i32) -> bool {
    !is_max_age(lower_state)
        && crop_block::has_sufficient_light(level, lower_pos)
        && !level.is_outside_build_height(lower_pos.above().y())
        && (!is_double(new_age) || can_grow_into(level, lower_pos.above()))
}

/// `grow(level, lowerState, lowerPos, increase)`.
fn grow(level: &mut ServerLevel, lower_state: BlockState, lower_pos: BlockPos, increase: i32) {
    let updated_age = (lower_state.get(props::AGE_4) + increase).min(4);
    if can_grow(&*level, lower_pos, lower_state, updated_age) {
        let new_lower_state = lower_state.set(props::AGE_4, updated_age);
        level.set_block_flags(lower_pos, new_lower_state, 2);
        if is_double(updated_age) {
            level.set_block_and_update(lower_pos.above(), new_lower_state.set(props::DOUBLE_BLOCK_HALF, DoubleBlockHalf::Upper));
        }
    }
}

/// `getLowerHalf(level, pos, state)`.
fn get_lower_half(level: &dyn LevelReader, pos: BlockPos, state: BlockState) -> Option<(BlockPos, BlockState)> {
    if is_lower(state) {
        Some((pos, state))
    } else {
        let lower_pos = pos.below();
        let lower_state = level.get_block_state(lower_pos);
        is_lower(lower_state).then_some((lower_pos, lower_state))
    }
}

impl BlockBehaviour for PitcherCropBlock {
    fn get_state_for_placement(&self, block: Block, _level: &mut ServerLevel, _context: &BlockPlaceContext) -> Option<BlockState> {
        Some(block.default_state())
    }
    fn can_be_replaced(&self, _state: BlockState, _context: &BlockPlaceContext) -> bool {
        false
    }
    fn set_placed_by(&self, _state: BlockState, _level: &mut ServerLevel, _pos: BlockPos, _placer: Option<&mut mc_game_core::world::entity::ServerPlayer>, _stack: &mc_game_core::world::item::ItemStack) {}
    fn entity_inside(
        &self,
        _state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        entity: &mut dyn mc_game_core::world::entity::Entity,
        _effect_applier: &mut dyn mc_game_core::world::entity::inside_block_effects::InsideBlockEffectApplier,
        _is_precise: bool,
    ) {
        mc_game_core::world::level::block::impls::vegetation_block::ravager_destroys_crop(level, pos, &*entity);
    }
    fn random_tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        let growth_speed = crop_block::get_growth_speed(state.block(), &*level, pos);
        if level.random().next_int_bounded((25.0f32 / growth_speed) as i32 + 1) == 0 {
            grow(level, state, pos, 1);
        }
    }
    fn is_bonemealable(&self) -> bool {
        true
    }
    fn is_valid_bonemeal_target(&self, level: &dyn LevelReader, pos: BlockPos, state: BlockState, _source: BonemealSource) -> bool {
        get_lower_half(level, pos, state).is_some_and(|(lower_pos, lower_state)| can_grow(level, lower_pos, lower_state, lower_state.get(props::AGE_4) + 1))
    }
    fn perform_bonemeal(&self, level: &mut ServerLevel, pos: BlockPos, state: BlockState, _source: BonemealSource) {
        if let Some((lower_pos, lower_state)) = get_lower_half(&*level, pos, state) {
            grow(level, lower_state, lower_pos, 1);
        }
    }
    fn get_seed(&self, state: BlockState, pos: BlockPos) -> i64 {
        double_plant_block::get_seed(state, pos)
    }
    fn player_destroy(
        &self,
        level: &mut ServerLevel,
        player: &mut mc_game_core::world::entity::ServerPlayer,
        pos: BlockPos,
        _state: BlockState,
        block_entity: Option<&dyn mc_game_core::world::block_entity::BlockEntity>,
        destroyed_with: &mc_game_core::world::item::ItemStack,
    ) {
        double_plant_block::player_destroy(level, player, pos, block_entity, destroyed_with);
    }
    fn player_will_destroy(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, player: &mut mc_game_core::world::entity::ServerPlayer) -> BlockState {
        double_plant_block::player_will_destroy(level, pos, state, player)
    }
    fn can_survive(&self, state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        if is_lower(state) && !crop_block::has_sufficient_light(level, pos) {
            false
        } else {
            double_plant_block::can_survive(self, state, level, pos)
        }
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if is_double(state.get(props::AGE_4)) {
            double_plant_block::update_shape(state, &*level, pos, direction, neighbour_state)
        } else if state.can_survive(&*level, pos) {
            state
        } else {
            air()
        }
    });
}

register_block_class!("PitcherCropBlock", |_| Box::new(PitcherCropBlock));
