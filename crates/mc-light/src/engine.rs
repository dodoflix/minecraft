//! `LightEngine` (queue semantics shared by both layers) with `BlockLightEngine` and `SkyLightEngine` behavior.

use crate::chunk_source::{LightChunk, LightChunkGetter};
use crate::long_set::LongOpenHashSet;
use crate::storage::{LayerLightSectionStorage, LayerReader, LightLayer, SectionType};
use mc_core::direction::Direction;
use mc_core::pos::{BlockPos, ChunkPos, SectionPos};
use mc_data::block::{Block, BlockState};
use mc_world::chunk::LevelHeight;
use mc_world::data_layer::DataLayer;
use mc_world::sky_light_sources::{ChunkSkyLightSources, get_occlusion_shape, is_empty_shape};
use mc_core::phys::shapes::Shapes;
use std::collections::VecDeque;
use std::sync::Arc;

pub const MAX_LEVEL: i32 = 15;
const MIN_QUEUE_SIZE: usize = 512;

/// `LightEngine.QueueEntry`: packed propagation data (level, per-direction bits, flags).
pub mod queue_entry {
    use mc_core::direction::Direction;

    const LEVEL_MASK: i64 = 15;
    const DIRECTIONS_MASK: i64 = 1008;
    const FLAG_FROM_EMPTY_SHAPE: i64 = 1024;
    const FLAG_INCREASE_FROM_EMISSION: i64 = 2048;

    pub fn decrease_skip_one_direction(old_from_level: i32, skip_direction: Direction) -> i64 {
        with_level(without_direction(DIRECTIONS_MASK, skip_direction), old_from_level)
    }

    pub fn decrease_all_directions(old_from_level: i32) -> i64 {
        with_level(DIRECTIONS_MASK, old_from_level)
    }

    pub fn increase_light_from_emission(new_from_level: i32, from_empty_shape: bool) -> i64 {
        let mut data = DIRECTIONS_MASK | FLAG_INCREASE_FROM_EMISSION;
        if from_empty_shape {
            data |= FLAG_FROM_EMPTY_SHAPE;
        }
        with_level(data, new_from_level)
    }

    pub fn increase_skip_one_direction(new_from_level: i32, from_empty_shape: bool, skip_direction: Direction) -> i64 {
        let mut data = without_direction(DIRECTIONS_MASK, skip_direction);
        if from_empty_shape {
            data |= FLAG_FROM_EMPTY_SHAPE;
        }
        with_level(data, new_from_level)
    }

    pub fn increase_only_one_direction(new_from_level: i32, from_empty_shape: bool, direction: Direction) -> i64 {
        let mut data = 0;
        if from_empty_shape {
            data |= FLAG_FROM_EMPTY_SHAPE;
        }
        with_level(with_direction(data, direction), new_from_level)
    }

    pub fn increase_sky_source_in_directions(down: bool, north: bool, south: bool, west: bool, east: bool) -> i64 {
        let mut data = with_level(0, 15);
        for (flag, direction) in [(down, Direction::Down), (north, Direction::North), (south, Direction::South), (west, Direction::West), (east, Direction::East)] {
            if flag {
                data = with_direction(data, direction);
            }
        }
        data
    }

    pub fn get_from_level(entry: i64) -> i32 {
        (entry & LEVEL_MASK) as i32
    }

    pub fn is_from_empty_shape(entry: i64) -> bool {
        entry & FLAG_FROM_EMPTY_SHAPE != 0
    }

    pub fn is_increase_from_emission(entry: i64) -> bool {
        entry & FLAG_INCREASE_FROM_EMISSION != 0
    }

    pub fn should_propagate_in_direction(entry: i64, direction: Direction) -> bool {
        entry & (1i64 << (direction.ordinal() + 4)) != 0
    }

    fn with_level(entry: i64, level: i32) -> i64 {
        entry & -16 | i64::from(level) & 15
    }

    fn with_direction(entry: i64, direction: Direction) -> i64 {
        entry | 1i64 << (direction.ordinal() + 4)
    }

