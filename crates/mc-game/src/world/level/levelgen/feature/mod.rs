//! `net.minecraft.world.level.levelgen.feature`: the `Feature` interface (26.3: a data-driven `worldgen/feature` entry is
//! the feature itself, config included), the type table and shared helpers.
//!
//! Each feature type lives in its own file under this module and registers itself:
//! ```ignore
//! #[linkme::distributed_slice(FEATURE_TYPES)]
//! static ORE: TypeEntry<dyn Feature> = TypeEntry { name: "ore", decode: OreFeature::decode };
//! ```
//! Area modules (`tree`, ...) add their own `pub mod` lines to this file.

pub mod end_gateway_feature;
pub mod end_island_feature;
pub mod end_platform_feature;
pub mod end_podium_feature;
pub mod end_spike_feature;
pub mod geode_feature;
pub mod large_dripstone_feature;
pub mod monster_room_feature;
pub mod speleothem_cluster_feature;
pub mod speleothem_feature;
pub mod speleothem_utils;
pub mod spike_feature;

use super::codec::{LoadResult, TypeEntry, decode_dispatch};
use super::env::GenEnv;
use super::holder::Holder;
use super::registries::DecodeCtx;
use super::world_gen_level::WorldGenLevel;
use mc_core::direction::Direction;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::BlockState;
use serde_json::Value;
use std::sync::Arc;

pub mod fill_layer_feature;
pub mod block_blob_feature;
pub mod cuboid_placement;
pub mod disk_feature;
pub mod iceberg_feature;
pub mod lake_feature;
pub mod random_boolean_selector_feature;
pub mod random_selector_feature;
pub mod replace_blobs_feature;
pub mod simple_random_selector_feature;
pub mod single_block_pillar_feature;
pub mod spring_feature;
pub mod stepped_column_cluster_feature;
pub mod weighted_placed_feature;
pub mod weighted_random_selector_feature;

/// `Feature`.
pub trait Feature: Send + Sync {
    /// `place(level, chunkGenerator, random, origin)`.
    fn place(&self, level: &mut dyn WorldGenLevel, generator: &GenEnv, random: &mut dyn RandomSource, origin: BlockPos) -> bool;

    /// `getSubFeatures()`.
    fn get_sub_features(&self) -> Vec<Holder<dyn Feature>> {
        Vec::new()
    }
}

#[linkme::distributed_slice]
pub static FEATURE_TYPES: [TypeEntry<dyn Feature>];

/// `Feature.DIRECT_CODEC`.
pub fn decode_direct(value: &Value, ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn Feature>> {
    decode_dispatch(value, ctx, &FEATURE_TYPES)
}

/// `Feature.setBlock(level, pos, state)`.
pub fn set_block(level: &mut dyn WorldGenLevel, pos: BlockPos, state: BlockState) {
    level.set_block_and_update(pos, state);
}

/// `Feature.safeSetBlock(level, pos, state, canReplace)`.
pub fn safe_set_block(level: &mut dyn WorldGenLevel, pos: BlockPos, state: BlockState, can_replace: impl Fn(BlockState) -> bool) {
    if can_replace(level.get_block_state(pos)) {
        level.set_block(pos, state, 2, 512);
    }
}

/// `Feature.markAboveForPostProcessing(level, placePos)`.
pub fn mark_above_for_post_processing(level: &mut dyn WorldGenLevel, place_pos: BlockPos) {
    let mut pos = place_pos;
    for _ in 0..2 {
        pos = pos.relative(Direction::Up);
        if level.get_block_state(pos).is_air() {
            return;
        }
        level.mark_pos_for_post_processing(pos);
    }
}

/// Registered by `no_op` (kept here so the table is never empty and the pipeline has a trivially testable feature).
pub struct NoOpFeature;

impl Feature for NoOpFeature {
    fn place(&self, _level: &mut dyn WorldGenLevel, _generator: &GenEnv, _random: &mut dyn RandomSource, _origin: BlockPos) -> bool {
        true
    }
}

#[linkme::distributed_slice(FEATURE_TYPES)]
static NO_OP: TypeEntry<dyn Feature> = TypeEntry { name: "no_op", decode: |_, _| Ok(Arc::new(NoOpFeature)) };
