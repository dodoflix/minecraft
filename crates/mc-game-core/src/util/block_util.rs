//! `net.minecraft.util.BlockUtil`.
use crate::world::level::BlockGetter;
use crate::world::level::block::BlockStateExt;
use mc_core::direction::{Axis, AxisDirection, Direction};
use mc_core::math::mth;
use mc_core::pos::BlockPos;
use mc_data::block::Block;

pub const MAX_POSITION_DIFFERENCE_PACKING_RADIUS: i32 = 127;

/// `BlockUtil.FoundRectangle`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FoundRectangle {
    pub min_corner: BlockPos,
    pub axis1_size: i32,
    pub axis2_size: i32,
}

/// `BlockUtil.IntBounds`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IntBounds {
    pub min: i32,
    pub max: i32,
}

/// `BlockUtil.getLargestRectangleAround(center, axis1, limit1, axis2, limit2, test)`.
pub fn get_largest_rectangle_around(
    center: BlockPos,
    axis1: Axis,
    limit1: i32,
    axis2: Axis,
    limit2: i32,
    test: &mut dyn FnMut(BlockPos) -> bool,
) -> FoundRectangle {
    let negative_direction1 = Direction::get(AxisDirection::Negative, axis1);
    let positive_direction1 = negative_direction1.get_opposite();
    let negative_direction2 = Direction::get(AxisDirection::Negative, axis2);
    let positive_direction2 = negative_direction2.get_opposite();
    let negative_delta1 = get_limit(test, center, negative_direction1, limit1);
    let positive_delta1 = get_limit(test, center, positive_direction1, limit1);
    let center_index1 = negative_delta1;
    let mut bounds_by_axis1 =
        vec![IntBounds { min: 0, max: 0 }; (negative_delta1 + 1 + positive_delta1) as usize];
    bounds_by_axis1[negative_delta1 as usize] = IntBounds {
        min: get_limit(test, center, negative_direction2, limit2),
        max: get_limit(test, center, positive_direction2, limit2),
    };
    let center_index2 = bounds_by_axis1[negative_delta1 as usize].min;

    for i in 1..=negative_delta1 {
        let last_bounds = bounds_by_axis1[(center_index1 - (i - 1)) as usize];
        let at = center.relative_n(negative_direction1, i);
        bounds_by_axis1[(center_index1 - i) as usize] = IntBounds {
            min: get_limit(test, at, negative_direction2, last_bounds.min),
            max: get_limit(test, at, positive_direction2, last_bounds.max),
        };
    }

    for i in 1..=positive_delta1 {
        let last_bounds = bounds_by_axis1[(center_index1 + i - 1) as usize];
        let at = center.relative_n(positive_direction1, i);
        bounds_by_axis1[(center_index1 + i) as usize] = IntBounds {
            min: get_limit(test, at, negative_direction2, last_bounds.min),
            max: get_limit(test, at, positive_direction2, last_bounds.max),
        };
    }

    let mut min_axis1 = 0;
    let mut min_axis2 = 0;
    let mut size_axis1 = 0;
    let mut size_axis2 = 0;
    let mut columns = vec![0i32; bounds_by_axis1.len()];

    for i2 in (0..=center_index2).rev() {
        for (i1, bounds2) in bounds_by_axis1.iter().enumerate() {
            let min2 = center_index2 - bounds2.min;
            let max2 = center_index2 + bounds2.max;
            columns[i1] = if i2 >= min2 && i2 <= max2 {
                max2 + 1 - i2
            } else {
                0
            };
        }

        let (bounds_axis1, new_size_axis2) = get_max_rectangle_location(&columns);
        let new_size_axis1 = 1 + bounds_axis1.max - bounds_axis1.min;
        if new_size_axis1 * new_size_axis2 > size_axis1 * size_axis2 {
            min_axis1 = bounds_axis1.min;
            min_axis2 = i2;
            size_axis1 = new_size_axis1;
            size_axis2 = new_size_axis2;
        }
    }

    FoundRectangle {
        min_corner: center
            .relative_axis(axis1, min_axis1 - center_index1)
            .relative_axis(axis2, min_axis2 - center_index2),
        axis1_size: size_axis1,
        axis2_size: size_axis2,
    }
}

/// `getLimit(test, pos, direction, limit)` (the cursor starts at `start`).
fn get_limit(
    test: &mut dyn FnMut(BlockPos) -> bool,
    start: BlockPos,
    direction: Direction,
    limit: i32,
) -> i32 {
    let mut max = 0;
    while max < limit && test(start.relative_n(direction, max + 1)) {
        max += 1;
    }
    max
}