    fn without_direction(entry: i64, direction: Direction) -> i64 {
        entry & !(1i64 << (direction.ordinal() + 4))
    }
}

use queue_entry as qe;

const PULL_LIGHT_IN_ENTRY: i64 = 1008 | 1; // decreaseAllDirections(1)
const REMOVE_TOP_SKY_SOURCE_ENTRY: i64 = 1008 | 15; // decreaseAllDirections(15)

fn remove_sky_source_entry() -> i64 {
    qe::decrease_skip_one_direction(15, Direction::Up)
}

fn add_sky_source_entry() -> i64 {
    qe::increase_skip_one_direction(15, false, Direction::Up)
}

/// `LightEngine.hasDifferentLightProperties`.
pub use mc_world::chunk::has_different_light_properties;

/// `LightEngine.getLightDampeningInto(fromState, toState, direction, simpleOpacity)`.
pub fn get_light_dampening_into(from_state: BlockState, to_state: BlockState, direction: Direction, simple_opacity: i32) -> i32 {
    let from_empty = is_empty_shape(from_state);
    let to_empty = is_empty_shape(to_state);
    if from_empty && to_empty {
        return simple_opacity;
    }
    let from_shape = if from_empty { Shapes::empty() } else { from_state.occlusion_voxel_shape() };
    let to_shape = if to_empty { Shapes::empty() } else { to_state.occlusion_voxel_shape() };
    if Shapes::merged_face_occludes(&from_shape, &to_shape, direction) { 16 } else { simple_opacity }
}

/// `LightEngine.getOpacity`.
fn get_opacity(state: BlockState) -> i32 {
    i32::from(state.get_light_block()).max(1)
}

/// `LightEngine.shapeOccludes`.
fn shape_occludes(from_state: BlockState, to_state: BlockState, direction: Direction) -> bool {
    if is_empty_shape(from_state) && is_empty_shape(to_state) {
        return false; // faceShapeOccludes(empty, empty)
    }
    let from_shape = get_occlusion_shape(from_state, direction);
    let to_shape = get_occlusion_shape(to_state, direction.get_opposite());
    Shapes::face_shape_occludes(&from_shape, &to_shape)
}

/// The two-entry chunk cache of `LightEngine.getChunk`, valid for one `runLightUpdates`.
struct ChunkCache<'a> {
    getter: &'a dyn LightChunkGetter,
    pos: [i64; 2],
    chunk: [Option<&'a dyn LightChunk>; 2],
}

impl<'a> ChunkCache<'a> {
    fn new(getter: &'a dyn LightChunkGetter) -> Self {
        ChunkCache { getter, pos: [ChunkPos::INVALID_CHUNK_POS; 2], chunk: [None; 2] }
    }

    fn get_chunk(&mut self, chunk_x: i32, chunk_z: i32) -> Option<&'a dyn LightChunk> {
        let pos = ChunkPos::pack_xz(chunk_x, chunk_z);
        for i in 0..2 {
            if pos == self.pos[i] {
                return self.chunk[i];
            }
        }
        let chunk = self.getter.get_chunk_for_lighting(chunk_x, chunk_z);
        self.pos[1] = self.pos[0];
        self.chunk[1] = self.chunk[0];
        self.pos[0] = pos;
        self.chunk[0] = chunk;
        chunk
    }

    /// `getState(pos)`: bedrock where no chunk is available.
    fn get_state(&mut self, node: i64) -> BlockState {
        let pos = BlockPos::of_long(node);
        match self.get_chunk(SectionPos::block_to_section_coord_i32(pos.x), SectionPos::block_to_section_coord_i32(pos.z)) {
            Some(chunk) => chunk.get_block_state(pos),
            None => Block::BEDROCK.default_state(),
        }
    }
}

fn enqueue(queue: &mut VecDeque<i64>, node: i64, data: i64) {
    queue.push_back(node);
    queue.push_back(data);
}

