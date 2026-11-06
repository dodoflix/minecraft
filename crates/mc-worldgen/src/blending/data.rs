//! `BlendingData` runtime (`levelgen/blending/BlendingData.java`): the persisted/transient state is
//! [`mc_world::blending_data::BlendingData`]; this adds `calculateData`, the cell accessors and the iterators.

use std::sync::LazyLock;

use mc_core::direction::Direction8;
use mc_core::pos::{BlockPos, quart_pos};
use mc_data::block::{Block, BlockState};
use mc_data::tags;
use mc_world::blending_data::{BlendingData, NO_VALUE};
use mc_world::chunk::LevelHeight;
use mc_world::heightmap::HeightmapType;
use mc_world::palette::BiomeId;
use mc_world::status::ChunkStatus;

use super::region::{BlendingChunk, BlendingRegion, ChunkView};

const BLENDING_DENSITY_FACTOR: f32 = 0.1;
const SOLID_DENSITY: f32 = 1.0;
const AIR_DENSITY: f32 = -1.0;
const CELLS_PER_SECTION_Y: i32 = 2;
const QUARTS_PER_SECTION: i32 = quart_pos::from_block(16);
const CELL_HORIZONTAL_MAX_INDEX_INSIDE: i32 = QUARTS_PER_SECTION - 1;
const CELL_HORIZONTAL_MAX_INDEX_OUTSIDE: i32 = QUARTS_PER_SECTION;
const CELL_COLUMN_INSIDE_COUNT: i32 = 2 * CELL_HORIZONTAL_MAX_INDEX_INSIDE + 1;
/// Largest number of sections of an old-generation area `calculateData` accepts; vanilla would run out of memory first.
const MAX_AREA_SECTIONS: i32 = 1 << 16;

/// `BlendingData.SURFACE_BLOCKS`.
const SURFACE_BLOCKS: [Block; 11] = [
    Block::PODZOL,
    Block::GRAVEL,
    Block::GRASS_BLOCK,
    Block::STONE,
    Block::COARSE_DIRT,
    Block::SAND,
    Block::RED_SAND,
    Block::MYCELIUM,
    Block::SNOW_BLOCK,
    Block::TERRACOTTA,
    Block::DIRT,
];

fn block_table(tag: &str) -> Box<[bool]> {
    let set = tags::get::<Block>(tag)
        .unwrap_or_else(|| panic!("vanilla block tag {tag} missing from mc-data"));
    let mut table = vec![false; mc_data::block::BLOCK_COUNT];
    for block in set.iter() {
        table[block.id() as usize] = true;
    }
    table.into_boxed_slice()
}

/// `state.is(BlockTags.LEAVES)`.
pub fn is_leaves(state: BlockState) -> bool {
    static TABLE: LazyLock<Box<[bool]>> = LazyLock::new(|| block_table("leaves"));
    TABLE[state.block().id() as usize]
}

fn is_logs(state: BlockState) -> bool {
    static TABLE: LazyLock<Box<[bool]>> = LazyLock::new(|| block_table("logs"));
    TABLE[state.block().id() as usize]
}

/// `BlendingData.isGround(chunk, pos)`.
fn is_ground(chunk: &dyn ChunkView, pos: BlockPos) -> bool {
    let state = chunk.get_block_state(pos);
    let block = state.block();
    if state.is_air()
        || is_leaves(state)
        || is_logs(state)
        || block == Block::BROWN_MUSHROOM_BLOCK
        || block == Block::RED_MUSHROOM_BLOCK
    {
        false
    } else {
        !state.collision_shape().is_empty()
    }
}

/// `Mth.floor(float)`.
fn floor_f32(v: f32) -> i32 {
    let i = v as i32;
    if v < i as f32 { i.wrapping_sub(1) } else { i }
}

/// `getInsideIndex(x, z)`.
fn get_inside_index(x: i32, z: i32) -> usize {
    (CELL_HORIZONTAL_MAX_INDEX_INSIDE - x + z) as usize
}

/// `getOutsideIndex(x, z)`.
fn get_outside_index(x: i32, z: i32) -> usize {
    (CELL_COLUMN_INSIDE_COUNT + x + CELL_HORIZONTAL_MAX_INDEX_OUTSIDE - z) as usize
}

fn zero_if_negative(value: i32) -> i32 {
    value & !(value >> 31)
}

