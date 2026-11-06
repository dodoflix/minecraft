//! `net.minecraft.world.level.lighting.ChunkSkyLightSources` (per chunk column: the lowest Y from which the sky
//! shines straight down) and the `LightEngine` shape helpers it shares with the light engine.
//! It lives in mc-world because every `ChunkAccess` owns one.

use crate::heightmap::HeightmapChunk;
use crate::section::LevelChunkSection;
use mc_core::bits::{BitStorage, SimpleBitStorage};
use mc_core::direction::Direction;
use mc_core::math::mth;
use mc_core::phys::shapes::{Shapes, VoxelShape};
use mc_core::pos::SectionPos;
use mc_data::block::BlockState;

pub const NEGATIVE_INFINITY: i32 = i32::MIN;

/// `LightEngine.isEmptyShape`.
pub fn is_empty_shape(state: BlockState) -> bool {
    !state.can_occlude() || !state.use_shape_for_light_occlusion()
}

/// `LightEngine.getOcclusionShape(state, direction)`.
pub fn get_occlusion_shape(state: BlockState, direction: Direction) -> VoxelShape {
    if is_empty_shape(state) {
        Shapes::empty()
    } else {
        state.face_occlusion_shape(direction)
    }
}

#[derive(Clone, Debug)]
pub struct ChunkSkyLightSources {
    min_y: i32,
    heightmap: SimpleBitStorage,
}

impl ChunkSkyLightSources {
    /// `new ChunkSkyLightSources(level)`; `level_min_y`/`level_height` of the `LevelHeightAccessor`.
    pub fn new(level_min_y: i32, level_height: i32) -> Self {
        let min_y = level_min_y - 1;
        let max_y = level_min_y + level_height - 1 + 1;
        let bits = mth::ceillog2(max_y - min_y + 1);
        ChunkSkyLightSources {
            min_y,
            heightmap: SimpleBitStorage::new(bits, 256),
        }
    }

    /// `fillFrom(chunk)` for a chunk given as its sections (`level_min_section_y` = `getMinSectionY`).
    pub fn fill_from(&mut self, level_min_section_y: i32, sections: &[LevelChunkSection]) {
        match sections.iter().rposition(|s| !s.has_only_air()) {
            None => self.fill(self.min_y),
            Some(max_section_index) => {
                for z in 0..16 {
                    for x in 0..16 {
                        let initial_edge_y = self
                            .find_lowest_source_y(
                                level_min_section_y,
                                sections,
                                max_section_index,
                                x,
                                z,
                            )
                            .max(self.min_y);
                        self.set(Self::index(x, z), initial_edge_y);
                    }
                }
            }
        }
    }

    fn find_lowest_source_y(
        &self,
        level_min_section_y: i32,
        sections: &[LevelChunkSection],
        top_section_index: usize,
        x: i32,
        z: i32,
    ) -> i32 {
        let mut top_y =
            SectionPos::section_to_block_coord(level_min_section_y + top_section_index as i32 + 1);
        let mut top_state = mc_data::block::Block::AIR.default_state();
        for section_index in (0..=top_section_index).rev() {
            let section = &sections[section_index];
            if section.has_only_air() {
                top_state = mc_data::block::Block::AIR.default_state();
                top_y =
                    SectionPos::section_to_block_coord(level_min_section_y + section_index as i32);
            } else {
                for y in (0..=15).rev() {
                    let bottom_state = section.get_block_state(x, y, z);
                    if is_edge_occluded(top_state, bottom_state) {
                        return top_y;
                    }
                    top_state = bottom_state;
                    top_y -= 1;
                }
            }
        }
        self.min_y
    }

    /// `update(level, x, y, z)` with chunk-local `x`/`z`; returns whether the lowest source changed.
    pub fn update(&mut self, level: &impl HeightmapChunk, x: i32, y: i32, z: i32) -> bool {
        let upper_edge_y = y + 1;
        let index = Self::index(x, z);
        let current_lowest_source_y = self.get(index);
        if upper_edge_y < current_lowest_source_y {
            return false;
        }
        let top_state = level.get_block_state(x, y + 1, z);
        let middle_state = level.get_block_state(x, y, z);
        if self.update_edge(
            level,
            index,
            current_lowest_source_y,
            (x, y + 1, z),
            top_state,
            (x, y, z),
            middle_state,
        ) {
            return true;
        }
        let bottom_state = level.get_block_state(x, y - 1, z);
        self.update_edge(
            level,
            index,
            current_lowest_source_y,
            (x, y, z),
            middle_state,
            (x, y - 1, z),
            bottom_state,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn update_edge(
        &mut self,
        level: &impl HeightmapChunk,
        index: usize,
        old_top_edge_y: i32,
        top_pos: (i32, i32, i32),
        top_state: BlockState,
        bottom_pos: (i32, i32, i32),
        bottom_state: BlockState,
    ) -> bool {
        let checked_edge_y = top_pos.1;
        if is_edge_occluded(top_state, bottom_state) {
            if checked_edge_y > old_top_edge_y {
                self.set(index, checked_edge_y);
                return true;
            }
        } else if checked_edge_y == old_top_edge_y {
            let lowest = self.find_lowest_source_below(level, bottom_pos, bottom_state);
            self.set(index, lowest);
            return true;
        }
        false
    }

    fn find_lowest_source_below(
        &self,
        level: &impl HeightmapChunk,
        start_pos: (i32, i32, i32),
        start_state: BlockState,
    ) -> i32 {
        let (x, start_y, z) = start_pos;
        let mut top_y = start_y;
        let mut bottom_y = start_y - 1;
        let mut top_state = start_state;
        while bottom_y >= self.min_y {
            let bottom_state = level.get_block_state(x, bottom_y, z);
            if is_edge_occluded(top_state, bottom_state) {
                return top_y;
            }
            top_state = bottom_state;
            top_y = bottom_y;
            bottom_y -= 1;
        }
        self.min_y
    }

    pub fn get_lowest_source_y(&self, x: i32, z: i32) -> i32 {
        self.extend_sources_below_world(self.get(Self::index(x, z)))
    }

    pub fn get_highest_lowest_source_y(&self) -> i32 {
        let max_value = (0..self.heightmap.get_size())
            .map(|i| self.heightmap.get(i))
            .fold(i32::MIN, i32::max);
        self.extend_sources_below_world(max_value.wrapping_add(self.min_y))
    }

    fn fill(&mut self, lowest_source_y: i32) {
        let value = lowest_source_y - self.min_y;
        for i in 0..self.heightmap.get_size() {
            self.heightmap.set(i, value);
        }
    }

    fn set(&mut self, index: usize, value: i32) {
        self.heightmap.set(index, value - self.min_y);
    }

    fn get(&self, index: usize) -> i32 {
        self.heightmap.get(index) + self.min_y
    }

    fn extend_sources_below_world(&self, value: i32) -> i32 {
        if value == self.min_y {
            NEGATIVE_INFINITY
        } else {
            value
        }
    }

    fn index(x: i32, z: i32) -> usize {
        (x + z * 16) as usize
    }
}

fn is_edge_occluded(top_state: BlockState, bottom_state: BlockState) -> bool {
    if bottom_state.get_light_block() != 0 {
        return true;
    }
    if is_empty_shape(top_state) && is_empty_shape(bottom_state) {
        return false; // faceShapeOccludes(empty, empty)
    }
    let top_shape = get_occlusion_shape(top_state, Direction::Down);
    let bottom_shape = get_occlusion_shape(bottom_state, Direction::Up);
    Shapes::face_shape_occludes(&top_shape, &bottom_shape)
}