/// `BlockLightEngine` / `SkyLightEngine` (selected by `layer`).
pub struct LightEngine {
    layer: LightLayer,
    pub(crate) storage: LayerLightSectionStorage,
    block_nodes_to_check: LongOpenHashSet,
    decrease_queue: VecDeque<i64>,
    increase_queue: VecDeque<i64>,
    /// Sky: `emptyChunkSources`.
    empty_chunk_sources: ChunkSkyLightSources,
    light_updates: Vec<i64>,
}

impl LightEngine {
    pub fn new(layer: LightLayer, level_height: LevelHeight) -> Self {
        LightEngine {
            layer,
            storage: LayerLightSectionStorage::new(layer),
            block_nodes_to_check: LongOpenHashSet::with_capacity(MIN_QUEUE_SIZE, 0.5),
            decrease_queue: VecDeque::new(),
            increase_queue: VecDeque::new(),
            empty_chunk_sources: ChunkSkyLightSources::new(level_height.min_y, level_height.height),
            light_updates: Vec::new(),
        }
    }

    pub fn layer(&self) -> LightLayer {
        self.layer
    }

    pub fn reader(&self) -> LayerReader {
        self.storage.reader()
    }

    /// `checkBlock(pos)`.
    pub fn check_block(&mut self, pos: BlockPos) {
        self.block_nodes_to_check.add(pos.as_long());
    }

    /// `queueSectionData(sectionNode, data)`.
    pub fn queue_section_data(&mut self, section: SectionPos, data: Option<DataLayer>) {
        self.storage.queue_section_data(section.as_long(), data);
    }

    /// `retainData(chunkPos, retain)`.
    pub fn retain_data(&mut self, pos: ChunkPos, retain: bool) {
        self.storage.retain_data(SectionPos::get_zero_node_xz(pos.x, pos.z), retain);
    }

    /// `updateSectionStatus(pos, sectionEmpty)`.
    pub fn update_section_status(&mut self, pos: SectionPos, section_empty: bool) {
        self.storage.update_section_status(pos.as_long(), section_empty);
    }

    pub fn has_light_work(&self) -> bool {
        self.storage.has_inconsistencies() || !self.block_nodes_to_check.is_empty() || !self.decrease_queue.is_empty() || !self.increase_queue.is_empty()
    }

    /// `getDataLayerData(pos)` (queued layer first, else the published one).
    pub fn get_data_layer_data(&self, pos: SectionPos) -> Option<Arc<DataLayer>> {
        self.storage.get_data_layer_data(pos.as_long())
    }

    /// `getLightValue(pos)` on the published data.
    pub fn get_light_value(&self, pos: BlockPos) -> i32 {
        self.storage.get_light_value(pos.as_long())
    }

    pub fn get_debug_section_type(&self, section: SectionPos) -> SectionType {
        self.storage.get_debug_section_type(section.as_long())
    }

    /// Sections whose light changed since the last call (vanilla `chunkSource.onLightUpdate(layer, pos)` calls).
    pub fn take_light_updates(&mut self) -> Vec<SectionPos> {
        std::mem::take(&mut self.light_updates).into_iter().map(SectionPos::of_long).collect()
    }

    /// `setLightEnabled(pos, enable)`.
    pub fn set_light_enabled(&mut self, chunks: &dyn LightChunkGetter, pos: ChunkPos, enable: bool) {
        self.storage.set_light_enabled(SectionPos::get_zero_node_xz(pos.x, pos.z), enable);
        if self.layer == LightLayer::Sky && enable {
            let sources = chunks.get_chunk_for_lighting(pos.x, pos.z).map(|c| c.get_sky_light_sources()).unwrap_or(&self.empty_chunk_sources);
            // Java `int` arithmetic: an empty source set has `Integer.MIN_VALUE` here and wraps.
            let highest_non_source_y = sources.get_highest_lowest_source_y().wrapping_sub(1);
            let lowest_fully_source_section_y = SectionPos::block_to_section_coord_i32(highest_non_source_y).wrapping_add(1);
            let zero_node = SectionPos::get_zero_node_xz(pos.x, pos.z);
            let top_section_y = self.storage.get_top_section_y(zero_node);
            let bottom_section_y = self.storage.get_bottom_section_y().max(lowest_fully_source_section_y);
            let mut section_y = top_section_y - 1;
            while section_y >= bottom_section_y {
                if let Some(data_layer) = self.storage.get_data_layer_to_write(SectionPos::as_long_xyz(pos.x, section_y, pos.z))
                    && data_layer.is_empty()
                {
                    data_layer.fill(15);
                }
                section_y -= 1;
            }
        }
    }

