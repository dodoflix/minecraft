//! The placement-walking part of `ChunkGenerator.findNearestMapStructure` (`/locate`, explorer maps): the order in
//! which candidate chunks are visited. Whether a candidate chunk really holds one of the wanted structures
//! (`getStructureGeneratingAt`: structure check / chunk `STRUCTURE_STARTS` / references) needs structure starts and
//! is supplied by the caller as a closure; the `SharedConstants.DEBUG_DISABLE_FEATURES` and `generateStructures`
//! checks stay with the caller too.

use mc_core::ident::Identifier;
use mc_core::pos::block_pos::BlockPos;
use mc_core::pos::chunk_pos::ChunkPos;
use mc_core::pos::section_pos::SectionPos;

use super::placement::{
    ConcentricRingsStructurePlacement, RandomSpreadStructurePlacement, StructurePlacement,
};
use super::state::ChunkGeneratorStructureState;
use super::structure_set::StructureSetHolder;

/// `Pair<BlockPos, Holder<Structure>>`: the locate position and the structure found there.
pub type LocateResult = Option<(BlockPos, Identifier)>;

/// `getStructureGeneratingAt(structures, level, structureManager, createReference, config, chunkTarget)`: the
/// structure of the (ordered) set that generates at `chunk_target`, with its locate position (use
/// [`StructurePlacement::get_locate_pos`]).
pub type StructureGeneratingAt<'a> =
    dyn FnMut(&[Identifier], &StructurePlacement, ChunkPos) -> LocateResult + 'a;

/// The candidate chunks of one search ring of the random spread walk
/// (`getNearestGeneratedStructure(..., chunkOriginX, chunkOriginZ, radius, ..., config)`): the
/// `getPotentialStructureChunk` of every spacing-sized sector on the border of the square of the given `radius`
/// around the origin chunk, in vanilla's x-major order. Radius 0 is the origin sector alone.
pub fn random_spread_candidates(
    placement: &RandomSpreadStructurePlacement,
    seed: i64,
    chunk_origin_x: i32,
    chunk_origin_z: i32,
    radius: i32,
) -> impl Iterator<Item = ChunkPos> + '_ {
    let spacing = placement.spacing;
    (-radius..=radius).flat_map(move |x| {
        let x_edge = x == -radius || x == radius;
        (-radius..=radius).filter_map(move |z| {
            let z_edge = z == -radius || z == radius;
            (x_edge || z_edge).then(|| {
                let sector_x = chunk_origin_x.wrapping_add(spacing.wrapping_mul(x));
                let sector_z = chunk_origin_z.wrapping_add(spacing.wrapping_mul(z));
                placement.get_potential_structure_chunk(seed, sector_x, sector_z)
            })
        })
    })
}

/// `getNearestGeneratedStructure(structures, ..., chunkOriginX, chunkOriginZ, radius, ..., config)`: the first
/// candidate of the ring that generates a structure.
pub fn get_nearest_generated_structure_random_spread(
    structures: &[Identifier],
    set: &StructureSetHolder,
    config: &RandomSpreadStructurePlacement,
    seed: i64,
    chunk_origin_x: i32,
    chunk_origin_z: i32,
    radius: i32,
    generating_at: &mut StructureGeneratingAt<'_>,
) -> LocateResult {
    let placement = &set.placement;
    random_spread_candidates(config, seed, chunk_origin_x, chunk_origin_z, radius)
        .find_map(|chunk_target| generating_at(structures, placement, chunk_target))
}

