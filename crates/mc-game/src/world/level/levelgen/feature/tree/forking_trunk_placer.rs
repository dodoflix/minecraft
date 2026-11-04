//! `trunkplacers.ForkingTrunkPlacer`.

use super::TRUNK_PLACER_TYPES;
use super::foliage_placer::FoliageAttachment;
use super::tree_feature::{TreeFeature, TreeSetter};
use super::trunk_placer::{TrunkBase, TrunkPlacer, place_below_trunk_block};
use crate::world::level::levelgen::codec::TypeEntry;
use crate::world::level::levelgen::world_gen_level::WorldGenLevel;
use mc_core::direction::Plane;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use std::sync::Arc;

pub struct ForkingTrunkPlacer {
    base: TrunkBase,
}

impl TrunkPlacer for ForkingTrunkPlacer {
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
        let mut attachments = Vec::new();
        let lean_direction = Plane::Horizontal.get_random_direction(random);
        let lean_height = tree_height - random.next_int_bounded(4) - 1;
        let mut lean_steps = 3 - random.next_int_bounded(3);
        let mut tx = origin.x();
        let mut tz = origin.z();
        let mut ey: Option<i32> = None;

        for yo in 0..tree_height {
            let yy = origin.y() + yo;
            if yo >= lean_height && lean_steps > 0 {
                tx += lean_direction.get_step_x();
                tz += lean_direction.get_step_z();
                lean_steps -= 1;
            }
            if self.place_log(level, trunk_setter, random, BlockPos::new(tx, yy, tz), tree) {
                ey = Some(yy + 1);
            }
        }
        if let Some(ey) = ey {
            attachments.push(FoliageAttachment::new(BlockPos::new(tx, ey, tz), 1, false));
        }

        tx = origin.x();
        tz = origin.z();
        let branch_direction = Plane::Horizontal.get_random_direction(random);
        if branch_direction != lean_direction {
            let branch_pos = lean_height - random.next_int_bounded(2) - 1;
            let mut branch_steps = 1 + random.next_int_bounded(3);
            ey = None;

            let mut yo = branch_pos;
            while yo < tree_height && branch_steps > 0 {
                if yo >= 1 {
                    let yy = origin.y() + yo;
                    tx += branch_direction.get_step_x();
                    tz += branch_direction.get_step_z();
                    if self.place_log(level, trunk_setter, random, BlockPos::new(tx, yy, tz), tree) {
                        ey = Some(yy + 1);
                    }
                }
                yo += 1;
                branch_steps -= 1;
            }
            if let Some(ey) = ey {
                attachments.push(FoliageAttachment::new(BlockPos::new(tx, ey, tz), 0, false));
            }
        }
        attachments
    }
}

#[linkme::distributed_slice(TRUNK_PLACER_TYPES)]
static FORKING_TRUNK_PLACER: TypeEntry<dyn TrunkPlacer> =
    TypeEntry { name: "forking_trunk_placer", decode: |fields, _| Ok(Arc::new(ForkingTrunkPlacer { base: TrunkBase::decode(fields)? })) };