    /// `propagateLightSources(pos)`.
    pub fn propagate_light_sources(&mut self, chunks: &dyn LightChunkGetter, pos: ChunkPos) {
        match self.layer {
            LightLayer::Block => {
                self.set_light_enabled(chunks, pos, true);
                if let Some(chunk) = chunks.get_chunk_for_lighting(pos.x, pos.z) {
                    let queue = &mut self.increase_queue;
                    chunk.find_block_light_sources(&mut |light_pos, state| {
                        let emission = i32::from(state.get_light_emission());
                        enqueue(queue, light_pos.as_long(), qe::increase_light_from_emission(emission, is_empty_shape(state)));
                    });
                }
            }
            LightLayer::Sky => self.propagate_sky_light_sources(chunks, pos),
        }
    }

    /// `runLightUpdates()`.
    pub fn run_light_updates(&mut self, chunks: &dyn LightChunkGetter) -> i32 {
        let mut cache = ChunkCache::new(chunks);
        let nodes: Vec<i64> = self.block_nodes_to_check.iter().collect();
        for node in nodes {
            self.check_node(&mut cache, node);
        }
        self.block_nodes_to_check.clear();
        self.block_nodes_to_check.trim(MIN_QUEUE_SIZE);
        let mut count = self.propagate_decreases(&mut cache);
        count += self.propagate_increases(&mut cache);
        self.storage.mark_new_inconsistencies();
        let affected = self.storage.swap_section_map();
        self.light_updates.extend(affected);
        count
    }

    fn propagate_increases(&mut self, cache: &mut ChunkCache<'_>) -> i32 {
        let mut count = 0;
        while let Some(from_node) = self.increase_queue.pop_front() {
            let increase_data = self.increase_queue.pop_front().expect("queue entries come in pairs");
            let mut from_level = self.storage.get_stored_level(from_node);
            let from_target_level = qe::get_from_level(increase_data);
            if qe::is_increase_from_emission(increase_data) && from_level < from_target_level {
                self.storage.set_stored_level(from_node, from_target_level);
                from_level = from_target_level;
            }
            if from_level == from_target_level {
                self.propagate_increase(cache, from_node, increase_data, from_level);
            }
            count += 1;
        }
        count
    }

    fn propagate_decreases(&mut self, cache: &mut ChunkCache<'_>) -> i32 {
        let mut count = 0;
        while let Some(from_node) = self.decrease_queue.pop_front() {
            let decrease_data = self.decrease_queue.pop_front().expect("queue entries come in pairs");
            self.propagate_decrease(cache, from_node, decrease_data);
            count += 1;
        }
        count
    }

    fn enqueue_decrease(&mut self, node: i64, data: i64) {
        enqueue(&mut self.decrease_queue, node, data);
    }

    fn enqueue_increase(&mut self, node: i64, data: i64) {
        enqueue(&mut self.increase_queue, node, data);
    }

    fn check_node(&mut self, cache: &mut ChunkCache<'_>, block_node: i64) {
        match self.layer {
            LightLayer::Block => self.check_node_block(cache, block_node),
            LightLayer::Sky => self.check_node_sky(cache.getter, block_node),
        }
    }

