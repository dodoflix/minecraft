//! `net.minecraft.world.level.block.NetherPortalBlock`.
use super::prelude::*;

pub struct NetherPortalBlock;

/// The part of `PortalShape` that `NetherPortalBlock.updateShape` uses (`findAnyShape(..).isComplete()`).
struct PortalShape {
    num_portal_blocks: i32,
    height: i32,
    width: i32,
}

impl PortalShape {
    fn is_valid(&self) -> bool {
        (2..=21).contains(&self.width) && (3..=21).contains(&self.height)
    }

    fn is_complete(&self) -> bool {
        self.is_valid() && self.num_portal_blocks == self.width * self.height
    }

    /// `PortalShape.findAnyShape`.
    fn find_any_shape(level: &dyn BlockGetter, pos: BlockPos, axis: Axis) -> PortalShape {
        let right_dir = if axis == Axis::X { Direction::West } else { Direction::South };
        let Some(bottom_left) = Self::calculate_bottom_left(level, right_dir, pos) else {
            return PortalShape { num_portal_blocks: 0, height: 0, width: 0 };
        };
        let width = Self::calculate_width(level, bottom_left, right_dir);
        if width == 0 {
            return PortalShape { num_portal_blocks: 0, height: 0, width: 0 };
        }
        let mut count = 0;
        let height = Self::calculate_height(level, bottom_left, right_dir, width, &mut count);
        PortalShape { num_portal_blocks: count, height, width }
    }

    fn calculate_bottom_left(level: &dyn BlockGetter, right_dir: Direction, mut pos: BlockPos) -> Option<BlockPos> {
        let min_y = level.get_min_y().max(pos.y() - 21);
        while pos.y() > min_y && Self::is_empty(level.get_block_state(pos.below())) {
            pos = pos.below();
        }
        let left_dir = right_dir.get_opposite();
        let edge = Self::distance_until_edge_above_frame(level, pos, left_dir) - 1;
        if edge < 0 { None } else { Some(pos.relative_n(left_dir, edge)) }
    }

    fn calculate_width(level: &dyn BlockGetter, bottom_left: BlockPos, right_dir: Direction) -> i32 {
        let width = Self::distance_until_edge_above_frame(level, bottom_left, right_dir);
        if (2..=21).contains(&width) { width } else { 0 }
    }

    fn is_frame(state: BlockState) -> bool {
        state.is_tag("nether_portal_frame")
    }

    fn distance_until_edge_above_frame(level: &dyn BlockGetter, pos: BlockPos, direction: Direction) -> i32 {
        for width in 0..=21 {
            let block_pos = pos.relative_n(direction, width);
            let block_state = level.get_block_state(block_pos);
            if !Self::is_empty(block_state) {
                if Self::is_frame(block_state) {
                    return width;
                }
                break;
            }
            let below_state = level.get_block_state(block_pos.below());
            if !Self::is_frame(below_state) {
                break;
            }
        }
        0
    }

    fn calculate_height(level: &dyn BlockGetter, bottom_left: BlockPos, right_dir: Direction, width: i32, count: &mut i32) -> i32 {
        let height = Self::distance_until_top(level, bottom_left, right_dir, width, count);
        if (3..=21).contains(&height) && Self::has_top_frame(level, bottom_left, right_dir, width, height) { height } else { 0 }
    }

    fn has_top_frame(level: &dyn BlockGetter, bottom_left: BlockPos, right_dir: Direction, width: i32, height: i32) -> bool {
        (0..width).all(|i| Self::is_frame(level.get_block_state(bottom_left.relative_n(Direction::Up, height).relative_n(right_dir, i))))
    }

    fn distance_until_top(level: &dyn BlockGetter, bottom_left: BlockPos, right_dir: Direction, width: i32, count: &mut i32) -> i32 {
        for height in 0..21 {
            let row = bottom_left.relative_n(Direction::Up, height);
            if !Self::is_frame(level.get_block_state(row.relative_n(right_dir, -1))) {
                return height;
            }
            if !Self::is_frame(level.get_block_state(row.relative_n(right_dir, width))) {
                return height;
            }
            for i in 0..width {
                let state = level.get_block_state(row.relative_n(right_dir, i));
                if !Self::is_empty(state) {
                    return height;
                }
                if state.is_block(Block::NETHER_PORTAL) {
                    *count += 1;
                }
            }
        }
        21
    }

    fn is_empty(state: BlockState) -> bool {
        state.is_air() || state.is_tag("fire") || state.is_block(Block::NETHER_PORTAL)
    }
}

impl BlockBehaviour for NetherPortalBlock {
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        let update_axis = direction.get_axis();
        let axis = state.get(props::HORIZONTAL_AXIS);
        let wrong_axis = axis != update_axis && update_axis.is_horizontal();
        if !wrong_axis && !neighbour_state.is_block(state.block()) && !PortalShape::find_any_shape(&*level, pos, axis).is_complete() {
            air()
        } else {
            state
        }
    });
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        rotate_pillar_horizontal(state, rotation)
    }
}

/// `NetherPortalBlock.rotate` (same axis swap as `RotatedPillarBlock.rotatePillar`, on the `axis` property of the portal).
fn rotate_pillar_horizontal(state: BlockState, rotation: Rotation) -> BlockState {
    match rotation {
        Rotation::Counterclockwise90 | Rotation::Clockwise90 => match state.get(props::HORIZONTAL_AXIS) {
            Axis::X => state.set(props::HORIZONTAL_AXIS, Axis::Z),
            Axis::Z => state.set(props::HORIZONTAL_AXIS, Axis::X),
            _ => state,
        },
        _ => state,
    }
}

register_block_class!("NetherPortalBlock", |_| Box::new(NetherPortalBlock));