/// `getX(index)`.
fn get_x(index: usize) -> i32 {
    let index = index as i32;
    if index < CELL_COLUMN_INSIDE_COUNT {
        zero_if_negative(CELL_HORIZONTAL_MAX_INDEX_INSIDE - index)
    } else {
        let offset_index = index - CELL_COLUMN_INSIDE_COUNT;
        CELL_HORIZONTAL_MAX_INDEX_OUTSIDE
            - zero_if_negative(CELL_HORIZONTAL_MAX_INDEX_OUTSIDE - offset_index)
    }
}

/// `getZ(index)`.
fn get_z(index: usize) -> i32 {
    let index = index as i32;
    if index < CELL_COLUMN_INSIDE_COUNT {
        zero_if_negative(index - CELL_HORIZONTAL_MAX_INDEX_INSIDE)
    } else {
        let offset_index = index - CELL_COLUMN_INSIDE_COUNT;
        CELL_HORIZONTAL_MAX_INDEX_OUTSIDE
            - zero_if_negative(offset_index - CELL_HORIZONTAL_MAX_INDEX_OUTSIDE)
    }
}

/// `LevelHeightAccessor.create(minY, height)` sizes with Java's wrapping `int` arithmetic.
struct Area {
    min_y: i32,
    max_y: i32,
}

impl Area {
    fn of(data: &BlendingData) -> Area {
        Area {
            min_y: data.min_y(),
            max_y: data.min_y().wrapping_add(data.height()).wrapping_sub(1),
        }
    }

    fn min_section_y(&self) -> i32 {
        self.min_y >> 4
    }

    fn sections_count(&self) -> i32 {
        (self.max_y >> 4)
            .wrapping_sub(self.min_section_y())
            .wrapping_add(1)
    }

    /// `cellCountPerColumn()`.
    fn cell_count_per_column(&self) -> i32 {
        self.sections_count().wrapping_mul(CELLS_PER_SECTION_Y)
    }

    /// `quartCountPerColumn()`.
    fn quart_count_per_column(&self) -> i32 {
        quart_pos::from_section(self.sections_count())
    }

    /// `getMinY()` of the cell grid.
    fn cell_min_y(&self) -> i32 {
        self.min_section_y().wrapping_mul(2)
    }

    /// `getColumnMinY()`.
    fn column_min_y(&self) -> i32 {
        self.cell_min_y().wrapping_add(1)
    }

    /// `getCellYIndex(cellY)`.
    fn cell_y_index(&self, cell_y: i32) -> i32 {
        cell_y.wrapping_sub(self.column_min_y())
    }
}

/// `read1`: moves `y` down one block and reports whether it is ground.
fn read1(chunk: &dyn ChunkView, x: i32, y: &mut i32, z: i32) -> f32 {
    *y = y.wrapping_sub(1);
    if is_ground(chunk, BlockPos::new(x, *y, z)) {
        SOLID_DENSITY
    } else {
        AIR_DENSITY
    }
}

/// `read7`.
fn read7(chunk: &dyn ChunkView, x: i32, y: &mut i32, z: i32) -> f32 {
    let mut sum = 0.0f32;
    for _ in 0..7 {
        sum += read1(chunk, x, y, z);
    }
    sum
}

/// `CompositeDirection.Direction8` set (`EnumSet`).
fn contains(sides: &[Direction8], side: Direction8) -> bool {
    sides.contains(&side)
}

/// `BlendingData.sideByGenerationAge(region, chunkX, chunkZ, wantedOldGen)`.
pub fn side_by_generation_age(
    region: &dyn BlendingRegion,
    chunk_x: i32,
    chunk_z: i32,
    wanted_old_gen: bool,
) -> Vec<Direction8> {
    use mc_core::direction::Directional;
    let mut sides = Vec::new();
    for direction8 in Direction8::ALL {
        let old = region_is_old(
            region,
            chunk_x + direction8.get_step_x(),
            chunk_z + direction8.get_step_z(),
        );
        if old == wanted_old_gen {
            sides.push(direction8);
        }
    }
    sides
}

fn region_is_old(region: &dyn BlendingRegion, chunk_x: i32, chunk_z: i32) -> bool {
    let mut result = false;
    region.with_chunk(chunk_x, chunk_z, &mut |chunk| {
        result = chunk.is_old_noise_generation()
    });
    result
}