    fn propagate_increase(&mut self, cache: &mut ChunkCache<'_>, from_node: i64, increase_data: i64, from_level: i32) {
        let sky = self.layer == LightLayer::Sky;
        let empty_sections_below = if sky { self.count_empty_sections_below_if_at_border(from_node) } else { 0 };
        let mut from_state: Option<BlockState> = None;
        for propagation_direction in Direction::ALL {
            if !qe::should_propagate_in_direction(increase_data, propagation_direction) {
                continue;
            }
            let to_node = BlockPos::offset_long_direction(from_node, propagation_direction);
            if !self.storage.storing_light_for_section(SectionPos::block_to_section(to_node)) {
                continue;
            }
            let to_level = self.storage.get_stored_level(to_node);
            let max_possible_new_to_level = from_level - 1;
            if max_possible_new_to_level > to_level {
                let to_state = cache.get_state(to_node);
                let new_to_level = from_level - get_opacity(to_state);
                if new_to_level > to_level {
                    let from_state = *from_state.get_or_insert_with(|| {
                        if qe::is_from_empty_shape(increase_data) { Block::AIR.default_state() } else { cache.get_state(from_node) }
                    });
                    if !shape_occludes(from_state, to_state, propagation_direction) {
                        self.storage.set_stored_level(to_node, new_to_level);
                        if new_to_level > 1 {
                            self.enqueue_increase(
                                to_node,
                                qe::increase_skip_one_direction(new_to_level, is_empty_shape(to_state), propagation_direction.get_opposite()),
                            );
                        }
                        if sky {
                            self.propagate_from_empty_sections(to_node, propagation_direction, new_to_level, true, empty_sections_below);
                        }
                    }
                }
            }
        }
    }

    fn propagate_decrease(&mut self, cache: &mut ChunkCache<'_>, from_node: i64, decrease_data: i64) {
        let sky = self.layer == LightLayer::Sky;
        let empty_sections_below = if sky { self.count_empty_sections_below_if_at_border(from_node) } else { 0 };
        let old_from_level = qe::get_from_level(decrease_data);
        for propagation_direction in Direction::ALL {
            if !qe::should_propagate_in_direction(decrease_data, propagation_direction) {
                continue;
            }
            let to_node = BlockPos::offset_long_direction(from_node, propagation_direction);
            if !self.storage.storing_light_for_section(SectionPos::block_to_section(to_node)) {
                continue;
            }
            let to_level = self.storage.get_stored_level(to_node);
            if to_level == 0 {
                continue;
            }
            if to_level <= old_from_level - 1 {
                if sky {
                    self.storage.set_stored_level(to_node, 0);
                    self.enqueue_decrease(to_node, qe::decrease_skip_one_direction(to_level, propagation_direction.get_opposite()));
                    self.propagate_from_empty_sections(to_node, propagation_direction, to_level, false, empty_sections_below);
                } else {
                    let to_state = cache.get_state(to_node);
                    let to_emission = self.get_emission(to_node, to_state);
                    self.storage.set_stored_level(to_node, 0);
                    if to_emission < to_level {
                        self.enqueue_decrease(to_node, qe::decrease_skip_one_direction(to_level, propagation_direction.get_opposite()));
                    }
                    if to_emission > 0 {
                        self.enqueue_increase(to_node, qe::increase_light_from_emission(to_emission, is_empty_shape(to_state)));
                    }
                }
            } else {
                self.enqueue_increase(to_node, qe::increase_only_one_direction(to_level, false, propagation_direction.get_opposite()));
            }
        }
    }

    // ------------------------------------------------------------------ BlockLightEngine

    fn check_node_block(&mut self, cache: &mut ChunkCache<'_>, block_node: i64) {
        let section_node = SectionPos::block_to_section(block_node);
        if self.storage.storing_light_for_section(section_node) {
            let state = cache.get_state(block_node);
            let light_emission = self.get_emission(block_node, state);
            let old_level = self.storage.get_stored_level(block_node);
            if light_emission < old_level {
                self.storage.set_stored_level(block_node, 0);
                self.enqueue_decrease(block_node, qe::decrease_all_directions(old_level));
            } else {
                self.enqueue_decrease(block_node, PULL_LIGHT_IN_ENTRY);
            }
            if light_emission > 0 {
                self.enqueue_increase(block_node, qe::increase_light_from_emission(light_emission, is_empty_shape(state)));
            }
        }
    }

