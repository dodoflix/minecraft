//! `net.minecraft.world.level.block.DoublePlantBlock` (extends `VegetationBlock`; also `TallFlowerBlock`).
use super::prelude::*;
use super::vegetation_block::{self, MayPlaceOn};
use crate::server::level::ServerLevel;
use crate::world::entity::ServerPlayer;
use crate::world::item::{BlockPlaceContext, ItemStack};
use mc_core::math::mth;
use mc_data::enums::DoubleBlockHalf;

pub struct DoublePlantBlock;
impl MayPlaceOn for DoublePlantBlock {}

/// `DoublePlantBlock.updateShape` (`neighbour_state.is(this)` compares with the state's own block).
pub fn update_shape(
    state: BlockState,
    level: &dyn LevelReader,
    pos: BlockPos,
    direction_to_neighbour: Direction,
    neighbour_state: BlockState,
) -> BlockState {
    let half = state.get(props::DOUBLE_BLOCK_HALF);
    if direction_to_neighbour.get_axis() != Axis::Y
        || (half == DoubleBlockHalf::Lower) != (direction_to_neighbour == Direction::Up)
        || (neighbour_state.is_block(state.block())
            && neighbour_state.get(props::DOUBLE_BLOCK_HALF) != half)
    {
        if half == DoubleBlockHalf::Lower
            && direction_to_neighbour == Direction::Down
            && !state.can_survive(level, pos)
        {
            air()
        } else {
            vegetation_block::update_shape(state, level, pos)
        }
    } else {
        air()
    }
}

/// `DoublePlantBlock.canSurvive` (`this` supplies `mayPlaceOn` for the lower half).
pub fn can_survive(
    this: &dyn MayPlaceOn,
    state: BlockState,
    level: &dyn LevelReader,
    pos: BlockPos,
) -> bool {
    if state.get(props::DOUBLE_BLOCK_HALF) != DoubleBlockHalf::Upper {
        vegetation_block::can_survive(this, level, pos)
    } else {
        let below_state = level.get_block_state(pos.below());
        below_state.is_block(state.block())
            && below_state.get(props::DOUBLE_BLOCK_HALF) == DoubleBlockHalf::Lower
    }
}

/// `DoublePlantBlock.placeAt`.
pub fn place_at(
    level: &mut dyn LevelAccessor,
    state: BlockState,
    lower_pos: BlockPos,
    update_type: u32,
) {
    let upper_pos = lower_pos.above();
    let lower = copy_waterlogged_from(
        &*level,
        lower_pos,
        state.set(props::DOUBLE_BLOCK_HALF, DoubleBlockHalf::Lower),
    );
    level.set_block_flags(lower_pos, lower, update_type);
    let upper = copy_waterlogged_from(
        &*level,
        upper_pos,
        state.set(props::DOUBLE_BLOCK_HALF, DoubleBlockHalf::Upper),
    );
    level.set_block_flags(upper_pos, upper, update_type);
}

/// `DoublePlantBlock.copyWaterloggedFrom`.
pub fn copy_waterlogged_from(
    level: &dyn LevelReader,
    pos: BlockPos,
    state: BlockState,
) -> BlockState {
    if state.has_property(props::WATERLOGGED) {
        state.set(props::WATERLOGGED, helpers::is_water_at(level, pos))
    } else {
        state
    }
}

/// `DoublePlantBlock.getStateForPlacement`: both halves must fit.
pub fn get_state_for_placement(
    block: Block,
    level: &mut ServerLevel,
    context: &BlockPlaceContext,
) -> Option<BlockState> {
    let pos = context.get_clicked_pos();
    if pos.y() < level.get_max_y()
        && level
            .get_block_state(pos.above())
            .can_be_replaced_by(context)
    {
        Some(block.default_state())
    } else {
        None
    }
}

/// `DoublePlantBlock.setPlacedBy`: the upper half.
pub fn set_placed_by(block: Block, level: &mut ServerLevel, pos: BlockPos) {
    let above_pos = pos.above();
    let upper = copy_waterlogged_from(
        &*level,
        above_pos,
        block
            .default_state()
            .set(props::DOUBLE_BLOCK_HALF, DoubleBlockHalf::Upper),
    );
    level.set_block_and_update(above_pos, upper);
}

/// `DoublePlantBlock.playerWillDestroy` before its `super` call (destroy particles and the game event): the drops of a
/// survival player are dropped here, a creative player removes the lower half without drops.
pub fn player_will_destroy(
    level: &mut ServerLevel,
    pos: BlockPos,
    state: BlockState,
    player: &mut ServerPlayer,
) -> BlockState {
    if player.player.abilities.instabuild {
        prevent_drop_from_bottom_part(level, pos, state, player);
    } else {
        let tool = player.get_item_in_hand(crate::world::item::InteractionHand::MainHand);
        crate::world::level::storage::loot::block_drops::drop_resources_with_tool(
            level,
            state,
            pos,
            None,
            Some(&*player),
            &tool,
        );
    }
    crate::server::level::block_break::player_will_destroy(level, pos, state, player)
}