/// `getNearestGeneratedStructure(structures, ..., pos, ..., rings)`: the closest generating ring position (measured
/// from `pos` to the ring chunk's centre at y 32). Panics like vanilla's `IllegalStateException` if the placement is
/// not one of the state's possible sets.
pub fn get_nearest_generated_structure_rings(
    state: &ChunkGeneratorStructureState,
    structures: &[Identifier],
    set: &StructureSetHolder,
    rings: &ConcentricRingsStructurePlacement,
    pos: BlockPos,
    generating_at: &mut StructureGeneratingAt<'_>,
) -> LocateResult {
    let positions = state
        .get_ring_positions_for(rings)
        .expect("Somehow tried to find structures for a placement that doesn't exist");
    let placement = &set.placement;
    let mut closest_pos: LocateResult = None;
    let mut closest = f64::MAX;
    for &chunk_pos in positions {
        let structure_pos = BlockPos::new(
            SectionPos::section_to_block_coord_offset(chunk_pos.x, 8),
            32,
            SectionPos::section_to_block_coord_offset(chunk_pos.z, 8),
        );
        let dist_sqr = structure_pos.dist_sqr(&pos);
        let is_closest = closest_pos.is_none() || dist_sqr < closest;
        if is_closest && let Some(generating) = generating_at(structures, placement, chunk_pos) {
            closest_pos = Some(generating);
            closest = dist_sqr;
        }
    }
    closest_pos
}

/// `ChunkGenerator.findNearestMapStructure(level, wantedStructures, pos, maxSearchRadius, createReference)` without
/// the level-bound parts: the concentric ring placements are scanned first, then the random spread placements in
/// growing rings until one ring yields a hit (the nearest of that ring wins against earlier ring hits by distance).
/// `wanted_structures` is the (ordered) `HolderSet<Structure>`.
pub fn find_nearest_map_structure(
    state: &ChunkGeneratorStructureState,
    wanted_structures: &[Identifier],
    pos: BlockPos,
    max_search_radius: i32,
    generating_at: &mut StructureGeneratingAt<'_>,
) -> LocateResult {
    // `placementScans`: placement -> wanted structures placed by it (insertion ordered, deduplicated).
    let mut placement_scans: Vec<(&StructureSetHolder, Vec<Identifier>)> = Vec::new();
    for structure in wanted_structures {
        for set in state.get_placements_for_structure(structure) {
            match placement_scans.iter_mut().find(|(s, _)| s.ptr_eq(set)) {
                Some((_, structures)) => {
                    if !structures.contains(structure) {
                        structures.push(structure.clone());
                    }
                }
                None => placement_scans.push((set, vec![structure.clone()])),
            }
        }
    }
    if placement_scans.is_empty() {
        return None;
    }

    let mut nearest: LocateResult = None;
    let mut distance_sqr = f64::MAX;
    let mut random_spread_entries = Vec::with_capacity(placement_scans.len());
    for (set, structures) in &placement_scans {
        match &set.placement {
            StructurePlacement::ConcentricRings(rings) => {
                if let Some(generating) = get_nearest_generated_structure_rings(
                    state,
                    structures,
                    set,
                    rings,
                    pos,
                    generating_at,
                ) {
                    let new_distance_sqr = pos.dist_sqr(&generating.0);
                    if new_distance_sqr < distance_sqr {
                        distance_sqr = new_distance_sqr;
                        nearest = Some(generating);
                    }
                }
            }
            StructurePlacement::RandomSpread(random_placement) => {
                random_spread_entries.push((*set, structures, random_placement));
            }
            StructurePlacement::DimensionOrigin => {}
        }
    }

    if !random_spread_entries.is_empty() {
        let chunk_origin_x = SectionPos::block_to_section_coord_i32(pos.x());
        let chunk_origin_z = SectionPos::block_to_section_coord_i32(pos.z());
        for radius in 0..=max_search_radius {
            let mut found_something = false;
            for (set, structures, random_placement) in &random_spread_entries {
                if let Some(structure_pos) = get_nearest_generated_structure_random_spread(
                    structures,
                    set,
                    random_placement,
                    state.level_seed(),
                    chunk_origin_x,
                    chunk_origin_z,
                    radius,
                    generating_at,
                ) {
                    found_something = true;
                    let new_distance_sqr = pos.dist_sqr(&structure_pos.0);
                    if new_distance_sqr < distance_sqr {
                        distance_sqr = new_distance_sqr;
                        nearest = Some(structure_pos);
                    }
                }
            }
            if found_something {
                return nearest;
            }
        }
    }
    nearest
}
