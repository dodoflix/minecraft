//! `net.minecraft.world.level.block.ScaffoldingBlock`.
use super::prelude::*;
use crate::world::level::collision::CollisionContext;
use mc_core::phys::shapes::{Shapes, VoxelShape};
use mc_data::Item;

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

fn boxp(x0: f64, y0: f64, z0: f64, x1: f64, y1: f64, z1: f64) -> VoxelShape {
    Shapes::create(mc_core::phys::AABB::new(x0 / 16.0, y0 / 16.0, z0 / 16.0, x1 / 16.0, y1 / 16.0, z1 / 16.0))
}

/// `SHAPE_STABLE`: the top plate and the four corner legs.
fn shape_stable() -> &'static VoxelShape {
    static S: std::sync::LazyLock<VoxelShape> = std::sync::LazyLock::new(|| {
        let mut s = boxp(0.0, 14.0, 0.0, 16.0, 16.0, 16.0);
        for (x, z) in [(0.0, 0.0), (14.0, 0.0), (0.0, 14.0), (14.0, 14.0)] {
            s = Shapes::or(&s, &boxp(x, 0.0, z, x + 2.0, 16.0, z + 2.0));
        }
        s
    });
    &S
}

/// `SHAPE_UNSTABLE_BOTTOM`.
fn shape_unstable_bottom() -> &'static VoxelShape {
    static S: std::sync::LazyLock<VoxelShape> = std::sync::LazyLock::new(|| boxp(0.0, 0.0, 0.0, 16.0, 2.0, 16.0));
    &S
}

/// `SHAPE_UNSTABLE`: stable + bottom plate + the four bottom edge rails.
fn shape_unstable() -> &'static VoxelShape {
    static S: std::sync::LazyLock<VoxelShape> = std::sync::LazyLock::new(|| {
        let mut s = Shapes::or(shape_stable(), shape_unstable_bottom());
        for (x0, z0, x1, z1) in [(0.0, 0.0, 16.0, 2.0), (0.0, 14.0, 16.0, 16.0), (0.0, 0.0, 2.0, 16.0), (14.0, 0.0, 16.0, 16.0)] {
            s = Shapes::or(&s, &boxp(x0, 0.0, z0, x1, 2.0, z1));
        }
        s
    });
    &S
}

impl BlockBehaviour for ScaffoldingBlock {
    fn get_shape(&self, state: BlockState, _level: &dyn BlockGetter, _pos: BlockPos, context: &CollisionContext) -> VoxelShape {
        if !context.is_holding_item(Item::SCAFFOLDING) {
            if state.get(props::BOTTOM) { shape_unstable().clone() } else { shape_stable().clone() }
        } else {
            Shapes::block()
        }
    }
    fn get_collision_shape(&self, state: BlockState, _level: &dyn BlockGetter, pos: BlockPos, context: &CollisionContext) -> VoxelShape {
        if context.is_placement() {
            Shapes::empty()
        } else if context.is_above(&Shapes::block(), pos, true) && !context.is_descending() {
            shape_stable().clone()
        } else if state.get(props::STABILITY_DISTANCE) != 0
            && state.get(props::BOTTOM)
            && context.is_above(&Shapes::block().move_(0.0, -1.0, 0.0), pos, true)
        {
            shape_unstable_bottom().clone()
        } else {
            Shapes::empty()
        }
    }
    fn can_survive(&self, _state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        get_distance(level, pos) < 7
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if state.get(props::WATERLOGGED) {
            helpers::schedule_water_tick(level, pos);
        }
        // `!level.isClientSide()` always holds on the server
        helpers::schedule_block_tick(level, pos, state.block(), 1);
        state
    });
}

register_block_class!("ScaffoldingBlock", |_| Box::new(ScaffoldingBlock));
