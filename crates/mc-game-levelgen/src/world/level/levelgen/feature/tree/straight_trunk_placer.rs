//! `trunkplacers.StraightTrunkPlacer`.

use super::TRUNK_PLACER_TYPES;
use super::foliage_placer::FoliageAttachment;
use super::tree_feature::{TreeFeature, TreeSetter};
use super::trunk_placer::{TrunkBase, TrunkPlacer, place_below_trunk_block};
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_game_core::world::level::levelgen::codec::TypeEntry;
use mc_game_core::world::level::levelgen::world_gen_level::WorldGenLevel;
use std::sync::Arc;

pub struct StraightTrunkPlacer {
    base: TrunkBase,
}

impl TrunkPlacer for StraightTrunkPlacer {
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
        place_below_trunk_block(level, trunk_setter, random, origin.below(), tree);
        for y in 0..tree_height {
            self.place_log(level, trunk_setter, random, origin.above_n(y), tree);
        }
        vec![FoliageAttachment::new(
            origin.above_n(tree_height),
            0,
            false,
        )]
    }
}

#[linkme::distributed_slice(TRUNK_PLACER_TYPES)]
static STRAIGHT_TRUNK_PLACER: TypeEntry<dyn TrunkPlacer> = TypeEntry {
    name: "straight_trunk_placer",
    decode: |fields, _| {
        Ok(Arc::new(StraightTrunkPlacer {
            base: TrunkBase::decode(fields)?,
        }))
    },
};
