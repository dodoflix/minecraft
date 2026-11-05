//! `net.minecraft.world.level.block.BaseRailBlock` (`RailBlock`, `PoweredRailBlock`, `DetectorRailBlock` below).
use super::prelude::*;
use crate::server::level::ServerLevel;
use crate::world::item::BlockPlaceContext;
use crate::world::level::block::rail_state::RailState;
use crate::world::level::redstone::Orientation;
use crate::world::level::signal_getter;
use mc_data::enums::RailShape;

/// `BaseRailBlock.rotate(shape, rotation)`.
pub fn rotate_shape(shape: RailShape, rotation: Rotation) -> RailShape {
    use RailShape::*;
    match rotation {
        Rotation::Clockwise180 => match shape {
            AscendingEast => AscendingWest,
            AscendingWest => AscendingEast,
            AscendingNorth => AscendingSouth,
            AscendingSouth => AscendingNorth,
            NorthSouth => NorthSouth,
            EastWest => EastWest,
            SouthEast => NorthWest,
            SouthWest => NorthEast,
            NorthWest => SouthEast,
            NorthEast => SouthWest,
        },
        Rotation::Counterclockwise90 => match shape {
            AscendingEast => AscendingNorth,
            AscendingWest => AscendingSouth,
            AscendingNorth => AscendingWest,
            AscendingSouth => AscendingEast,
            NorthSouth => EastWest,
            EastWest => NorthSouth,
            SouthEast => NorthEast,
            SouthWest => SouthEast,
            NorthWest => SouthWest,
            NorthEast => NorthWest,
        },
        Rotation::Clockwise90 => match shape {
            AscendingEast => AscendingSouth,
            AscendingWest => AscendingNorth,
            AscendingNorth => AscendingEast,
            AscendingSouth => AscendingWest,
            NorthSouth => EastWest,
            EastWest => NorthSouth,
            SouthEast => SouthWest,
            SouthWest => NorthWest,
            NorthWest => NorthEast,
            NorthEast => SouthEast,
        },
        Rotation::None => shape,
    }
}

/// `BaseRailBlock.mirror(shape, mirror)`.
pub fn mirror_shape(shape: RailShape, mirror: Mirror) -> RailShape {
    use RailShape::*;
    match mirror {
        Mirror::LeftRight => match shape {
            AscendingNorth => AscendingSouth,
            AscendingSouth => AscendingNorth,
            SouthEast => NorthEast,
            SouthWest => NorthWest,
            NorthWest => SouthWest,
            NorthEast => SouthEast,
            other => other,
        },
        Mirror::FrontBack => match shape {
            AscendingEast => AscendingWest,
            AscendingWest => AscendingEast,
            SouthEast => SouthWest,
            SouthWest => SouthEast,
            NorthWest => NorthEast,
            NorthEast => NorthWest,
            other => other,
        },
        Mirror::None => shape,
    }
}

/// `BaseRailBlock.isRail(state)`.
pub fn is_rail_state(state: BlockState) -> bool {
    state.is_tag("rails") && helpers::is_instance_of(state, "BaseRailBlock")
}

/// `BaseRailBlock.isRail(level, pos)`.
pub fn is_rail(level: &dyn BlockGetter, pos: BlockPos) -> bool {
    is_rail_state(level.get_block_state(pos))
}

/// `BaseRailBlock.isStraight()` (powered, activator and detector rails).
pub fn is_straight(block: Block) -> bool {
    let classes = block.data().classes;
    classes.contains(&"PoweredRailBlock") || classes.contains(&"DetectorRailBlock")
}

/// `BaseRailBlock.getShapeProperty()`.
pub fn shape_property(state: BlockState) -> props::EnumProperty<RailShape> {
    if is_straight(state.block()) { props::RAIL_SHAPE_STRAIGHT } else { props::RAIL_SHAPE }
}

/// `RailShape.isSlope()`.
pub fn is_slope(shape: RailShape) -> bool {
    matches!(shape, RailShape::AscendingEast | RailShape::AscendingWest | RailShape::AscendingNorth | RailShape::AscendingSouth)
}

/// `BaseRailBlock.canSurvive`.
pub fn can_survive(level: &dyn LevelReader, pos: BlockPos) -> bool {
    helpers::can_support_rigid_block(level, pos.below())
}

/// `BaseRailBlock.updateShape`.
pub fn update_shape(state: BlockState, level: &mut dyn LevelAccessor, pos: BlockPos) -> BlockState {
    if state.get(props::WATERLOGGED) {
        helpers::schedule_water_tick(level, pos);
    }
    state
}

/// `BaseRailBlock.getStateForPlacement`.
pub fn get_state_for_placement(block: Block, level: &dyn BlockGetter, context: &BlockPlaceContext) -> BlockState {
    let is_water_source = level.get_fluid_state(context.get_clicked_pos()).fluid() == mc_data::fluid::Fluid::WATER;
    let direction = context.get_horizontal_direction();
    let is_east_west = direction == Direction::East || direction == Direction::West;
    let state = block.default_state();
    state
        .set(shape_property(state), if is_east_west { RailShape::EastWest } else { RailShape::NorthSouth })
        .set(props::WATERLOGGED, is_water_source)
}

