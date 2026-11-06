//! `StructureUtils`.

use crate::commands::arguments::blocks::BlockInput;
use crate::server::MinecraftServer;
use crate::server::level::ServerLevel;
use crate::world::block_entity::test_instance_block_entity::{
    self, Data, Status, TestInstanceView, structure_pos_of,
};
use crate::world::level::{DimensionKey, LevelAccessor};
use mc_core::bounding_box::BoundingBox;
use mc_core::ident::Identifier;
use mc_core::mirror::Mirror;
use mc_core::phys::AABB;
use mc_core::pos::{BlockPos, Vec3i};
use mc_core::rotation::Rotation;
use mc_data::BlockEntityType;
use mc_data::block::Block;

/// `DEFAULT_Y_SEARCH_RADIUS`.
pub const DEFAULT_Y_SEARCH_RADIUS: i32 = 10;

/// `getRotationForRotationSteps(rotationSteps)`; vanilla throws `IllegalArgumentException` (a command handler panic
/// is reported as `command.failed`).
pub fn get_rotation_for_rotation_steps(rotation_steps: i32) -> Rotation {
    match rotation_steps {
        0 => Rotation::None,
        1 => Rotation::Clockwise90,
        2 => Rotation::Clockwise180,
        3 => Rotation::Counterclockwise90,
        _ => panic!("rotationSteps must be a value from 0-3. Got value {rotation_steps}"),
    }
}

/// `getRotationStepsForRotation(rotation)`.
pub fn get_rotation_steps_for_rotation(rotation: Rotation) -> i32 {
    match rotation {
        Rotation::None => 0,
        Rotation::Clockwise90 => 1,
        Rotation::Clockwise180 => 2,
        Rotation::Counterclockwise90 => 3,
    }
}

/// `createNewEmptyTest(id, structurePos, size, rotation, level)`: places the test instance block and returns its data.
pub fn create_new_empty_test(
    server: &mut MinecraftServer,
    dimension: &DimensionKey,
    id: &Identifier,
    structure_pos: BlockPos,
    size: Vec3i,
    rotation: Rotation,
) {
    let bounding_box = get_structure_bounding_box(structure_pos_of(structure_pos), size, rotation);
    let Some(level) = server.level_mut(dimension) else {
        return;
    };
    clear_space_for_structure(level, &bounding_box);
    level.set_block_and_update(structure_pos, Block::TEST_INSTANCE_BLOCK.default_state());
    test_instance_block_entity::set(
        server,
        dimension,
        structure_pos,
        Data {
            test: Some(id.clone()),
            size,
            rotation,
            ignore_entities: false,
            status: Status::Cleared,
            error_message: None,
        },
    );
}

/// `clearSpaceForStructure(structureBoundingBox, level)`.
pub fn clear_space_for_structure(level: &mut ServerLevel, structure_bounding_box: &BoundingBox) {
    let ground_height = structure_bounding_box.min_y() - 1;
    for pos in BlockPos::between_closed_box(structure_bounding_box) {
        clear_block(ground_height, pos, level);
    }
    clear_block_ticks_and_events(level, structure_bounding_box);
    discard_non_player_entities(
        level,
        &test_instance_block_entity::aabb_of(structure_bounding_box),
    );
}

/// `level.getBlockTicks().clearArea(box)` + `level.clearBlockEvents(box)`.
pub fn clear_block_ticks_and_events(level: &mut ServerLevel, bounding_box: &BoundingBox) {
    level.tick_state.ticks.lock().block.clear_area(bounding_box);
    level.clear_block_events(bounding_box);
}

/// `level.getEntitiesOfClass(Entity.class, bounds, mob -> !(mob instanceof Player)).forEach(Entity::discard)`.
pub fn discard_non_player_entities(level: &mut ServerLevel, bounds: &AABB) {
    let ids = level.get_entities(None, bounds, |e| !e.is_player());
    for id in ids {
        level.with_entity(id, |_, entity| entity.discard());
    }
}

/// `getTransformedFarCorner(structurePosition, size, rotation)`.
pub fn get_transformed_far_corner(
    structure_position: BlockPos,
    size: Vec3i,
    rotation: Rotation,
) -> BlockPos {
    let far_corner_before_transform = structure_position.offset_vec(&size).offset(-1, -1, -1);
    crate::world::level::levelgen::structure::templatesystem::structure_template::transform(
        far_corner_before_transform,
        Mirror::None,
        rotation,
        structure_position,
    )
}

/// `getStructureBoundingBox(northWestCorner, size, rotation)`.
pub fn get_structure_bounding_box(
    north_west_corner: BlockPos,
    size: Vec3i,
    rotation: Rotation,
) -> BoundingBox {
    let far_corner = get_transformed_far_corner(north_west_corner, size, rotation);
    let bounding_box = BoundingBox::from_corners(&north_west_corner, &far_corner);
    let current_north_west_corner_x = bounding_box.min_x().min(bounding_box.max_x());
    let current_north_west_corner_z = bounding_box.min_z().min(bounding_box.max_z());
    bounding_box.moved(
        north_west_corner.x() - current_north_west_corner_x,
        0,
        north_west_corner.z() - current_north_west_corner_z,
    )
}

