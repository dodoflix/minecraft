//! `net.minecraft.world.level.block.SelectableSlotContainer.getHitSlot(hitResult, blockFacing)`.
use mc_core::direction::Direction;
use mc_core::phys::BlockHitResult;

/// `getSection(relativeCoordinate, maxSections)`.
fn get_section(relative_coordinate: f32, max_sections: i32) -> i32 {
    let targeted_pixel = relative_coordinate * 16.0;
    let section_size = 16.0 / max_sections as f32;
    ((targeted_pixel / section_size).floor() as i32).clamp(0, max_sections - 1)
}

/// `getRelativeHitCoordinatesForBlockFace(hitResult, blockFacing)`.
fn relative_hit_coordinates(hit: &BlockHitResult, block_facing: Direction) -> Option<(f32, f32)> {
    let hit_direction = hit.direction;
    if block_facing != hit_direction {
        return None;
    }
    let hit_block_pos = hit.block_pos.relative(hit_direction);
    let (rx, ry, rz) = (hit.location.x - f64::from(hit_block_pos.x()), hit.location.y - f64::from(hit_block_pos.y()), hit.location.z - f64::from(hit_block_pos.z()));
    match hit_direction {
        Direction::North => Some(((1.0 - rx) as f32, ry as f32)),
        Direction::South => Some((rx as f32, ry as f32)),
        Direction::West => Some((rz as f32, ry as f32)),
        Direction::East => Some(((1.0 - rz) as f32, ry as f32)),
        Direction::Down | Direction::Up => None,
    }
}

/// `getHitSlot(hitResult, blockFacing)` for a `rows` x `columns` grid.
pub fn get_hit_slot(hit: &BlockHitResult, block_facing: Direction, rows: i32, columns: i32) -> Option<i32> {
    relative_hit_coordinates(hit, block_facing).map(|(x, y)| {
        let row = get_section(1.0 - y, rows);
        let column = get_section(x, columns);
        column + row * columns
    })
}