    fn get_emission(&self, block_node: i64, state: BlockState) -> i32 {
        let emission = i32::from(state.get_light_emission());
        if emission > 0 && self.storage.light_on_in_section(SectionPos::block_to_section(block_node)) { emission } else { 0 }
    }

    // ------------------------------------------------------------------ SkyLightEngine

    fn get_lowest_source_y(chunks: &dyn LightChunkGetter, x: i32, z: i32, default_value: i32) -> i32 {
        match chunks.get_chunk_for_lighting(SectionPos::block_to_section_coord_i32(x), SectionPos::block_to_section_coord_i32(z)) {
            Some(chunk) => chunk.get_sky_light_sources().get_lowest_source_y(SectionPos::section_relative(x), SectionPos::section_relative(z)),
            None => default_value,
        }
    }

    fn check_node_sky(&mut self, chunks: &dyn LightChunkGetter, block_node: i64) {
        let (x, y, z) = (BlockPos::get_x(block_node), BlockPos::get_y(block_node), BlockPos::get_z(block_node));
        let section_node = SectionPos::block_to_section(block_node);
        let lowest_source_y = if self.storage.light_on_in_section(section_node) { Self::get_lowest_source_y(chunks, x, z, i32::MAX) } else { i32::MAX };
        if lowest_source_y != i32::MAX {
            self.update_sources_in_column(chunks, x, z, lowest_source_y);
        }
        if self.storage.storing_light_for_section(section_node) {
            let is_source = y >= lowest_source_y;
            if is_source {
                self.enqueue_decrease(block_node, remove_sky_source_entry());
                self.enqueue_increase(block_node, add_sky_source_entry());
            } else {
                let old_level = self.storage.get_stored_level(block_node);
                if old_level > 0 {
                    self.storage.set_stored_level(block_node, 0);
                    self.enqueue_decrease(block_node, qe::decrease_all_directions(old_level));
                } else {
                    self.enqueue_decrease(block_node, PULL_LIGHT_IN_ENTRY);
                }
            }
        }
    }

    fn update_sources_in_column(&mut self, chunks: &dyn LightChunkGetter, x: i32, z: i32, lowest_source_y: i32) {
        let world_bottom_y = SectionPos::section_to_block_coord(self.storage.get_bottom_section_y());
        self.remove_sources_below(x, z, lowest_source_y, world_bottom_y);
        self.add_sources_above(chunks, x, z, lowest_source_y, world_bottom_y);
    }

    fn remove_sources_below(&mut self, x: i32, z: i32, lowest_source_y: i32, world_bottom_y: i32) {
        if lowest_source_y > world_bottom_y {
            let section_x = SectionPos::block_to_section_coord_i32(x);
            let section_z = SectionPos::block_to_section_coord_i32(z);
            let start_y = lowest_source_y - 1;
            let mut section_y = SectionPos::block_to_section_coord_i32(start_y);
            while self.storage.has_light_data_at_or_below(section_y) {
                if self.storage.storing_light_for_section(SectionPos::as_long_xyz(section_x, section_y, section_z)) {
                    let section_bottom_y = SectionPos::section_to_block_coord(section_y);
                    let section_top_y = section_bottom_y + 15;
                    let mut y = section_top_y.min(start_y);
                    while y >= section_bottom_y {
                        let block_node = BlockPos::as_long_xyz(x, y, z);
                        if self.storage.get_stored_level(block_node) != 15 {
                            return;
                        }
                        self.storage.set_stored_level(block_node, 0);
                        self.enqueue_decrease(block_node, if y == lowest_source_y - 1 { REMOVE_TOP_SKY_SOURCE_ENTRY } else { remove_sky_source_entry() });
                        y -= 1;
                    }
                }
                section_y -= 1;
            }
        }
    }

