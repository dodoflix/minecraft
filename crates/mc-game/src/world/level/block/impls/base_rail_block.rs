//! `net.minecraft.world.level.block.BaseRailBlock` (`RailBlock`, `PoweredRailBlock`, `DetectorRailBlock` below).
use super::prelude::*;
use mc_data::enums::RailShape;

pub struct BaseRailBlock;

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

/// `BlockBehaviour` of a rail class that rotates / mirrors its `shape` property (`$shape`).
macro_rules! rail_behaviour {
    ($ty:ty, $shape:expr) => {
        impl BlockBehaviour for $ty {
            fn can_survive(&self, _state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
                super::base_rail_block::can_survive(level, pos)
            }
            update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
                super::base_rail_block::update_shape(state, level, pos)
            });
            fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
                state.set($shape, super::base_rail_block::rotate_shape(state.get($shape), rotation))
            }
            fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
                state.set($shape, super::base_rail_block::mirror_shape(state.get($shape), mirror))
            }
        }
    };
}
pub(crate) use rail_behaviour;

impl BlockBehaviour for BaseRailBlock {
    fn can_survive(&self, _state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        can_survive(level, pos)
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        update_shape(state, level, pos)
    });
}

register_block_class!("BaseRailBlock", |_| Box::new(BaseRailBlock));
