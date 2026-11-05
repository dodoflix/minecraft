//! `trunkplacers.DarkOakTrunkPlacer`.

use super::TRUNK_PLACER_TYPES;
use super::foliage_placer::FoliageAttachment;
use super::tree_feature::{TreeFeature, TreeSetter, is_air_or_leaves};
use super::trunk_placer::{TrunkBase, TrunkPlacer, place_below_trunk_block};
use crate::world::level::levelgen::codec::TypeEntry;
use crate::world::level::levelgen::world_gen_level::WorldGenLevel;
use mc_core::direction::Plane;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use std::sync::Arc;

pub struct DarkOakTrunkPlacer {
    base: TrunkBase,
}

impl TrunkPlacer for DarkOakTrunkPlacer {
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
        let mut attachments = Vec::new();
        let below = origin.below();
        place_below_trunk_block(level, trunk_setter, random, below, tree);
        place_below_trunk_block(level, trunk_setter, random, below.east(), tree);
        place_below_trunk_block(level, trunk_setter, random, below.south(), tree);
        place_below_trunk_block(level, trunk_setter, random, below.south().east(), tree);
        let lean_direction = Plane::Horizontal.get_random_direction(random);
        let lean_height = tree_height - random.next_int_bounded(4);
        let mut lean_steps = 2 - random.next_int_bounded(3);
        let x = origin.x();
        let y = origin.y();
        let z = origin.z();
        let mut tx = x;
        let mut tz = z;
        let ey = y + tree_height - 1;

        for dy in 0..tree_height {
            if dy >= lean_height && lean_steps > 0 {
                tx += lean_direction.get_step_x();
                tz += lean_direction.get_step_z();
                lean_steps -= 1;
            }
            let yy = y + dy;
            let block_pos = BlockPos::new(tx, yy, tz);
            if is_air_or_leaves(level, block_pos) {
                self.place_log(level, trunk_setter, random, block_pos, tree);
                self.place_log(level, trunk_setter, random, block_pos.east(), tree);
                self.place_log(level, trunk_setter, random, block_pos.south(), tree);
                self.place_log(level, trunk_setter, random, block_pos.east().south(), tree);
            }
        }

        attachments.push(FoliageAttachment::new(BlockPos::new(tx, ey, tz), 0, true));

        for ox in -1..=2 {
            for oz in -1..=2 {
                if (!(0..=1).contains(&ox) || !(0..=1).contains(&oz)) && random.next_int_bounded(3) <= 0 {
                    let length = random.next_int_bounded(3) + 2;
                    for branch_y in 0..length {
                        self.place_log(level, trunk_setter, random, BlockPos::new(x + ox, ey - branch_y - 1, z + oz), tree);
                    }
                    attachments.push(FoliageAttachment::new(BlockPos::new(x + ox, ey, z + oz), 0, false));
                }
            }
        }
        attachments
    }
}

#[linkme::distributed_slice(TRUNK_PLACER_TYPES)]
static DARK_OAK_TRUNK_PLACER: TypeEntry<dyn TrunkPlacer> =
    TypeEntry { name: "dark_oak_trunk_placer", decode: |fields, _| Ok(Arc::new(DarkOakTrunkPlacer { base: TrunkBase::decode(fields)? })) };