/// `BaseRailBlock.shouldBeRemoved`.
fn should_be_removed(pos: BlockPos, level: &dyn BlockGetter, shape: RailShape) -> bool {
    if !helpers::can_support_rigid_block(level, pos.below()) {
        return true;
    }
    match shape {
        RailShape::AscendingEast => !helpers::can_support_rigid_block(level, pos.east()),
        RailShape::AscendingWest => !helpers::can_support_rigid_block(level, pos.west()),
        RailShape::AscendingNorth => !helpers::can_support_rigid_block(level, pos.north()),
        RailShape::AscendingSouth => !helpers::can_support_rigid_block(level, pos.south()),
        _ => false,
    }
}

/// `BaseRailBlock.updateDir`.
pub fn update_dir(level: &mut ServerLevel, pos: BlockPos, state: BlockState, first: bool) -> BlockState {
    let current = state.get(shape_property(state));
    let has_signal = signal_getter::has_neighbor_signal(&*level, pos);
    RailState::new(pos, state).place(level, has_signal, first, current).get_state()
}

/// `BaseRailBlock.updateState(state, level, pos, movedByPiston)`.
pub fn update_state_moved(state: BlockState, level: &mut ServerLevel, pos: BlockPos, moved_by_piston: bool) -> BlockState {
    let state = update_dir(level, pos, state, true);
    if is_straight(state.block()) {
        level.neighbor_changed_with_state(state, pos, state.block(), None, moved_by_piston);
    }
    state
}

/// `BaseRailBlock.onPlace`.
pub fn on_place(state: BlockState, level: &mut ServerLevel, pos: BlockPos, old_state: BlockState, moved_by_piston: bool) {
    if old_state.block() != state.block() {
        update_state_moved(state, level, pos, moved_by_piston);
    }
}

/// `BaseRailBlock.neighborChanged`; `update_state` is the class's `updateState(state, level, pos, block)`.
pub fn neighbor_changed(
    update_state: fn(BlockState, &mut ServerLevel, BlockPos, Block),
    state: BlockState,
    level: &mut ServerLevel,
    pos: BlockPos,
    block: Block,
    moved_by_piston: bool,
) {
    if level.get_block_state(pos).block() == state.block() {
        let shape = state.get(shape_property(state));
        if should_be_removed(pos, &*level, shape) {
            helpers::drop_resources(level, state, pos, None);
            level.remove_block(pos, moved_by_piston);
        } else {
            update_state(state, level, pos, block);
        }
    }
}

/// `BaseRailBlock.affectNeighborsAfterRemoval`.
pub fn affect_neighbors_after_removal(state: BlockState, level: &mut ServerLevel, pos: BlockPos, moved_by_piston: bool) {
    if !moved_by_piston {
        let block = state.block();
        if is_slope(state.get(shape_property(state))) {
            level.update_neighbors_at(pos.above(), block);
        }
        if is_straight(block) {
            level.update_neighbors_at(pos, block);
            level.update_neighbors_at(pos.below(), block);
        }
    }
}

/// `BlockBehaviour` of a rail class. `$update_state` is the class's `updateState(state, level, pos, block)`, `$on_place`
/// its `onPlace`;
/// `$extra` are further trait items.
macro_rules! rail_behaviour {
    ($ty:ty, $update_state:expr, $on_place:path $(, $($extra:tt)*)?) => {
        impl BlockBehaviour for $ty {
            crate::simple_waterlogged!();
            fn can_survive(&self, _state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
                super::base_rail_block::can_survive(level, pos)
            }
            update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
                super::base_rail_block::update_shape(state, level, pos)
            });
            fn get_state_for_placement(&self, block: Block, level: &mut crate::server::level::ServerLevel, context: &crate::world::item::BlockPlaceContext) -> Option<BlockState> {
                Some(super::base_rail_block::get_state_for_placement(block, &*level, context))
            }
            fn on_place(&self, state: BlockState, level: &mut crate::server::level::ServerLevel, pos: BlockPos, old_state: BlockState, moved_by_piston: bool) {
                $on_place(state, level, pos, old_state, moved_by_piston);
            }
            fn neighbor_changed(&self, state: BlockState, level: &mut crate::server::level::ServerLevel, pos: BlockPos, block: Block, _orientation: Option<crate::world::level::redstone::Orientation>, moved_by_piston: bool) {
                super::base_rail_block::neighbor_changed($update_state, state, level, pos, block, moved_by_piston);
            }
            fn affect_neighbors_after_removal(&self, state: BlockState, level: &mut crate::server::level::ServerLevel, pos: BlockPos, moved_by_piston: bool) {
                super::base_rail_block::affect_neighbors_after_removal(state, level, pos, moved_by_piston);
            }
            fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
                let shape = super::base_rail_block::shape_property(state);
                state.set(shape, super::base_rail_block::rotate_shape(state.get(shape), rotation))
            }
            fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
                let shape = super::base_rail_block::shape_property(state);
                state.set(shape, super::base_rail_block::mirror_shape(state.get(shape), mirror))
            }
            $($($extra)*)?
        }
    };
}
pub(crate) use rail_behaviour;
