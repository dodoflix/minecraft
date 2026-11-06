//! `net.minecraft.world.level.block.GrowingPlantHeadBlock`.
//!
//! The concrete head blocks (kelp, cave vines, twisting/weeping vines) implement [`GrowingPlantHead`] and get their
//! [`BlockBehaviour`] methods from `growing_plant_head_behaviour!`.
use super::growing_plant_block::GrowingPlant;
use mc_core::random::RandomSource;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::item::BlockPlaceContext;
use mc_game_core::world::level::block::impls::prelude::*;

pub trait GrowingPlantHead: GrowingPlant {
    /// `growPerTickProbability` (constructor argument).
    fn grow_per_tick_probability(&self) -> f64;
    /// `getBlocksToGrowWhenBonemealed(random)`.
    fn blocks_to_grow_when_bonemealed(&self, random: &mut dyn RandomSource) -> i32;
    /// `canGrowInto(state)`.
    fn can_grow_into(&self, state: BlockState) -> bool;
    /// `getGrowIntoState(growFromState, random)`.
    fn get_grow_into_state(
        &self,
        grow_from_state: BlockState,
        _random: &mut dyn RandomSource,
    ) -> BlockState {
        grow_from_state.cycle(props::AGE_25)
    }
    /// `updateBodyAfterConvertedFromHead(headState, bodyState)`.
    fn update_body_after_converted_from_head(
        &self,
        _head_state: BlockState,
        body_state: BlockState,
    ) -> BlockState {
        body_state
    }
    /// Subclass guard in front of `GrowingPlantBlock.getStateForPlacement` (kelp: only placeable in a full water block).
    fn can_be_placed_in(&self, _fluid_state: mc_data::fluid::FluidState) -> bool {
        true
    }
}

/// The `GrowingPlantHeadBlock` implementor of a head block (`(GrowingPlantHeadBlock) block`).
pub fn head(block: Block) -> &'static dyn GrowingPlantHead {
    match block {
        b if b == Block::KELP => &super::kelp_block::KelpBlock,
        b if b == Block::CAVE_VINES => &super::cave_vines_block::CaveVinesBlock,
        b if b == Block::TWISTING_VINES => &super::twisting_vines_block::TwistingVinesBlock,
        b if b == Block::WEEPING_VINES => &super::weeping_vines_block::WeepingVinesBlock,
        _ => unreachable!("not a growing plant head block"),
    }
}

/// `GrowingPlantHeadBlock.getStateForPlacement(random)`.
pub fn get_state_for_placement(head_block: Block, random: &mut dyn RandomSource) -> BlockState {
    head_block
        .default_state()
        .set(props::AGE_25, random.next_int_bounded(25))
}

/// `GrowingPlantBlock.getStateForPlacement(context)` of a head block (kelp calls this as its `super`).
pub fn get_state_for_placement_ctx(
    this: &dyn GrowingPlantHead,
    block: Block,
    level: &mut ServerLevel,
    context: &BlockPlaceContext,
) -> Option<BlockState> {
    if !this.can_be_placed_in(level.get_fluid_state(context.get_clicked_pos())) {
        return None;
    }
    let growth_direction_state =
        level.get_block_state(context.get_clicked_pos().relative(this.growth_direction()));
    if !growth_direction_state.is_block(this.head_block())
        && !growth_direction_state.is_block(this.body_block())
    {
        Some(get_state_for_placement(block, level.random()))
    } else {
        Some(this.body_block().default_state())
    }
}

/// `GrowingPlantHeadBlock.randomTick`.
pub fn random_tick(
    this: &dyn GrowingPlantHead,
    state: BlockState,
    level: &mut ServerLevel,
    pos: BlockPos,
) {
    if state.get(props::AGE_25) < 25
        && level.random().next_double() < this.grow_per_tick_probability()
    {
        let growth_pos = pos.relative(this.growth_direction());
        if this.can_grow_into(level.get_block_state(growth_pos)) {
            let grow_into = this.get_grow_into_state(state, level.random());
            level.set_block_and_update(growth_pos, grow_into);
        }
    }
}

