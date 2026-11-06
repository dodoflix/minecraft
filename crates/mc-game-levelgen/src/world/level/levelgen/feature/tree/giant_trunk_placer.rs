//! `trunkplacers.GiantTrunkPlacer`.

use super::TRUNK_PLACER_TYPES;
use super::foliage_placer::FoliageAttachment;
use super::tree_feature::{TreeFeature, TreeSetter};
use super::trunk_placer::{TrunkBase, TrunkPlacer, place_below_trunk_block};
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_game_core::world::level::levelgen::codec::TypeEntry;
use mc_game_core::world::level::levelgen::world_gen_level::WorldGenLevel;
use std::sync::Arc;

pub struct GiantTrunkPlacer {
    base: TrunkBase,
}

impl GiantTrunkPlacer {
    pub fn new(base: TrunkBase) -> Self {
        GiantTrunkPlacer { base }
    }
}

/// `GiantTrunkPlacer.placeTrunk` for `placer` (shared with the `MegaJungleTrunkPlacer` subclass).
pub fn place_giant_trunk(
    placer: &dyn TrunkPlacer,
    level: &mut dyn WorldGenLevel,
    trunk_setter: &mut TreeSetter,
    random: &mut dyn RandomSource,
    tree_height: i32,
    origin: BlockPos,
    tree: &TreeFeature,
) -> Vec<FoliageAttachment> {
    let below = origin.below();
    place_below_trunk_block(level, trunk_setter, random, below, tree);
    place_below_trunk_block(level, trunk_setter, random, below.east(), tree);
    place_below_trunk_block(level, trunk_setter, random, below.south(), tree);
    place_below_trunk_block(level, trunk_setter, random, below.south().east(), tree);

    for hh in 0..tree_height {
        placer.place_log_if_free(level, trunk_setter, random, origin.offset(0, hh, 0), tree);
        if hh < tree_height - 1 {
            placer.place_log_if_free(level, trunk_setter, random, origin.offset(1, hh, 0), tree);
            placer.place_log_if_free(level, trunk_setter, random, origin.offset(1, hh, 1), tree);
            placer.place_log_if_free(level, trunk_setter, random, origin.offset(0, hh, 1), tree);
        }
    }
    vec![FoliageAttachment::new(origin.above_n(tree_height), 0, true)]
}

impl TrunkPlacer for GiantTrunkPlacer {
    fn base(&self) -> &TrunkBase {
        &self.base
    }

    fn place_trunk(
        &self,
        level: &mut dyn WorldGenLevel,
        trunk_setter: &mut TreeSetter,
        random: &mut dyn RandomSource,
        tree_height: i32,
        origin: BlockPos,
        tree: &TreeFeature,
    ) -> Vec<FoliageAttachment> {
        place_giant_trunk(self, level, trunk_setter, random, tree_height, origin, tree)
    }
}

#[linkme::distributed_slice(TRUNK_PLACER_TYPES)]
static GIANT_TRUNK_PLACER: TypeEntry<dyn TrunkPlacer> = TypeEntry {
    name: "giant_trunk_placer",
    decode: |fields, _| Ok(Arc::new(GiantTrunkPlacer::new(TrunkBase::decode(fields)?))),
};
