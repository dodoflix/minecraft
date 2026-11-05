//! `StructureManager` (`net.minecraft.world.level.StructureManager`): structure start/reference queries over the chunks of
//! a level or generation region.
//!
//! Vanilla's manager wraps the level (`forWorldGenRegion`); here the level is passed to each query as a
//! [`StructureDataSource`] (a `WorldGenLevel`, or a `WorldGenRegion` during structure references) so that a piece being
//! placed can hold `&mut dyn WorldGenLevel` and the manager at once. Chunk contents are raw NBT; parsed starts come from
//! the environment's [`StartCache`](super::registries::StartCache).

use super::structure::Structure;
use super::structure_start::StartHandle;
use crate::world::level::levelgen::env::GenEnv;
use crate::world::level::levelgen::holder::Holder;
use crate::world::level::levelgen::world_gen_level::WorldGenLevel;
use indexmap::IndexSet;
use mc_core::ident::Identifier;
use mc_core::pos::{BlockPos, ChunkPos, SectionPos};
use mc_world::chunk::StructureData;

/// Where the structure starts/references of a chunk come from (`level.getChunk(x, z, STRUCTURE_STARTS/REFERENCES)`).
pub trait StructureDataSource {
    fn structure_data(&self, pos: ChunkPos) -> Option<&StructureData>;
}

/// A `WorldGenLevel` as a structure data source.
pub struct LevelSource<'a>(pub &'a dyn WorldGenLevel);

impl StructureDataSource for LevelSource<'_> {
    fn structure_data(&self, pos: ChunkPos) -> Option<&StructureData> {
        self.0.structure_data(pos)
    }
}

/// `StructureManager`.
pub struct StructureManager<'a> {
    pub env: &'a GenEnv,
}