/// `BlendingData.getOrUpdateBlendingData(region, chunkX, chunkZ)`: the chunk's blending data after `calculateData`
/// (a snapshot: the live data is updated in the chunk), or `None` for chunks that are not old or have no biomes yet.
pub fn get_or_update_blending_data(
    region: &dyn BlendingRegion,
    chunk_x: i32,
    chunk_z: i32,
) -> Option<BlendingData> {
    let mut eligible = false;
    region.with_chunk(chunk_x, chunk_z, &mut |chunk: &mut dyn BlendingChunk| {
        eligible = chunk.blending_data().is_some()
            && !chunk
                .get_highest_generated_status()
                .is_before(ChunkStatus::Biomes);
    });
    if !eligible {
        return None;
    }
    let sides = side_by_generation_age(region, chunk_x, chunk_z, false);
    let mut result = None;
    region.with_chunk(chunk_x, chunk_z, &mut |chunk: &mut dyn BlendingChunk| {
        chunk.update_blending_data(&mut |data, view| data.calculate_data(view, &sides));
        result = chunk.blending_data().cloned();
    });
    result
}

/// Runtime half of `BlendingData`.
pub trait BlendingDataExt {
    /// `getAreaWithOldGeneration()`.
    fn get_area_with_old_generation(&self) -> LevelHeight;
    /// `calculateData(chunk, newSides)`.
    fn calculate_data(&mut self, chunk: &dyn ChunkView, new_sides: &[Direction8]);
    /// `getHeight(cellX, cellY, cellZ)`: [`NO_VALUE`] when unknown.
    fn get_height(&self, cell_x: i32, cell_y: i32, cell_z: i32) -> f32;
    /// `getDensity(cellX, cellY, cellZ)`: [`NO_VALUE`] when unknown.
    fn get_density(&self, cell_x: i32, cell_y: i32, cell_z: i32) -> f32;
    /// `iterateBiomes(minCellX, quartY, minCellZ, consumer)`: `consumer(cellX, cellZ, biome)`.
    fn iterate_biomes(
        &self,
        min_cell_x: i32,
        quart_y: i32,
        min_cell_z: i32,
        consumer: &mut dyn FnMut(i32, i32, BiomeId),
    );
    /// `iterateHeights(minCellX, minCellZ, consumer)`: `consumer(cellX, cellZ, height)`.
    fn iterate_heights(
        &self,
        min_cell_x: i32,
        min_cell_z: i32,
        consumer: &mut dyn FnMut(i32, i32, f32),
    );
    /// `iterateDensities(minCellX, minCellZ, fromCellY, toCellY, consumer)`: `consumer(cellX, cellY, cellZ, density)`.
    fn iterate_densities(
        &self,
        min_cell_x: i32,
        min_cell_z: i32,
        from_cell_y: i32,
        to_cell_y: i32,
        consumer: &mut dyn FnMut(i32, i32, i32, f32),
    );
}

/// `getDensity(float[] densityColumn, cellY)`.
fn column_density(area: &Area, column: &Option<Vec<f32>>, cell_y: i32) -> f32 {
    match column {
        None => NO_VALUE,
        Some(column) => {
            let y_index = area.cell_y_index(cell_y);
            if y_index >= 0 && (y_index as usize) < column.len() {
                column[y_index as usize] * BLENDING_DENSITY_FACTOR
            } else {
                NO_VALUE
            }
        }
    }
}

impl BlendingDataExt for BlendingData {
    fn get_area_with_old_generation(&self) -> LevelHeight {
        LevelHeight::new(self.min_y(), self.height())
    }