    fn add_sources_above(&mut self, chunks: &dyn LightChunkGetter, x: i32, z: i32, lowest_source_y: i32, world_bottom_y: i32) {
        let section_x = SectionPos::block_to_section_coord_i32(x);
        let section_z = SectionPos::block_to_section_coord_i32(z);
        let neighbor_lowest_source_y = Self::get_lowest_source_y(chunks, x - 1, z, i32::MIN)
            .max(Self::get_lowest_source_y(chunks, x + 1, z, i32::MIN))
            .max(Self::get_lowest_source_y(chunks, x, z - 1, i32::MIN).max(Self::get_lowest_source_y(chunks, x, z + 1, i32::MIN)));
        let start_y = lowest_source_y.max(world_bottom_y);
        let mut section_node = SectionPos::as_long_xyz(section_x, SectionPos::block_to_section_coord_i32(start_y), section_z);
        while !self.storage.is_above_data(section_node) {
            if self.storage.storing_light_for_section(section_node) {
                let section_bottom_y = SectionPos::section_to_block_coord(SectionPos::y_from_long(section_node));
                let section_top_y = section_bottom_y + 15;
                for y in section_bottom_y.max(start_y)..=section_top_y {
                    let block_node = BlockPos::as_long_xyz(x, y, z);
                    if self.storage.get_stored_level(block_node) == 15 {
                        return;
                    }
                    self.storage.set_stored_level(block_node, 15);
                    if y < neighbor_lowest_source_y || y == lowest_source_y {
                        self.enqueue_increase(block_node, add_sky_source_entry());
                    }
                }
            }
            section_node = SectionPos::offset_long_direction(section_node, Direction::Up);
        }
    }

    fn count_empty_sections_below_if_at_border(&self, block_node: i64) -> i32 {
        let y = BlockPos::get_y(block_node);
        let local_y = SectionPos::section_relative(y);
        if local_y != 0 {
            return 0;
        }
        let x = BlockPos::get_x(block_node);
        let z = BlockPos::get_z(block_node);
        let local_x = SectionPos::section_relative(x);
        let local_z = SectionPos::section_relative(z);
        if local_x != 0 && local_x != 15 && local_z != 0 && local_z != 15 {
            return 0;
        }
        let section_x = SectionPos::block_to_section_coord_i32(x);
        let section_y = SectionPos::block_to_section_coord_i32(y);
        let section_z = SectionPos::block_to_section_coord_i32(z);
        let mut empty_sections_below = 0;
        while !self.storage.storing_light_for_section(SectionPos::as_long_xyz(section_x, section_y - empty_sections_below - 1, section_z))
            && self.storage.has_light_data_at_or_below(section_y - empty_sections_below - 1)
        {
            empty_sections_below += 1;
        }
        empty_sections_below
    }

    fn propagate_from_empty_sections(&mut self, to_node: i64, propagation_direction: Direction, to_level: i32, increase: bool, empty_sections_below: i32) {
        if empty_sections_below == 0 {
            return;
        }
        let x = BlockPos::get_x(to_node);
        let z = BlockPos::get_z(to_node);
        if crossed_section_edge(propagation_direction, SectionPos::section_relative(x), SectionPos::section_relative(z)) {
            let y = BlockPos::get_y(to_node);
            let section_x = SectionPos::block_to_section_coord_i32(x);
            let section_z = SectionPos::block_to_section_coord_i32(z);
            let mut section_y = SectionPos::block_to_section_coord_i32(y) - 1;
            let bottom_section_y = section_y - empty_sections_below + 1;
            while section_y >= bottom_section_y {
                if !self.storage.storing_light_for_section(SectionPos::as_long_xyz(section_x, section_y, section_z)) {
                    section_y -= 1;
                } else {
                    let section_min_y = SectionPos::section_to_block_coord(section_y);
                    for local_y in (0..=15).rev() {
                        let block_node = BlockPos::as_long_xyz(x, section_min_y + local_y, z);
                        if increase {
                            self.storage.set_stored_level(block_node, to_level);
                            if to_level > 1 {
                                self.enqueue_increase(block_node, qe::increase_skip_one_direction(to_level, true, propagation_direction.get_opposite()));
                            }
                        } else {
                            self.storage.set_stored_level(block_node, 0);
                            self.enqueue_decrease(block_node, qe::decrease_skip_one_direction(to_level, propagation_direction.get_opposite()));
                        }
                    }
                    section_y -= 1;
                }
            }
        }
    }

