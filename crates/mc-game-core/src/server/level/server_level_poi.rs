//! `ServerLevel` side of the POI manager: `isVillage`/`sectionsToVillage` and `PoiManager.ensureLoadedAndValid`
//! (which loads chunks through the level).

use super::ServerLevel;
use crate::world::entity::ai::village::poi::PoiTypeHolder;
use mc_core::pos::{BlockPos, SectionPos};
use mc_world::status::ChunkStatus;

impl ServerLevel {
    /// `isVillage(pos)`.
    pub fn is_village(&self, pos: BlockPos) -> bool {
        self.is_close_to_village(pos, 1)
    }

    /// `isVillage(sectionPos)`.
    pub fn is_village_section(&self, section_pos: SectionPos) -> bool {
        self.is_village(section_pos.center())
    }

    /// `isCloseToVillage(pos, sectionDistance)`.
    pub fn is_close_to_village(&self, pos: BlockPos, section_distance: i32) -> bool {
        section_distance <= 6
            && self.sections_to_village(SectionPos::of_block_pos(&pos)) <= section_distance
    }

    /// Binds `PoiTypeTags.VILLAGE` of the level's registries (data packs may change the tag) for the village distances.
    pub fn refresh_poi_village_tag(&mut self) {
        let village = mc_core::ident::Identifier::parse("minecraft:village").expect("valid tag");
        let mask = (0..mc_data::misc::POI_TYPES.len())
            .filter_map(PoiTypeHolder::from_index)
            .filter(|t| {
                mc_core::ident::Identifier::parse(t.registered_name()).is_ok_and(|id| {
                    self.registries
                        .is_in_tag("minecraft:point_of_interest_type", &id, &village)
                })
            })
            .fold(0u32, |mask, t| mask | 1 << t.index());
        self.poi_manager().set_village_types(mask);
    }

    /// `sectionsToVillage(pos)`.
    pub fn sections_to_village(&self, section_pos: SectionPos) -> i32 {
        self.poi_manager().sections_to_village(section_pos)
    }

    /// `PoiManager.ensureLoadedAndValid(reader, center, radius)`: every section around `center` that has no valid POI data
    /// gets its chunk loaded at least to `EMPTY` (once), which checks the POI data against the blocks.
    pub fn poi_ensure_loaded_and_valid(&mut self, center: BlockPos, radius: i32) {
        // lazy like the stream: a chunk load can validate the sections visited after it
        for section in self.poi_manager().sections_around(center, radius) {
            if self.poi_manager().is_section_valid(section) {
                continue;
            }
            let chunk = section.chunk();
            if self.poi_manager().mark_chunk_ensured(chunk) {
                self.proto_chunk_blocking(chunk.x, chunk.z, ChunkStatus::Empty, true);
            }
        }
    }
}
impl ServerLevel {
    /// `holder.is(PoiTypeTags.VILLAGE)`.
    pub fn poi_type_is_village(&self, t: PoiTypeHolder) -> bool {
        let village = mc_core::ident::Identifier::parse("minecraft:village").expect("valid tag");
        mc_core::ident::Identifier::parse(t.registered_name()).is_ok_and(|id| {
            self.registries
                .is_in_tag("minecraft:point_of_interest_type", &id, &village)
        })
    }
}