/// `getMaxRectangleLocation(columns)`: the largest rectangle under the histogram (monotonic stack).
pub fn get_max_rectangle_location(columns: &[i32]) -> (IntBounds, i32) {
    let mut max_start = 0i32;
    let mut max_end = 0i32;
    let mut max_height = 0i32;
    let mut stack: Vec<i32> = vec![0];

    for column in 1..=columns.len() as i32 {
        let height = if column as usize == columns.len() {
            0
        } else {
            columns[column as usize]
        };

        while let Some(&top) = stack.last() {
            let stack_height = columns[top as usize];
            if height >= stack_height {
                stack.push(column);
                break;
            }

            stack.pop();
            let start = stack.last().map_or(0, |t| t + 1);
            if stack_height * (column - start) > max_height * (max_end - max_start) {
                max_end = column;
                max_start = start;
                max_height = stack_height;
            }
        }

        if stack.is_empty() {
            stack.push(column);
        }
    }

    (
        IntBounds {
            min: max_start,
            max: max_end - 1,
        },
        max_height,
    )
}

/// `BlockUtil.getTopConnectedBlock(level, pos, bodyBlock, growthDirection, headBlock)`.
pub fn get_top_connected_block(
    level: &dyn BlockGetter,
    pos: BlockPos,
    body_block: Block,
    growth_direction: Direction,
    head_block: Block,
) -> Option<BlockPos> {
    let mut forward_pos = pos;
    loop {
        forward_pos = forward_pos.relative(growth_direction);
        let forward_state = level.get_block_state(forward_pos);
        if !forward_state.is_block(body_block) {
            return forward_state.is_block(head_block).then_some(forward_pos);
        }
    }
}

/// `BlockUtil.clampedPackDifferenceInPosition(pos, testPos, xRadius, yRadius, zRadius)`.
pub fn clamped_pack_difference_in_position(
    pos: BlockPos,
    test_pos: BlockPos,
    x_radius: i32,
    y_radius: i32,
    z_radius: i32,
) -> i32 {
    let clamped_x_radius = mth::clamp_i32(x_radius, 0, 127);
    let clamped_y_radius = mth::clamp_i32(y_radius, 0, 127);
    let clamped_z_radius = mth::clamp_i32(z_radius, 0, 127);
    let x_diff = mth::clamp_i32(test_pos.x().wrapping_sub(pos.x()), -127, 127);
    let y_diff = mth::clamp_i32(test_pos.y().wrapping_sub(pos.y()), -127, 127);
    let z_diff = mth::clamp_i32(test_pos.z().wrapping_sub(pos.z()), -127, 127);
    pack_difference_in_position_internal(
        clamped_x_radius,
        clamped_y_radius,
        clamped_z_radius,
        x_diff,
        y_diff,
        z_diff,
    )
}

/// `BlockUtil.packDifferenceInPosition(pos, testPos, xRadius, yRadius, zRadius)`; `Err` = vanilla's `IllegalArgumentException` message.
pub fn pack_difference_in_position(
    pos: BlockPos,
    test_pos: BlockPos,
    x_radius: i32,
    y_radius: i32,
    z_radius: i32,
) -> Result<i32, String> {
    if x_radius <= 127 && y_radius <= 127 && z_radius <= 127 {
        let x_diff = check_difference_in_bounds(test_pos.x(), pos.x())?;
        let y_diff = check_difference_in_bounds(test_pos.y(), pos.y())?;
        let z_diff = check_difference_in_bounds(test_pos.z(), pos.z())?;
        Ok(pack_difference_in_position_internal(
            x_radius, y_radius, z_radius, x_diff, y_diff, z_diff,
        ))
    } else {
        Err("The radius cannot be larger than 127 when packing the difference between two positions".to_owned())
    }
}

fn pack_difference_in_position_internal(
    x_radius: i32,
    y_radius: i32,
    z_radius: i32,
    x_diff: i32,
    y_diff: i32,
    z_diff: i32,
) -> i32 {
    ((x_diff.wrapping_add(x_radius) & 0xFF) << 16)
        | ((y_diff.wrapping_add(y_radius) & 0xFF) << 8)
        | (z_diff.wrapping_add(z_radius) & 0xFF)
}

fn check_difference_in_bounds(from: i32, to: i32) -> Result<i32, String> {
    let difference = from.wrapping_sub(to);
    if (-127..=127).contains(&difference) {
        Ok(difference)
    } else {
        Err(format!("The difference must be within [{},{}]", 127, -127))
    }
}

/// `BlockUtil.unpackDifferenceInPosition(pos, packedDiff, xRadius, yRadius, zRadius)`.
pub fn unpack_difference_in_position(
    pos: BlockPos,
    packed_diff: i32,
    x_radius: i32,
    y_radius: i32,
    z_radius: i32,
) -> Result<BlockPos, String> {
    if x_radius <= 127 && y_radius <= 127 && z_radius <= 127 {
        let x_diff = ((packed_diff >> 16) & 0xFF) - x_radius;
        let y_diff = ((packed_diff >> 8) & 0xFF) - y_radius;
        let z_diff = (packed_diff & 0xFF) - z_radius;
        Ok(pos.offset(x_diff, y_diff, z_diff))
    } else {
        Err("The radius cannot be larger than 127 when unpacking the difference between two positions.".to_owned())
    }
}
