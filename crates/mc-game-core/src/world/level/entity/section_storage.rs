//! `net.minecraft.world.level.entity.EntitySectionStorage`.
use super::{EntitySection, Visibility};
use mc_core::phys::AABB;
use mc_core::pos::SectionPos;
use mc_core::pos::chunk_pos::ChunkPos;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::ops::ControlFlow;

pub struct EntitySectionStorage {
    sections: HashMap<i64, EntitySection>,
    /// `sectionIds` (`LongAVLTreeSet`): signed-long order decides the query iteration order.
    section_ids: BTreeSet<i64>,
}

fn chunk_key_from_section_key(section_pos: i64) -> i64 {
    ChunkPos::pack_xz(
        SectionPos::x_from_long(section_pos),
        SectionPos::z_from_long(section_pos),
    )
}

impl EntitySectionStorage {
    pub const CHONKY_ENTITY_SEARCH_GRACE: i32 = 2;
    pub const MAX_NON_CHONKY_ENTITY_SIZE: i32 = 4;

    pub fn new() -> Self {
        Self {
            sections: HashMap::new(),
            section_ids: BTreeSet::new(),
        }
    }

    /// `forEachAccessibleNonEmptySection`: calls `output(section_key, section)` in vanilla order until it breaks.
    pub fn for_each_accessible_non_empty_section(
        &self,
        bb: &AABB,
        mut output: impl FnMut(i64, &EntitySection) -> ControlFlow<()>,
    ) {
        let x_min = SectionPos::pos_to_section_coord(bb.min_x - 2.0);
        let y_min = SectionPos::pos_to_section_coord(bb.min_y - 4.0);
        let z_min = SectionPos::pos_to_section_coord(bb.min_z - 2.0);
        let x_max = SectionPos::pos_to_section_coord(bb.max_x + 2.0);
        let y_max = SectionPos::pos_to_section_coord(bb.max_y + 0.0);
        let z_max = SectionPos::pos_to_section_coord(bb.max_z + 2.0);
        for x in x_min..=x_max {
            let lowest = SectionPos::as_long_xyz(x, 0, 0);
            let highest = SectionPos::as_long_xyz(x, -1, -1);
            for &section_key in self.section_ids.range(lowest..=highest) {
                let y = SectionPos::y_from_long(section_key);
                let z = SectionPos::z_from_long(section_key);
                if y >= y_min
                    && y <= y_max
                    && z >= z_min
                    && z <= z_max
                    && let Some(section) = self.sections.get(&section_key)
                    && !section.is_empty()
                    && section.status().is_accessible()
                    && output(section_key, section).is_break()
                {
                    return;
                }
            }
        }
    }

    /// `getExistingSectionPositionsInChunk`: section keys of a chunk in `sectionIds` order.
    pub fn existing_section_positions_in_chunk(&self, chunk_key: i64) -> Vec<i64> {
        let x = ChunkPos::get_x(chunk_key);
        let z = ChunkPos::get_z(chunk_key);
        let lowest = SectionPos::as_long_xyz(x, 0, z);
        let highest = SectionPos::as_long_xyz(x, -1, z);
        self.section_ids.range(lowest..=highest).copied().collect()
    }

    pub fn get_or_create_section(
        &mut self,
        key: i64,
        initial_visibility: impl FnOnce(i64) -> Visibility,
    ) -> &mut EntitySection {
        if !self.sections.contains_key(&key) {
            let status = initial_visibility(chunk_key_from_section_key(key));
            self.section_ids.insert(key);
            self.sections.insert(key, EntitySection::new(status));
        }
        self.sections
            .get_mut(&key)
            .expect("section was just inserted")
    }

    pub fn get_section(&self, key: i64) -> Option<&EntitySection> {
        self.sections.get(&key)
    }

    pub fn get_section_mut(&mut self, key: i64) -> Option<&mut EntitySection> {
        self.sections.get_mut(&key)
    }

    pub fn get_all_chunks_with_existing_sections(&self) -> HashSet<i64> {
        self.sections
            .keys()
            .map(|&k| chunk_key_from_section_key(k))
            .collect()
    }

    pub fn remove(&mut self, section_key: i64) {
        self.sections.remove(&section_key);
        self.section_ids.remove(&section_key);
    }

    pub fn count(&self) -> usize {
        self.section_ids.len()
    }
}

impl Default for EntitySectionStorage {
    fn default() -> Self {
        Self::new()
    }
}