    fn calculate_data(&mut self, chunk: &dyn ChunkView, new_sides: &[Direction8]) {
        if self.has_calculated_data {
            return;
        }
        let area = Area::of(self);
        if !(0..=MAX_AREA_SECTIONS).contains(&area.sections_count()) {
            // vanilla: NegativeArraySizeException / out of memory for a corrupt `min_section`/`max_section`.
            return;
        }
        use Direction8::*;
        if contains(new_sides, North) || contains(new_sides, West) || contains(new_sides, NorthWest)
        {
            self.add_values_for_column(get_inside_index(0, 0), chunk, 0, 0);
        }
        if contains(new_sides, North) {
            for i in 1..QUARTS_PER_SECTION {
                self.add_values_for_column(get_inside_index(i, 0), chunk, 4 * i, 0);
            }
        }
        if contains(new_sides, West) {
            for i in 1..QUARTS_PER_SECTION {
                self.add_values_for_column(get_inside_index(0, i), chunk, 0, 4 * i);
            }
        }
        if contains(new_sides, East) {
            for i in 1..QUARTS_PER_SECTION {
                self.add_values_for_column(
                    get_outside_index(CELL_HORIZONTAL_MAX_INDEX_OUTSIDE, i),
                    chunk,
                    15,
                    4 * i,
                );
            }
        }
        if contains(new_sides, South) {
            for i in 0..QUARTS_PER_SECTION {
                self.add_values_for_column(
                    get_outside_index(i, CELL_HORIZONTAL_MAX_INDEX_OUTSIDE),
                    chunk,
                    4 * i,
                    15,
                );
            }
        }
        if contains(new_sides, East) && contains(new_sides, NorthEast) {
            self.add_values_for_column(
                get_outside_index(CELL_HORIZONTAL_MAX_INDEX_OUTSIDE, 0),
                chunk,
                15,
                0,
            );
        }
        if contains(new_sides, East) && contains(new_sides, South) && contains(new_sides, SouthEast)
        {
            self.add_values_for_column(
                get_outside_index(
                    CELL_HORIZONTAL_MAX_INDEX_OUTSIDE,
                    CELL_HORIZONTAL_MAX_INDEX_OUTSIDE,
                ),
                chunk,
                15,
                15,
            );
        }
        self.has_calculated_data = true;
    }

    fn get_height(&self, cell_x: i32, _cell_y: i32, cell_z: i32) -> f32 {
        if cell_x == CELL_HORIZONTAL_MAX_INDEX_OUTSIDE
            || cell_z == CELL_HORIZONTAL_MAX_INDEX_OUTSIDE
        {
            self.heights[get_outside_index(cell_x, cell_z)]
        } else if cell_x != 0 && cell_z != 0 {
            NO_VALUE
        } else {
            self.heights[get_inside_index(cell_x, cell_z)]
        }
    }

    fn get_density(&self, cell_x: i32, cell_y: i32, cell_z: i32) -> f32 {
        let area = Area::of(self);
        if cell_y == area.cell_min_y() {
            0.1
        } else if cell_x == CELL_HORIZONTAL_MAX_INDEX_OUTSIDE
            || cell_z == CELL_HORIZONTAL_MAX_INDEX_OUTSIDE
        {
            column_density(
                &area,
                &self.densities[get_outside_index(cell_x, cell_z)],
                cell_y,
            )
        } else if cell_x != 0 && cell_z != 0 {
            NO_VALUE
        } else {
            column_density(
                &area,
                &self.densities[get_inside_index(cell_x, cell_z)],
                cell_y,
            )
        }
    }

    fn iterate_biomes(
        &self,
        min_cell_x: i32,
        quart_y: i32,
        min_cell_z: i32,
        consumer: &mut dyn FnMut(i32, i32, BiomeId),
    ) {
        let area = Area::of(self);
        let quart_min_y = quart_pos::from_block(area.min_y);
        if quart_y >= quart_min_y && quart_y <= quart_pos::from_block(area.max_y) {
            let quart_index = (quart_y - quart_min_y) as usize;
            for (i, cell) in self.biomes.iter().enumerate() {
                if let Some(biome) = cell.as_ref().and_then(|cell| cell.get(quart_index)) {
                    consumer(min_cell_x + get_x(i), min_cell_z + get_z(i), *biome);
                }
            }
        }
    }

    fn iterate_heights(
        &self,
        min_cell_x: i32,
        min_cell_z: i32,
        consumer: &mut dyn FnMut(i32, i32, f32),
    ) {
        for (i, &value) in self.heights.iter().enumerate() {
            if value != NO_VALUE {
                consumer(min_cell_x + get_x(i), min_cell_z + get_z(i), value);
            }
        }
    }

    fn iterate_densities(
        &self,
        min_cell_x: i32,
        min_cell_z: i32,
        from_cell_y: i32,
        to_cell_y: i32,
        consumer: &mut dyn FnMut(i32, i32, i32, f32),
    ) {
        let area = Area::of(self);
        let min_cell_y = area.column_min_y();
        let min_y_index = 0.max(from_cell_y.wrapping_sub(min_cell_y));
        let max_y_index = area
            .cell_count_per_column()
            .min(to_cell_y.wrapping_sub(min_cell_y));
        for (i, column) in self.densities.iter().enumerate() {
            if let Some(column) = column {
                let test_cell_x = min_cell_x + get_x(i);
                let test_cell_z = min_cell_z + get_z(i);
                for y_index in min_y_index..max_y_index {
                    consumer(
                        test_cell_x,
                        y_index + min_cell_y,
                        test_cell_z,
                        column[y_index as usize] * BLENDING_DENSITY_FACTOR,
                    );
                }
            }
        }
    }
}