impl<'a> StructureManager<'a> {
    pub fn new(env: &'a GenEnv) -> StructureManager<'a> {
        StructureManager { env }
    }

    /// `shouldGenerateStructures()`.
    pub fn should_generate_structures(&self) -> bool {
        self.env.generate_structures
    }

    /// `getStartForStructure(structure, chunk)`: the parsed start of `structure` in the chunk's data.
    pub fn get_start_for_structure(&self, chunk_pos: ChunkPos, data: &StructureData, structure: &Holder<dyn Structure>) -> Option<StartHandle> {
        let id = structure.id()?.to_string();
        let raw = data.starts.get(&id)?;
        self.env.start_cache.get_or_parse(self.env, chunk_pos, &id, raw)
    }

    /// `startsForStructure(sectionX, sectionZ, matcher)`: the valid starts of every referenced structure the matcher
    /// accepts, in reference order.
    pub fn starts_for_structure_matching(&self, source: &dyn StructureDataSource, section_x: i32, section_z: i32, matcher: &dyn Fn(&Identifier) -> bool) -> Vec<StartHandle> {
        let mut result = Vec::new();
        let Some(data) = source.structure_data(ChunkPos::new(section_x, section_z)) else { return result };
        for (key, references) in &data.references {
            let Some(id) = Identifier::try_parse(key) else { continue };
            if let Some(structure) = self.env.structures.get(&id)
                && matcher(&id)
            {
                self.fill_starts_for_structure(source, structure, references, &mut |start| result.push(start));
            }
        }
        result
    }

    /// `startsForStructure(sectionX, sectionZ, structure)`.
    pub fn starts_for_structure(&self, source: &dyn StructureDataSource, section_x: i32, section_z: i32, structure: &Holder<dyn Structure>) -> Vec<StartHandle> {
        let mut result = Vec::new();
        let Some(data) = source.structure_data(ChunkPos::new(section_x, section_z)) else { return result };
        let Some(id) = structure.id() else { return result };
        if let Some(references) = data.references.get(&id.to_string()) {
            self.fill_starts_for_structure(source, structure, references, &mut |start| result.push(start));
        }
        result
    }

    /// `fillStartsForStructure(structure, referencesForStructure, consumer)`.
    pub fn fill_starts_for_structure(
        &self,
        source: &dyn StructureDataSource,
        structure: &Holder<dyn Structure>,
        references_for_structure: &IndexSet<i64>,
        consumer: &mut dyn FnMut(StartHandle),
    ) {
        for &key in references_for_structure {
            let chunk_pos = ChunkPos::unpack(key);
            let Some(data) = source.structure_data(chunk_pos) else { continue };
            if let Some(start) = self.get_start_for_structure(chunk_pos, data, structure)
                && start.lock().is_valid()
            {
                consumer(start);
            }
        }
    }

    /// `getStructureAt(blockPos, structure)`; `None` is vanilla's `INVALID_START`.
    pub fn get_structure_at(&self, source: &dyn StructureDataSource, block_pos: BlockPos, structure: &Holder<dyn Structure>) -> Option<StartHandle> {
        let section_x = SectionPos::block_to_section_coord_i32(block_pos.x());
        let section_z = SectionPos::block_to_section_coord_i32(block_pos.z());
        self.starts_for_structure(source, section_x, section_z, structure).into_iter().find(|start| start.lock().get_bounding_box().is_inside(&block_pos))
    }

    /// `getStructureAt(blockPos, HolderSet<Structure>)` with the set as a membership test on the structure id.
    pub fn get_structure_at_set(&self, source: &dyn StructureDataSource, block_pos: BlockPos, structures: &dyn Fn(&Identifier) -> bool) -> Option<StartHandle> {
        let section_x = SectionPos::block_to_section_coord_i32(block_pos.x());
        let section_z = SectionPos::block_to_section_coord_i32(block_pos.z());
        self.starts_for_structure_matching(source, section_x, section_z, structures)
            .into_iter()
            .find(|start| start.lock().get_bounding_box().is_inside(&block_pos))
    }

    /// `getStructureWithPieceAt(blockPos, predicate)` (tag/holder-set variants are the predicate on the structure id).
    pub fn get_structure_with_piece_at(&self, source: &dyn StructureDataSource, block_pos: BlockPos, predicate: &dyn Fn(&Identifier) -> bool) -> Option<StartHandle> {
        let section_x = SectionPos::block_to_section_coord_i32(block_pos.x());
        let section_z = SectionPos::block_to_section_coord_i32(block_pos.z());
        self.starts_for_structure_matching(source, section_x, section_z, predicate)
            .into_iter()
            .find(|start| self.structure_has_piece_at(block_pos, &start.lock()))
    }

    /// `getStructureWithPieceAt(x, y, z, structure)`.
    pub fn get_structure_with_piece_at_structure(&self, source: &dyn StructureDataSource, x: i32, y: i32, z: i32, structure: &Holder<dyn Structure>) -> Option<StartHandle> {
        let section_x = SectionPos::block_to_section_coord_i32(x);
        let section_z = SectionPos::block_to_section_coord_i32(z);
        self.starts_for_structure(source, section_x, section_z, structure)
            .into_iter()
            .find(|start| Self::structure_has_piece_at_xyz(x, y, z, &start.lock()))
    }

    /// `structureHasPieceAt(blockPos, structureStart)`.
    pub fn structure_has_piece_at(&self, block_pos: BlockPos, start: &super::structure_start::StructureStart) -> bool {
        Self::structure_has_piece_at_xyz(block_pos.x(), block_pos.y(), block_pos.z(), start)
    }

    /// `structureHasPieceAt(x, y, z, structureStart)`.
    pub fn structure_has_piece_at_xyz(x: i32, y: i32, z: i32, start: &super::structure_start::StructureStart) -> bool {
        start.piece_container.pieces.iter().any(|p| p.base().bounding_box.is_inside_xyz(x, y, z))
    }

    /// `getAllStructuresAt(pos)`: the references of the chunk containing `pos`, structure id -> origin chunk keys.
    pub fn get_all_structures_at<'s>(&self, source: &'s dyn StructureDataSource, pos: BlockPos) -> Option<&'s indexmap::IndexMap<String, IndexSet<i64>>> {
        let section_x = SectionPos::block_to_section_coord_i32(pos.x());
        let section_z = SectionPos::block_to_section_coord_i32(pos.z());
        source.structure_data(ChunkPos::new(section_x, section_z)).map(|d| &d.references)
    }
}
