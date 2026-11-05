//! `net.minecraft.world.level.portal.PortalShape`.
use crate::server::level::ServerLevel;
use crate::util::block_util::FoundRectangle;
use crate::world::entity::dimensions::EntityDimensions;
use crate::world::entity::Entity;
use crate::world::level::block::BlockStateExt;
use crate::world::level::{BlockGetter, LevelAccessor};
use mc_core::direction::{Axis, Direction};
use mc_core::math::mth;
use mc_core::phys::shapes::Shapes;
use mc_core::phys::{Vec3, AABB};
use mc_core::pos::BlockPos;
use mc_data::block::{Block, BlockState};
use mc_data::props;

const MIN_WIDTH: i32 = 2;
pub const MAX_WIDTH: i32 = 21;
const MIN_HEIGHT: i32 = 3;
pub const MAX_HEIGHT: i32 = 21;
const SAFE_TRAVEL_MAX_ENTITY_XY: f32 = 4.0;

/// `PortalShape.FRAME`.
pub fn is_frame(state: BlockState) -> bool {
    state.is_tag("nether_portal_frame")
}

/// `PortalShape.isEmpty`.
fn is_empty(state: BlockState) -> bool {
    state.is_air() || state.is_tag("fire") || state.is_block(Block::NETHER_PORTAL)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PortalShape {
    pub axis: Axis,
    pub right_dir: Direction,
    pub num_portal_blocks: i32,
    pub bottom_left: BlockPos,
    pub height: i32,
    pub width: i32,
}

impl PortalShape {
    /// `findEmptyPortalShape(level, pos, preferredAxis)`.
    pub fn find_empty_portal_shape(level: &dyn BlockGetter, pos: BlockPos, preferred_axis: Axis) -> Option<PortalShape> {
        Self::find_portal_shape(level, pos, |shape| shape.is_valid() && shape.num_portal_blocks == 0, preferred_axis)
    }

    /// `findPortalShape(level, pos, isValid, preferredAxis)`.
    pub fn find_portal_shape(level: &dyn BlockGetter, pos: BlockPos, is_valid: impl Fn(&PortalShape) -> bool, preferred_axis: Axis) -> Option<PortalShape> {
        let first_axis = Self::find_any_shape(level, pos, preferred_axis);
        if is_valid(&first_axis) {
            return Some(first_axis);
        }
        let other_axis = if preferred_axis == Axis::X { Axis::Z } else { Axis::X };
        Some(Self::find_any_shape(level, pos, other_axis)).filter(|s| is_valid(s))
    }

    /// `findAnyShape(level, pos, axis)`.
    pub fn find_any_shape(level: &dyn BlockGetter, pos: BlockPos, axis: Axis) -> PortalShape {
        let right_dir = if axis == Axis::X { Direction::West } else { Direction::South };
        let Some(bottom_left) = Self::calculate_bottom_left(level, right_dir, pos) else {
            return PortalShape { axis, num_portal_blocks: 0, right_dir, bottom_left: pos, width: 0, height: 0 };
        };
        let width = Self::calculate_width(level, bottom_left, right_dir);
        if width == 0 {
            return PortalShape { axis, num_portal_blocks: 0, right_dir, bottom_left, width: 0, height: 0 };
        }
        let mut portal_block_count = 0;
        let height = Self::calculate_height(level, bottom_left, right_dir, width, &mut portal_block_count);
        PortalShape { axis, num_portal_blocks: portal_block_count, right_dir, bottom_left, width, height }
    }

    fn calculate_bottom_left(level: &dyn BlockGetter, right_dir: Direction, mut pos: BlockPos) -> Option<BlockPos> {
        let min_y = level.get_min_y().max(pos.y() - 21);
        while pos.y() > min_y && is_empty(level.get_block_state(pos.below())) {
            pos = pos.below();
        }
        let left_dir = right_dir.get_opposite();
        let edge = Self::get_distance_until_edge_above_frame(level, pos, left_dir) - 1;
        if edge < 0 { None } else { Some(pos.relative_n(left_dir, edge)) }
    }

    fn calculate_width(level: &dyn BlockGetter, bottom_left: BlockPos, right_dir: Direction) -> i32 {
        let width = Self::get_distance_until_edge_above_frame(level, bottom_left, right_dir);
        if (MIN_WIDTH..=MAX_WIDTH).contains(&width) { width } else { 0 }
    }

    fn get_distance_until_edge_above_frame(level: &dyn BlockGetter, pos: BlockPos, direction: Direction) -> i32 {
        for width in 0..=MAX_WIDTH {
            let block_pos = pos.relative_n(direction, width);
            let block_state = level.get_block_state(block_pos);
            if !is_empty(block_state) {
                if is_frame(block_state) {
                    return width;
                }
                break;
            }
            let below_state = level.get_block_state(block_pos.below());
            if !is_frame(below_state) {
                break;
            }
        }
        0
    }

    fn calculate_height(level: &dyn BlockGetter, bottom_left: BlockPos, right_dir: Direction, width: i32, portal_block_count: &mut i32) -> i32 {
        let height = Self::get_distance_until_top(level, bottom_left, right_dir, width, portal_block_count);
        if (MIN_HEIGHT..=MAX_HEIGHT).contains(&height) && Self::has_top_frame(level, bottom_left, right_dir, width, height) { height } else { 0 }
    }

    fn has_top_frame(level: &dyn BlockGetter, bottom_left: BlockPos, right_dir: Direction, width: i32, height: i32) -> bool {
        (0..width).all(|i| is_frame(level.get_block_state(bottom_left.relative_n(Direction::Up, height).relative_n(right_dir, i))))
    }

    fn get_distance_until_top(level: &dyn BlockGetter, bottom_left: BlockPos, right_dir: Direction, width: i32, portal_block_count: &mut i32) -> i32 {
        for height in 0..MAX_HEIGHT {
            let row = bottom_left.relative_n(Direction::Up, height);
            if !is_frame(level.get_block_state(row.relative_n(right_dir, -1))) {
                return height;
            }
            if !is_frame(level.get_block_state(row.relative_n(right_dir, width))) {
                return height;
            }
            for i in 0..width {
                let state = level.get_block_state(row.relative_n(right_dir, i));
                if !is_empty(state) {
                    return height;
                }
                if state.is_block(Block::NETHER_PORTAL) {
                    *portal_block_count += 1;
                }
            }
        }
        21
    }

    /// `isValid()`.
    pub fn is_valid(&self) -> bool {
        self.width >= MIN_WIDTH && self.width <= MAX_WIDTH && self.height >= MIN_HEIGHT && self.height <= MAX_HEIGHT
    }

    /// `createPortalBlocks(level)`.
    pub fn create_portal_blocks(&self, level: &mut dyn LevelAccessor) {
        let portal_state = Block::NETHER_PORTAL.default_state().set(props::HORIZONTAL_AXIS, self.axis);
        let end = self.bottom_left.relative_n(Direction::Up, self.height - 1).relative_n(self.right_dir, self.width - 1);
        for pos in BlockPos::between_closed(&self.bottom_left, &end) {
            level.set_block_flags(pos, portal_state, 18);
        }
    }

    /// `isComplete()`.
    pub fn is_complete(&self) -> bool {
        self.is_valid() && self.num_portal_blocks == self.width * self.height
    }

    /// `getRelativePosition(largestRectangleAround, axis, position, dimensions)`.
    pub fn get_relative_position(largest_rectangle_around: &FoundRectangle, axis: Axis, position: Vec3, dimensions: &EntityDimensions) -> Vec3 {
        let width = f64::from(largest_rectangle_around.axis1_size) - f64::from(dimensions.width);
        let height = f64::from(largest_rectangle_around.axis2_size) - f64::from(dimensions.height);
        let bottom_min = largest_rectangle_around.min_corner;
        let relative_right = if width > 0.0 {
            let bottom_start = f64::from(bottom_min.get(axis)) + f64::from(dimensions.width) / 2.0;
            mth::clamp_f64(mth::inverse_lerp_f64(position.get(axis) - bottom_start, 0.0, width), 0.0, 1.0)
        } else {
            0.5
        };

        let relative_up = if height > 0.0 {
            let height_axis = Axis::Y;
            mth::clamp_f64(mth::inverse_lerp_f64(position.get(height_axis) - f64::from(bottom_min.get(height_axis)), 0.0, height), 0.0, 1.0)
        } else {
            0.0
        };

        let forward_axis = if axis == Axis::X { Axis::Z } else { Axis::X };
        let relative_forward = position.get(forward_axis) - (f64::from(bottom_min.get(forward_axis)) + 0.5);
        Vec3::new(relative_right, relative_up, relative_forward)
    }

    /// `findCollisionFreePosition(bottomCenter, serverLevel, entity, dimensions)`.
    pub fn find_collision_free_position(bottom_center: Vec3, server_level: &ServerLevel, entity: &dyn Entity, dimensions: &EntityDimensions) -> Vec3 {
        if !(dimensions.width > SAFE_TRAVEL_MAX_ENTITY_XY) && !(dimensions.height > SAFE_TRAVEL_MAX_ENTITY_XY) {
            let half_height = f64::from(dimensions.height) / 2.0;
            let center = bottom_center.add_xyz(0.0, half_height, 0.0);
            let allowed_centers = Shapes::create(AABB::of_size(center, f64::from(dimensions.width), 0.0, f64::from(dimensions.width)).expand_towards(0.0, 1.0, 0.0).inflate(1.0E-6, 1.0E-6, 1.0E-6));
            let collision_free_position = server_level.find_free_position_for(entity, &allowed_centers, center, f64::from(dimensions.width), f64::from(dimensions.height), f64::from(dimensions.width));
            collision_free_position.map_or(bottom_center, |vec| vec.subtract_xyz(0.0, half_height, 0.0))
        } else {
            bottom_center
        }
    }
}
