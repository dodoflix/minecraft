//! `net.minecraft.world.level.block.ScaffoldingBlock`.
use mc_core::phys::shapes::{Shapes, VoxelShape};
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::item::BlockPlaceContext;
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::world::level::collision::CollisionContext;
use mc_game_core::world::level::shapes::{box_, box_z, column};
use mc_game_core::{register_block_class, update_shape};
use std::sync::LazyLock;

const TICK_DELAY: i32 = 1;
const STABILITY_MAX_DISTANCE: i32 = 7;

fn or_all(shapes: impl IntoIterator<Item = VoxelShape>) -> VoxelShape {
    shapes
        .into_iter()
        .fold(Shapes::empty(), |acc, shape| Shapes::or(&acc, &shape))
}

static SHAPE_STABLE: LazyLock<VoxelShape> = LazyLock::new(|| {
    Shapes::or(
        &column(16.0, 14.0, 16.0),
        &or_all(Shapes::rotate_horizontal(&box_(0.0, 0.0, 0.0, 2.0, 16.0, 2.0)).into_values()),
    )
});
static SHAPE_UNSTABLE_BOTTOM: LazyLock<VoxelShape> = LazyLock::new(|| column(16.0, 0.0, 2.0));
static SHAPE_UNSTABLE: LazyLock<VoxelShape> = LazyLock::new(|| {
    let rails = or_all(Shapes::rotate_horizontal(&box_z(16.0, 0.0, 2.0, 0.0, 2.0)).into_values());
    Shapes::or(&Shapes::or(&SHAPE_STABLE, &SHAPE_UNSTABLE_BOTTOM), &rails)
});
static SHAPE_BELOW_BLOCK: LazyLock<VoxelShape> =
    LazyLock::new(|| Shapes::block().move_(0.0, -1.0, 0.0).optimize());

pub struct ScaffoldingBlock;

/// `ScaffoldingBlock.getDistance`.
pub fn get_distance(level: &dyn BlockGetter, pos: BlockPos) -> i32 {
    let below_pos = pos.below();
    let below_state = level.get_block_state(below_pos);
    let mut distance = 7;
    if below_state.is_block(Block::SCAFFOLDING) {
        distance = below_state.get(props::STABILITY_DISTANCE);
    } else if below_state.is_face_sturdy_at(level, below_pos, Direction::Up) {
        return 0;
    }
    for direction in Direction::HORIZONTAL {
        let relative_state = level.get_block_state(pos.relative(direction));
        if relative_state.is_block(Block::SCAFFOLDING) {
            distance = distance.min(relative_state.get(props::STABILITY_DISTANCE) + 1);
            if distance == 1 {
                break;
            }
        }
    }
    distance
}

/// `isBottom`.
fn is_bottom(level: &dyn BlockGetter, pos: BlockPos, distance: i32) -> bool {
    distance > 0
        && !level
            .get_block_state(pos.below())
            .is_block(Block::SCAFFOLDING)
}

impl BlockBehaviour for ScaffoldingBlock {
    mc_game_core::simple_waterlogged!();
    fn get_shape(
        &self,
        state: BlockState,
        _level: &dyn BlockGetter,
        _pos: BlockPos,
        context: &CollisionContext,
    ) -> VoxelShape {
        if !state
            .block()
            .item()
            .is_some_and(|item| context.is_holding_item(item))
        {
            if state.get(props::BOTTOM) {
                SHAPE_UNSTABLE.clone()
            } else {
                SHAPE_STABLE.clone()
            }
        } else {
            Shapes::block()
        }
    }
    fn get_interaction_shape(
        &self,
        _state: BlockState,
        _level: &dyn BlockGetter,
        _pos: BlockPos,
    ) -> VoxelShape {
        Shapes::block()
    }
    fn can_be_replaced(&self, state: BlockState, context: &BlockPlaceContext) -> bool {
        let held = context.use_on.get_item_in_hand();
        !held.is_empty() && Some(held.item()) == state.block().item()
    }
    fn get_state_for_placement(
        &self,
        block: Block,
        level: &mut ServerLevel,
        context: &BlockPlaceContext,
    ) -> Option<BlockState> {
        let pos = context.get_clicked_pos();
        let distance = get_distance(level, pos);
        Some(
            block
                .default_state()
                .set(
                    props::WATERLOGGED,
                    level.get_fluid_state(pos).get_type() == mc_data::fluid::Fluid::WATER,
                )
                .set(props::STABILITY_DISTANCE, distance)
                .set(props::BOTTOM, is_bottom(level, pos, distance)),
        )
    }
    fn on_place(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        _old_state: BlockState,
        _moved_by_piston: bool,
    ) {
        level.schedule_tick_block(pos, state.block(), TICK_DELAY);
    }
    update_shape!(
        |self, state, level, pos, direction, neighbour_pos, neighbour_state| {
            if state.get(props::WATERLOGGED) {
                helpers::schedule_water_tick(level, pos);
            }
            // `!level.isClientSide()` always holds on the server
            helpers::schedule_block_tick(level, pos, state.block(), TICK_DELAY);
            state
        }
    );
    fn tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        let distance = get_distance(level, pos);
        let new_state = state
            .set(props::STABILITY_DISTANCE, distance)
            .set(props::BOTTOM, is_bottom(level, pos, distance));
        if new_state.get(props::STABILITY_DISTANCE) == STABILITY_MAX_DISTANCE {
            if state.get(props::STABILITY_DISTANCE) == STABILITY_MAX_DISTANCE {
                mc_game_core::world::entity::falling_block_entity::FallingBlockEntity::fall(
                    level, pos, new_state,
                );
            } else {
                level.destroy_block(pos, true, None, 512);
            }
        } else if state != new_state {
            level.set_block_and_update(pos, new_state);
        }
    }
    fn can_survive(&self, _state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        get_distance(level, pos) < STABILITY_MAX_DISTANCE
    }
    fn get_collision_shape(
        &self,
        state: BlockState,
        _level: &dyn BlockGetter,
        pos: BlockPos,
        context: &CollisionContext,
    ) -> VoxelShape {
        if context.is_placement() {
            Shapes::empty()
        } else if context.is_above(&Shapes::block(), pos, true) && !context.is_descending() {
            SHAPE_STABLE.clone()
        } else if state.get(props::STABILITY_DISTANCE) != 0
            && state.get(props::BOTTOM)
            && context.is_above(&SHAPE_BELOW_BLOCK, pos, true)
        {
            SHAPE_UNSTABLE_BOTTOM.clone()
        } else {
            Shapes::empty()
        }
    }
}

register_block_class!("ScaffoldingBlock", |_| Box::new(ScaffoldingBlock));
