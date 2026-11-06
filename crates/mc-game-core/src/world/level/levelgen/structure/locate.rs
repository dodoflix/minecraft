//! `ChunkGenerator.findNearestMapStructure` (`/locate`, explorer maps, eyes of ender): the walk over candidate chunks is
//! `mc_worldgen::structure_placement::find_nearest_map_structure`; this module supplies `getStructureGeneratingAt`
//! (structure check, then the chunk's start) and the reference bookkeeping.

use super::check::StructureCheckResult;
use super::manager::StructureManager;
use crate::world::level::levelgen::env::GenEnv;
use mc_core::ident::Identifier;
use mc_core::pos::{BlockPos, ChunkPos};
use mc_world::chunk::StructureData;
use mc_worldgen::structure_placement::{
    LocateResult, StructurePlacement, find_nearest_map_structure as walk,
};

/// The level side of the locate: chunks at `STRUCTURE_STARTS` (loaded or generated on demand) and persistence of the
/// reference counter of a start.
pub trait LocateLevel {
    /// `level.getChunk(x, z, ChunkStatus.STRUCTURE_STARTS)`: runs `f` on the chunk's structure data (`false` if the chunk
    /// cannot be provided).
    fn with_structure_data(&mut self, pos: ChunkPos, f: &mut dyn FnMut(&StructureData)) -> bool;

    /// Stores `references` into the raw start of `structure_id` in the chunk (`StructureStart.addReference` mutates the
    /// chunk's start object, which is saved with the chunk) and marks it unsaved.
    fn set_start_references(&mut self, pos: ChunkPos, structure_id: &str, references: i32);
}

/// `ChunkGenerator.findNearestMapStructure(level, wantedStructures, pos, maxSearchRadius, createReference)`.
pub fn find_nearest_map_structure(
    env: &GenEnv,
    level: &mut dyn LocateLevel,
    wanted_structures: &[Identifier],
    pos: BlockPos,
    max_search_radius: i32,
    create_reference: bool,
) -> LocateResult {
    if !env.generate_structures {
        return None;
    }
    let state = env.structure_state.as_deref()?;
    let manager = StructureManager::new(env);
    let mut generating_at = |structures: &[Identifier],
                             placement: &StructurePlacement,
                             chunk_target: ChunkPos|
     -> LocateResult {
        get_structure_generating_at(
            env,
            &manager,
            level,
            structures,
            placement,
            chunk_target,
            create_reference,
        )
    };
    walk(
        state,
        wanted_structures,
        pos,
        max_search_radius,
        &mut generating_at,
    )
}

/// `getStructureGeneratingAt(structures, level, structureManager, createReference, config, chunkTarget)`.
fn get_structure_generating_at(
    env: &GenEnv,
    manager: &StructureManager<'_>,
    level: &mut dyn LocateLevel,
    structures: &[Identifier],
    config: &StructurePlacement,
    chunk_target: ChunkPos,
    create_reference: bool,
) -> LocateResult {
    for id in structures {
        let Some(structure) = env.structures.get(id) else {
            continue;
        };
        let fast_check =
            env.structure_check
                .check_start(env, chunk_target, structure, config, create_reference);
        if fast_check == StructureCheckResult::StartNotPresent {
            continue;
        }
        if !create_reference && fast_check == StructureCheckResult::StartPresent {
            return Some((config.get_locate_pos(chunk_target), id.clone()));
        }
        let mut found: Option<(ChunkPos, i32)> = None;
        level.with_structure_data(chunk_target, &mut |data| {
            if let Some(start) = manager.get_start_for_structure(chunk_target, data, structure) {
                let mut start = start.lock();
                // `tryAddReference`
                if start.is_valid() && (!create_reference || start.can_be_referenced()) {
                    if create_reference {
                        start.add_reference();
                        env.structure_check
                            .increment_reference(start.get_chunk_pos(), id);
                    }
                    found = Some((start.get_chunk_pos(), start.get_references()));
                }
            }
        });
        if let Some((chunk_pos, references)) = found {
            if create_reference {
                level.set_start_references(chunk_target, &id.to_string(), references);
            }
            return Some((config.get_locate_pos(chunk_pos), id.clone()));
        }
    }
    None
}