/// `GrowingPlantHeadBlock.updateShape`.
pub fn update_shape(
    this: &dyn GrowingPlantHead,
    state: BlockState,
    level: &mut dyn LevelAccessor,
    pos: BlockPos,
    direction_to_neighbour: Direction,
    neighbour_state: BlockState,
) -> BlockState {
    let growth = this.growth_direction();
    let this_block = state.block();
    if direction_to_neighbour == growth.get_opposite() {
        if !state.can_survive(&*level, pos) {
            helpers::schedule_block_tick(level, pos, this_block, 1);
        } else {
            let neighbor_in_growth_direction = level.get_block_state(pos.relative(growth));
            if neighbor_in_growth_direction.is_block(this_block)
                || neighbor_in_growth_direction.is_block(this.body_block())
            {
                return this.update_body_after_converted_from_head(
                    state,
                    this.body_block().default_state(),
                );
            }
        }
    }
    if direction_to_neighbour != growth
        || (!neighbour_state.is_block(this_block) && !neighbour_state.is_block(this.body_block()))
    {
        if this.schedule_fluid_ticks() {
            helpers::schedule_water_tick(level, pos);
        }
        state
    } else {
        this.update_body_after_converted_from_head(state, this.body_block().default_state())
    }
}

/// `GrowingPlantHeadBlock.isValidBonemealTarget`.
pub fn is_valid_bonemeal_target(
    this: &dyn GrowingPlantHead,
    level: &dyn LevelReader,
    pos: BlockPos,
) -> bool {
    let growth_pos = pos.relative(this.growth_direction());
    this.can_grow_into(level.get_block_state(growth_pos))
        && !level.is_outside_build_height(growth_pos.y())
}

/// `GrowingPlantHeadBlock.performBonemeal`.
pub fn perform_bonemeal(
    this: &dyn GrowingPlantHead,
    level: &mut ServerLevel,
    pos: BlockPos,
    state: BlockState,
) {
    let mut forward_pos = pos.relative(this.growth_direction());
    let mut next_age = (state.get(props::AGE_25) + 1).min(25);
    let blocks_to_grow = this.blocks_to_grow_when_bonemealed(level.random());
    let mut i = 0;
    while i < blocks_to_grow
        && this.can_grow_into(level.get_block_state(forward_pos))
        && !level.is_outside_build_height(forward_pos.y())
    {
        level.set_block_and_update(forward_pos, state.set(props::AGE_25, next_age));
        forward_pos = forward_pos.relative(this.growth_direction());
        next_age = (next_age + 1).min(25);
        i += 1;
    }
}

/// The `BlockBehaviour` methods every `GrowingPlantHeadBlock` shares, to be invoked inside the `impl BlockBehaviour`:
/// `growing_plant_head_behaviour!()`, or `growing_plant_head_behaviour!(custom_bonemeal)` when the block overrides
/// `isValidBonemealTarget`/`performBonemeal` itself (cave vines).
macro_rules! growing_plant_head_behaviour {
    () => {
        $crate::world::level::block::impls::growing_plant_head_block::growing_plant_head_behaviour!(@common);
        fn is_valid_bonemeal_target(&self, level: &dyn LevelReader, pos: BlockPos, _state: BlockState, _source: ::mc_game_core::world::level::block::bonemeal::BonemealSource) -> bool {
            $crate::world::level::block::impls::growing_plant_head_block::is_valid_bonemeal_target(self, level, pos)
        }
        fn perform_bonemeal(&self, level: &mut ::mc_game_core::server::level::ServerLevel, pos: BlockPos, state: BlockState, _source: ::mc_game_core::world::level::block::bonemeal::BonemealSource) {
            $crate::world::level::block::impls::growing_plant_head_block::perform_bonemeal(self, level, pos, state)
        }
    };
    (custom_bonemeal) => {
        $crate::world::level::block::impls::growing_plant_head_block::growing_plant_head_behaviour!(@common);
    };
    (@common) => {
        fn can_survive(&self, _state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
            $crate::world::level::block::impls::growing_plant_block::can_survive(self, level, pos)
        }
        update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
            $crate::world::level::block::impls::growing_plant_head_block::update_shape(self, state, level, pos, direction, neighbour_state)
        });
        fn tick(&self, state: BlockState, level: &mut ::mc_game_core::server::level::ServerLevel, pos: BlockPos) {
            $crate::world::level::block::impls::growing_plant_block::tick(state, level, pos);
        }
        fn random_tick(&self, state: BlockState, level: &mut ::mc_game_core::server::level::ServerLevel, pos: BlockPos) {
            $crate::world::level::block::impls::growing_plant_head_block::random_tick(self, state, level, pos);
        }
        fn is_bonemealable(&self) -> bool {
            true
        }
        fn get_state_for_placement(&self, block: Block, level: &mut ::mc_game_core::server::level::ServerLevel, context: &::mc_game_core::world::item::BlockPlaceContext) -> Option<BlockState> {
            $crate::world::level::block::impls::growing_plant_head_block::get_state_for_placement_ctx(self, block, level, context)
        }
    };
}
pub(crate) use growing_plant_head_behaviour;