    fn propagate_sky_light_sources(&mut self, chunks: &dyn LightChunkGetter, pos: ChunkPos) {
        let zero_node = SectionPos::get_zero_node_xz(pos.x, pos.z);
        self.storage.set_light_enabled(zero_node, true);
        let empty = &self.empty_chunk_sources;
        let sources_at = |x: i32, z: i32| chunks.get_chunk_for_lighting(x, z).map(|c| c.get_sky_light_sources()).unwrap_or(empty);
        let sources = sources_at(pos.x, pos.z);
        let north_sources = sources_at(pos.x, pos.z - 1);
        let south_sources = sources_at(pos.x, pos.z + 1);
        let west_sources = sources_at(pos.x - 1, pos.z);
        let east_sources = sources_at(pos.x + 1, pos.z);
        let top_section_y = self.storage.get_top_section_y(zero_node);
        let bottom_section_y = self.storage.get_bottom_section_y();
        let section_min_x = SectionPos::section_to_block_coord(pos.x);
        let section_min_z = SectionPos::section_to_block_coord(pos.z);

        let mut section_y = top_section_y - 1;
        while section_y >= bottom_section_y {
            let section_node = SectionPos::as_long_xyz(pos.x, section_y, pos.z);
            if let Some(data_layer) = self.storage.get_data_layer_to_write(section_node) {
                let section_min_y = SectionPos::section_to_block_coord(section_y);
                let section_max_y = section_min_y + 15;
                let mut sources_below = false;
                for z in 0..16 {
                    for x in 0..16 {
                        let lowest_source_y = sources.get_lowest_source_y(x, z);
                        if lowest_source_y <= section_max_y {
                            let north_lowest_source_y = if z == 0 { north_sources.get_lowest_source_y(x, 15) } else { sources.get_lowest_source_y(x, z - 1) };
                            let south_lowest_source_y = if z == 15 { south_sources.get_lowest_source_y(x, 0) } else { sources.get_lowest_source_y(x, z + 1) };
                            let west_lowest_source_y = if x == 0 { west_sources.get_lowest_source_y(15, z) } else { sources.get_lowest_source_y(x - 1, z) };
                            let east_lowest_source_y = if x == 15 { east_sources.get_lowest_source_y(0, z) } else { sources.get_lowest_source_y(x + 1, z) };
                            let neighbor_lowest_source_y = north_lowest_source_y.max(south_lowest_source_y).max(west_lowest_source_y.max(east_lowest_source_y));
                            let mut y = section_max_y;
                            while y >= section_min_y.max(lowest_source_y) {
                                data_layer.set(x, SectionPos::section_relative(y), z, 15);
                                if y == lowest_source_y || y < neighbor_lowest_source_y {
                                    let block_node = BlockPos::as_long_xyz(section_min_x + x, y, section_min_z + z);
                                    enqueue(
                                        &mut self.increase_queue,
                                        block_node,
                                        qe::increase_sky_source_in_directions(
                                            y == lowest_source_y,
                                            y < north_lowest_source_y,
                                            y < south_lowest_source_y,
                                            y < west_lowest_source_y,
                                            y < east_lowest_source_y,
                                        ),
                                    );
                                }
                                y -= 1;
                            }
                            if lowest_source_y < section_min_y {
                                sources_below = true;
                            }
                        }
                    }
                }
                if !sources_below {
                    break;
                }
            }
            section_y -= 1;
        }
    }
}

fn crossed_section_edge(propagation_direction: Direction, x: i32, z: i32) -> bool {
    match propagation_direction {
        Direction::North => z == 15,
        Direction::South => z == 0,
        Direction::West => x == 15,
        Direction::East => x == 0,
        _ => false,
    }
}