/// Private half of the runtime (needs the columns' construction).
trait CalculateColumns {
    fn add_values_for_column(
        &mut self,
        index: usize,
        chunk: &dyn ChunkView,
        block_x: i32,
        block_z: i32,
    );
    fn get_height_at_xz(&self, chunk: &dyn ChunkView, block_x: i32, block_z: i32) -> i32;
    fn get_density_column(&self, chunk: &dyn ChunkView, x: i32, z: i32, height: i32) -> Vec<f32>;
    fn get_biome_column(&self, chunk: &dyn ChunkView, block_x: i32, block_z: i32) -> Vec<BiomeId>;
}

impl CalculateColumns for BlendingData {
    fn add_values_for_column(
        &mut self,
        index: usize,
        chunk: &dyn ChunkView,
        block_x: i32,
        block_z: i32,
    ) {
        if self.heights[index] == NO_VALUE {
            self.heights[index] = self.get_height_at_xz(chunk, block_x, block_z) as f32;
        }
        self.densities[index] =
            Some(self.get_density_column(chunk, block_x, block_z, floor_f32(self.heights[index])));
        self.biomes[index] = Some(self.get_biome_column(chunk, block_x, block_z));
    }

    fn get_height_at_xz(&self, chunk: &dyn ChunkView, block_x: i32, block_z: i32) -> i32 {
        let area = Area::of(self);
        let mut y = if chunk.has_primed_heightmap(HeightmapType::WorldSurfaceWg) {
            chunk
                .get_height(HeightmapType::WorldSurfaceWg, block_x, block_z)
                .min(area.max_y)
        } else {
            area.max_y
        };
        while y > area.min_y {
            if SURFACE_BLOCKS.contains(
                &chunk
                    .get_block_state(BlockPos::new(block_x, y, block_z))
                    .block(),
            ) {
                return y;
            }
            y -= 1;
        }
        area.min_y
    }

    fn get_density_column(&self, chunk: &dyn ChunkView, x: i32, z: i32, height: i32) -> Vec<f32> {
        let area = Area::of(self);
        let mut densities = vec![-1.0f32; area.cell_count_per_column().max(0) as usize];
        let mut y = area.max_y.wrapping_add(1);
        let mut last7 = read7(chunk, x, &mut y, z);
        let mut cell_index = densities.len() as i64 - 2;
        while cell_index >= 0 {
            let one = read1(chunk, x, &mut y, z);
            let current7 = read7(chunk, x, &mut y, z);
            densities[cell_index as usize] = (last7 + one + current7) / 15.0;
            last7 = current7;
            cell_index -= 1;
        }
        let highest_cell_with_surface_index =
            area.cell_y_index(mc_core::math::mth::floor_div(height, 8));
        if highest_cell_with_surface_index >= 0
            && (highest_cell_with_surface_index as i64) < densities.len() as i64 - 1
        {
            let i = highest_cell_with_surface_index as usize;
            let in_cell_index = (height as f32 + 0.5) % 8.0 / 8.0;
            let amplitude_above_to_make_surface_be_at_height =
                (1.0 - in_cell_index) / in_cell_index;
            let max = java_max_f32(amplitude_above_to_make_surface_be_at_height, 1.0) * 0.25;
            densities[i + 1] = -amplitude_above_to_make_surface_be_at_height / max;
            densities[i] = 1.0 / max;
        }
        densities
    }

    fn get_biome_column(&self, chunk: &dyn ChunkView, block_x: i32, block_z: i32) -> Vec<BiomeId> {
        let area = Area::of(self);
        let count = area.quart_count_per_column().max(0);
        (0..count)
            .map(|quart_index| {
                let quart_y = quart_index + quart_pos::from_block(area.min_y);
                chunk.get_noise_biome(
                    quart_pos::from_block(block_x),
                    quart_y,
                    quart_pos::from_block(block_z),
                )
            })
            .collect()
    }
}

/// `Math.max(float, float)`.
fn java_max_f32(a: f32, b: f32) -> f32 {
    if a.is_nan() || b.is_nan() {
        f32::NAN
    } else {
        a.max(b)
    }
}
