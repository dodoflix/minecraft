//! The structure side of the `NOISE` step: `Beardifier.forStructuresInChunk`'s input, i.e. the starts reaching the chunk
//! whose structure adapts the terrain, as [`StructureStartInfo`]s.

use super::manager::{StructureDataSource, StructureManager};
use crate::world::level::levelgen::env::GenEnv;
use mc_core::pos::ChunkPos;
use mc_world::chunk::{ProtoChunk, StructureData};
use mc_worldgen::beardifier::{StructurePieceInfo, StructureStartInfo, TerrainAdjustment};

/// The chunk being generated (lent separately) plus the surrounding region's chunks.
pub struct CenterAndRegion<'a> {
    pub center: &'a ProtoChunk,
    pub region: &'a crate::server::level::world_gen::WorldGenRegion,
}

impl StructureDataSource for CenterAndRegion<'_> {
    fn structure_data(&self, pos: ChunkPos) -> Option<&StructureData> {
        if pos == self.center.access.pos { Some(&self.center.access.structures) } else { self.region.structure_data(pos) }
    }
}

/// `structureManager.startsForStructure(chunkX, chunkZ, s -> s.terrainAdaptation() != NONE)` converted for the
/// beardifier (`getStructure().terrainAdaptation()`, every piece's box and `PoolElementStructurePiece` data).
pub fn structure_start_infos(env: &GenEnv, source: &dyn StructureDataSource, chunk_pos: ChunkPos) -> Vec<StructureStartInfo> {
    let manager = StructureManager::new(env);
    let adapts = |id: &mc_core::ident::Identifier| env.structures.get(id).is_some_and(|s| s.value().terrain_adaptation() != TerrainAdjustment::None);
    manager
        .starts_for_structure_matching(source, chunk_pos.x, chunk_pos.z, &adapts)
        .into_iter()
        .map(|start| {
            let start = start.lock();
            StructureStartInfo {
                terrain_adaptation: start.get_structure().value().terrain_adaptation(),
                pieces: start
                    .piece_container
                    .pieces
                    .iter()
                    .map(|p| StructurePieceInfo { bounding_box: p.base().bounding_box, kind: p.beardifier_kind() })
                    .collect(),
            }
        })
        .collect()
}
