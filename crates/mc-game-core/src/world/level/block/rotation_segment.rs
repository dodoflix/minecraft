//! `net.minecraft.world.level.block.state.properties.RotationSegment` over `SegmentedAnglePrecision(4)` (16 segments).

use mc_core::direction::Direction;
use mc_core::math::java::round_f32;

const PRECISION: i32 = 4;
const MASK: i32 = (1 << PRECISION) - 1;
const DEGREE_TO_ANGLE: f32 = (1 << PRECISION) as f32 / 360.0;
const ANGLE_TO_DEGREE: f32 = 360.0 / (1 << PRECISION) as f32;

/// `getMaxSegmentIndex()`.
pub const fn get_max_segment_index() -> i32 {
    MASK
}

/// `convertToSegment(Direction)`.
pub fn convert_direction_to_segment(direction: Direction) -> i32 {
    if direction.get_axis().is_vertical() {
        0
    } else {
        direction.get_2d_data_value() << (PRECISION - 2)
    }
}

/// `convertToSegment(float rotDegrees)`.
pub fn convert_to_segment(rot_degrees: f32) -> i32 {
    round_f32(rot_degrees * DEGREE_TO_ANGLE) & MASK
}

/// `convertToDirection(segment)`.
pub fn convert_to_direction(segment: i32) -> Option<Direction> {
    match segment {
        0 => Some(Direction::North),
        4 => Some(Direction::East),
        8 => Some(Direction::South),
        12 => Some(Direction::West),
        _ => None,
    }
}

/// `convertToDegrees(segment)`.
pub fn convert_to_degrees(segment: i32) -> f32 {
    let degrees = (segment & MASK) as f32 * ANGLE_TO_DEGREE;
    if degrees >= 180.0 {
        degrees - 360.0
    } else {
        degrees
    }
}