/// `DoublePlantBlock.playerDestroy`: `super.playerDestroy` with `Blocks.AIR` as the state (the drops were made in
/// `playerWillDestroy`).
pub fn player_destroy(
    level: &mut ServerLevel,
    player: &mut ServerPlayer,
    pos: BlockPos,
    block_entity: Option<&dyn crate::world::block_entity::BlockEntity>,
    destroyed_with: &ItemStack,
) {
    crate::server::level::block_break::player_destroy(
        level,
        player,
        pos,
        helpers::air(),
        block_entity,
        destroyed_with,
    );
}

/// `DoublePlantBlock.preventDropFromBottomPart`.
fn prevent_drop_from_bottom_part(
    level: &mut ServerLevel,
    pos: BlockPos,
    state: BlockState,
    player: &ServerPlayer,
) {
    if state.get(props::DOUBLE_BLOCK_HALF) == DoubleBlockHalf::Upper {
        let bottom_pos = pos.below();
        let bottom_state = level.get_block_state(bottom_pos);
        if bottom_state.is_block(state.block())
            && bottom_state.get(props::DOUBLE_BLOCK_HALF) == DoubleBlockHalf::Lower
        {
            let new_state = if bottom_state.get_fluid_state().is_tag("water") {
                Block::WATER.default_state()
            } else {
                helpers::air()
            };
            level.set_block_flags(bottom_pos, new_state, 35);
            level.level_event(
                Some(player.base_id()),
                2001,
                bottom_pos,
                bottom_state.id() as i32,
            );
        }
    }
}

/// `DoublePlantBlock.getSeed`.
pub fn get_seed(state: BlockState, pos: BlockPos) -> i64 {
    mth::get_seed_xyz(
        pos.x(),
        pos.below_n(
            if state.get(props::DOUBLE_BLOCK_HALF) == DoubleBlockHalf::Lower {
                0
            } else {
                1
            },
        )
        .y(),
        pos.z(),
    )
}

/// `BlockBehaviour` of every `DoublePlantBlock` subclass without own overrides of the methods below.
#[macro_export]
macro_rules! double_plant_behaviour_methods {
    () => {
        fn can_survive(&self, state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
            $crate::world::level::block::impls::double_plant_block::can_survive(
                self, state, level, pos,
            )
        }
        update_shape!(
            |self, state, level, pos, direction, neighbour_pos, neighbour_state| {
                $crate::world::level::block::impls::double_plant_block::update_shape(
                    state,
                    &*level,
                    pos,
                    direction,
                    neighbour_state,
                )
            }
        );
        fn get_state_for_placement(
            &self,
            block: Block,
            level: &mut $crate::server::level::ServerLevel,
            context: &$crate::world::item::BlockPlaceContext,
        ) -> Option<BlockState> {
            $crate::world::level::block::impls::double_plant_block::get_state_for_placement(
                block, level, context,
            )
        }
        fn set_placed_by(
            &self,
            state: BlockState,
            level: &mut $crate::server::level::ServerLevel,
            pos: BlockPos,
            _placer: Option<&mut $crate::world::entity::ServerPlayer>,
            _stack: &$crate::world::item::ItemStack,
        ) {
            $crate::world::level::block::impls::double_plant_block::set_placed_by(
                state.block(),
                level,
                pos,
            );
        }
        fn player_will_destroy(
            &self,
            state: BlockState,
            level: &mut $crate::server::level::ServerLevel,
            pos: BlockPos,
            player: &mut $crate::world::entity::ServerPlayer,
        ) -> BlockState {
            $crate::world::level::block::impls::double_plant_block::player_will_destroy(
                level, pos, state, player,
            )
        }
        fn player_destroy(
            &self,
            level: &mut $crate::server::level::ServerLevel,
            player: &mut $crate::world::entity::ServerPlayer,
            pos: BlockPos,
            _state: BlockState,
            block_entity: Option<&dyn $crate::world::block_entity::BlockEntity>,
            destroyed_with: &$crate::world::item::ItemStack,
        ) {
            $crate::world::level::block::impls::double_plant_block::player_destroy(
                level,
                player,
                pos,
                block_entity,
                destroyed_with,
            );
        }
        fn get_seed(&self, state: BlockState, pos: BlockPos) -> i64 {
            $crate::world::level::block::impls::double_plant_block::get_seed(state, pos)
        }
    };
}
pub use double_plant_behaviour_methods;

impl BlockBehaviour for DoublePlantBlock {
    double_plant_behaviour_methods!();
}

register_block_class!("DoublePlantBlock", |_| Box::new(DoublePlantBlock));
