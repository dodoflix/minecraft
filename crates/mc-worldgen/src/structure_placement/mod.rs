//! The block-behaviour-free part of vanilla's structure set placement: `StructurePlacement`
//! (`random_spread`, `concentric_rings`, `dimension_origin`), `StructureSet`, `ChunkGeneratorStructureState`
//! (possible sets, stronghold ring positions) and the candidate-walking part of `findNearestMapStructure`.
//!
//! Structures themselves are only referenced by [`Identifier`](mc_core::ident::Identifier). Gaps against vanilla:
//! a structure set entry may not be an inline structure definition (vanilla's `Structure.CODEC` accepts one), and
//! `Structure.biomes()` is supplied by the caller (see [`ChunkGeneratorStructureState::create_for_normal`]).

mod locate;
mod placement;
mod state;
mod structure_set;

pub use locate::{
    LocateResult, StructureGeneratingAt, find_nearest_map_structure, get_nearest_generated_structure_random_spread,
    get_nearest_generated_structure_rings, random_spread_candidates,
};
pub use placement::{
    ConcentricRingsStructurePlacement, ExclusionZone, FrequencyReductionMethod, RandomSpreadStructurePlacement,
    RandomSpreadType, SpreadingPlacement, StructurePlacement,
};
pub use state::ChunkGeneratorStructureState;
pub use structure_set::{
    STRUCTURE_REGISTRY, STRUCTURE_SET_REGISTRY, StructureSelectionEntry, StructureSet, StructureSetHolder,
    StructureSetRegistry, structure_biomes,
};
