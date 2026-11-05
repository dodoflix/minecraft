// bridge: remove after branch migration (docs/crate-split.md#removal)
//! `mc-game-levelgen`: re-exports the lower crate's module tree and adds this crate's modules.

pub use parent::world::level::levelgen::blockpredicates::*;

pub mod all_of_predicate;

pub mod any_of_predicate;

pub mod combining_predicate;

pub mod has_sturdy_face_predicate;

pub mod height_range_predicate;

pub mod inside_world_bounds_predicate;

pub mod matching_biomes_predicate;

pub mod matching_block_tag_predicate;

pub mod matching_blocks_predicate;

pub mod matching_fluids_predicate;

pub mod not_predicate;

pub mod replaceable_predicate;

pub mod solid_predicate;

pub mod true_block_predicate;

pub use true_block_predicate::TrueBlockPredicate;

pub mod unobstructed_predicate;

pub mod volume_match_predicate;

pub mod would_survive_predicate;

use parent::world::level::levelgen::codec::{BlockSet, FluidSet};
use mc_core::direction::Direction;
use mc_core::pos::Vec3i;
use mc_data::block::Block;
use mc_data::fluid::Fluid;
use mc_worldgen::vertical_anchor::VerticalAnchor;
use std::sync::Arc;

/// `allOf(predicates)`.
pub fn all_of(predicates: Vec<Arc<dyn BlockPredicate>>) -> Arc<dyn BlockPredicate> {
    Arc::new(all_of_predicate::AllOfPredicate { predicates })
}

/// `anyOf(predicates)`.
pub fn any_of(predicates: Vec<Arc<dyn BlockPredicate>>) -> Arc<dyn BlockPredicate> {
    Arc::new(any_of_predicate::AnyOfPredicate { predicates })
}

/// `matchesBlocks(offset, blocks)`.
pub fn matches_blocks(offset: Vec3i, blocks: Vec<Block>) -> Arc<dyn BlockPredicate> {
    Arc::new(matching_blocks_predicate::MatchingBlocksPredicate::new(offset, BlockSet::Blocks(blocks)))
}

/// `matchesTag(offset, tag)` (`tag`: `minecraft:air`).
pub fn matches_tag(offset: Vec3i, tag: &str) -> Arc<dyn BlockPredicate> {
    Arc::new(matching_block_tag_predicate::MatchingBlockTagPredicate::new(offset, tag))
}

/// `matchesFluids(offset, fluids)`.
pub fn matches_fluids(offset: Vec3i, fluids: Vec<Fluid>) -> Arc<dyn BlockPredicate> {
    Arc::new(matching_fluids_predicate::MatchingFluidsPredicate::new(offset, FluidSet::Fluids(fluids)))
}

/// `not(predicate)`.
pub fn not(predicate: Arc<dyn BlockPredicate>) -> Arc<dyn BlockPredicate> {
    Arc::new(not_predicate::NotPredicate { predicate })
}

/// `replaceable()`.
pub fn replaceable() -> Arc<dyn BlockPredicate> {
    Arc::new(replaceable_predicate::ReplaceablePredicate { offset: Vec3i::ZERO })
}

/// `wouldSurvive(block)`.
pub fn would_survive(block: Block) -> Arc<dyn BlockPredicate> {
    Arc::new(would_survive_predicate::WouldSurvivePredicate { offset: Vec3i::ZERO, state: block.default_state() })
}

/// `hasSturdyFace(offset, direction)`.
pub fn has_sturdy_face(offset: Vec3i, direction: Direction) -> Arc<dyn BlockPredicate> {
    Arc::new(has_sturdy_face_predicate::HasSturdyFacePredicate { offset, direction })
}

/// `solid(offset)`.
pub fn solid(offset: Vec3i) -> Arc<dyn BlockPredicate> {
    Arc::new(solid_predicate::SolidPredicate { offset })
}

/// `noFluid()`.
pub fn no_fluid() -> Arc<dyn BlockPredicate> {
    matches_fluids(Vec3i::ZERO, vec![Fluid::EMPTY])
}

/// `insideWorld(offset)`.
pub fn inside_world(offset: Vec3i) -> Arc<dyn BlockPredicate> {
    Arc::new(inside_world_bounds_predicate::InsideWorldBoundsPredicate { offset })
}

/// `alwaysTrue()`.
pub fn always_true() -> Arc<dyn BlockPredicate> {
    Arc::new(TrueBlockPredicate)
}

/// `unobstructed()`.
pub fn unobstructed() -> Arc<dyn BlockPredicate> {
    Arc::new(unobstructed_predicate::UnobstructedPredicate { offset: Vec3i::ZERO })
}

/// `heightRange(minInclusive, maxInclusive)`.
pub fn height_range(min_inclusive: VerticalAnchor, max_inclusive: VerticalAnchor) -> Arc<dyn BlockPredicate> {
    Arc::new(height_range_predicate::HeightRangePredicate { min_inclusive, max_inclusive })
}

/// `volumeMatch(min, max, match)`.
pub fn volume_match(min: Vec3i, max: Vec3i, match_: Arc<dyn BlockPredicate>) -> Arc<dyn BlockPredicate> {
    Arc::new(volume_match_predicate::VolumeMatchPredicate { min, max, match_ })
}

/// `ONLY_IN_AIR_PREDICATE` = `matchesTag(BlockTags.AIR)`.
pub fn only_in_air_predicate() -> Arc<dyn BlockPredicate> {
    matches_tag(Vec3i::ZERO, "minecraft:air")
}

/// `ONLY_IN_AIR_OR_WATER_PREDICATE` = `anyOf(ONLY_IN_AIR_PREDICATE, matchesBlocks(Blocks.WATER))`.
pub fn only_in_air_or_water_predicate() -> Arc<dyn BlockPredicate> {
    any_of(vec![only_in_air_predicate(), matches_blocks(Vec3i::ZERO, vec![Block::WATER])])
}