/// `findTestContainingPos(pos, searchRadius, level)`.
pub fn find_test_containing_pos(
    server: &MinecraftServer,
    dimension: &DimensionKey,
    pos: BlockPos,
    search_radius: i32,
) -> Option<BlockPos> {
    find_test_blocks(server, dimension, pos, search_radius)
        .into_iter()
        .find(|test_block_pos_to_check| {
            does_structure_contain(server, dimension, *test_block_pos_to_check, pos)
        })
}

/// `findNearestTest(relativeToPos, searchRadius, level)`: the first of the closest (`Stream.min` keeps the first).
pub fn find_nearest_test(
    server: &MinecraftServer,
    dimension: &DimensionKey,
    relative_to_pos: BlockPos,
    search_radius: i32,
) -> Option<BlockPos> {
    let mut best: Option<(i32, BlockPos)> = None;
    for pos in find_test_blocks(server, dimension, relative_to_pos, search_radius) {
        let distance = pos.dist_manhattan(&relative_to_pos);
        if best.is_none_or(|(d, _)| distance < d) {
            best = Some((distance, pos));
        }
    }
    best.map(|(_, pos)| pos)
}

/// `findTestBlocks(centerPos, searchRadius, level)`: `PoiManager.findAll(TEST_INSTANCE, any, center, radius, ANY)`, i.e.
/// the test instance blocks inside the square of `radius` around the center and within `radius` (euclidean).
/// The point-of-interest index of vanilla is the set of chunk block entities of the test instance block type here (every
/// test instance block has one); chunks are visited in `ChunkPos.rangeClosed` order (x fastest) and only loaded chunks are
/// searched (vanilla's POI manager also finds unloaded-but-saved chunks).
pub fn find_test_blocks(
    server: &MinecraftServer,
    dimension: &DimensionKey,
    center: BlockPos,
    search_radius: i32,
) -> Vec<BlockPos> {
    let Some(level) = server.level(dimension) else {
        return Vec::new();
    };
    let chunk_radius = search_radius.div_euclid(16) + 1;
    let (center_chunk_x, center_chunk_z) = (center.x() >> 4, center.z() >> 4);
    let radius_sqr = f64::from(search_radius.wrapping_mul(search_radius));
    let mut found = Vec::new();
    for chunk_z in (center_chunk_z - chunk_radius)..=(center_chunk_z + chunk_radius) {
        for chunk_x in (center_chunk_x - chunk_radius)..=(center_chunk_x + chunk_radius) {
            let Some(chunk) = level.chunk_now(chunk_x, chunk_z) else {
                continue;
            };
            let mut in_chunk: Vec<BlockPos> = chunk
                .block_entities
                .iter()
                .filter(|(_, be)| {
                    be.get_type() == BlockEntityType::TEST_INSTANCE_BLOCK && !be.is_removed()
                })
                .map(|(pos, _)| *pos)
                .filter(|pos| {
                    (pos.x() - center.x()).abs() <= search_radius
                        && (pos.z() - center.z()).abs() <= search_radius
                        && pos.dist_sqr(&center) <= radius_sqr
                })
                .collect();
            in_chunk.sort_by_key(|p| (p.y(), p.z(), p.x()));
            found.extend(in_chunk);
        }
    }
    found
}

/// `lookedAtTestPos(pos, camera, level)`: the closest test whose structure the view ray hits.
pub fn looked_at_test_pos(
    server: &MinecraftServer,
    dimension: &DimensionKey,
    pos: BlockPos,
    eye: mc_core::phys::Vec3,
    look: mc_core::phys::Vec3,
) -> Vec<BlockPos> {
    let end = eye.add(look.scale(250.0));
    let mut hits: Vec<BlockPos> = find_test_blocks(server, dimension, pos, 250)
        .into_iter()
        .filter(|block_pos| {
            TestInstanceView::read(server, dimension, *block_pos)
                .is_some_and(|view| view.get_structure_bounds().clip(eye, end).is_some())
        })
        .collect();
    // `sorted(Comparator.comparing(pos::distSqr))` is stable; `limit(1)` keeps the first
    hits.sort_by(|a, b| {
        pos.dist_sqr(a)
            .partial_cmp(&pos.dist_sqr(b))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    hits.truncate(1);
    hits
}

/// `clearBlock(airIfAboveThisY, pos, level)`.
fn clear_block(air_if_above_this_y: i32, pos: BlockPos, level: &mut ServerLevel) {
    let block_state = if pos.y() < air_if_above_this_y {
        Block::STONE.default_state()
    } else {
        Block::AIR.default_state()
    };
    BlockInput::new(block_state, Vec::new(), None).place(level, pos, 818);
    level.update_neighbors_at(pos, block_state.block());
}

/// `doesStructureContain(testInstanceBlockPos, pos, level)`.
fn does_structure_contain(
    server: &MinecraftServer,
    dimension: &DimensionKey,
    test_instance_block_pos: BlockPos,
    pos: BlockPos,
) -> bool {
    TestInstanceView::read(server, dimension, test_instance_block_pos)
        .is_some_and(|view| view.get_structure_bounding_box().is_inside(&pos))
}
