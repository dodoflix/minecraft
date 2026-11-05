//! `trunkplacers.MegaJungleTrunkPlacer`.

use super::TRUNK_PLACER_TYPES;
use super::foliage_placer::FoliageAttachment;
use super::giant_trunk_placer::place_giant_trunk;
use super::tree_feature::{TreeFeature, TreeSetter};
use super::trunk_placer::{TrunkBase, TrunkPlacer};
use mc_game_core::world::level::levelgen::codec::TypeEntry;
use mc_game_core::world::level::levelgen::world_gen_level::WorldGenLevel;
use mc_core::math::mth;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use std::sync::Arc;

pub struct MegaJungleTrunkPlacer {
    base: TrunkBase,
}

impl TrunkPlacer for MegaJungleTrunkPlacer {
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
        let mut attachments = place_giant_trunk(self, level, trunk_setter, random, tree_height, origin, tree);

        let mut branch_height = tree_height - 2 - random.next_int_bounded(4);
        while branch_height > tree_height / 2 {
            let angle = random.next_float() * (std::f64::consts::PI * 2.0) as f32;
            let mut bx = 0;
            let mut bz = 0;
            for b in 0..5 {
                bx = (1.5f32 + mth::cos(f64::from(angle)) * b as f32) as i32;
                bz = (1.5f32 + mth::sin(f64::from(angle)) * b as f32) as i32;
                let pos = origin.offset(bx, branch_height - 3 + b / 2, bz);
                self.place_log(level, trunk_setter, random, pos, tree);
            }
            attachments.push(FoliageAttachment::new(origin.offset(bx, branch_height, bz), -2, false));
            branch_height -= 2 + random.next_int_bounded(4);
        }
        attachments
    }
}

#[linkme::distributed_slice(TRUNK_PLACER_TYPES)]
static MEGA_JUNGLE_TRUNK_PLACER: TypeEntry<dyn TrunkPlacer> = TypeEntry {
    name: "mega_jungle_trunk_placer",
    decode: |fields, _| Ok(Arc::new(MegaJungleTrunkPlacer { base: TrunkBase::decode(fields)? })),
};
