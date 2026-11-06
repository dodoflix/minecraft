//! `trunkplacers.PoplarTrunkPlacer`.

use super::TRUNK_PLACER_TYPES;
use super::foliage_placer::FoliageAttachment;
use super::tree_feature::{TreeFeature, TreeSetter};
use super::trunk_placer::{TrunkBase, TrunkPlacer, place_below_trunk_block, with_axis};
use mc_core::direction::Direction;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_game_core::world::level::levelgen::codec::TypeEntry;
use mc_game_core::world::level::levelgen::world_gen_level::WorldGenLevel;
use mc_worldgen::provider::IntProvider;
use std::sync::Arc;

pub struct PoplarTrunkPlacer {
    base: TrunkBase,
    trunk_height_above_branches: IntProvider,
    branch_amount: IntProvider,
}

impl TrunkPlacer for PoplarTrunkPlacer {
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
        let trunk_height_up_to_foliage_branches =
            tree_height - self.trunk_height_above_branches.sample(random);

        for y in 0..tree_height {
            self.place_log(level, trunk_setter, random, origin.above_n(y), tree);
            let directions: Vec<Direction> = Direction::all_shuffled(random)
                .into_iter()
                .filter(|d| !d.get_axis().is_vertical())
                .collect();
            if trunk_height_up_to_foliage_branches - 1 == y {
                let branches = self.branch_amount.sample(random);
                for x in 0..branches {
                    let branch_direction = directions[x as usize];
                    let axis = branch_direction.get_axis();
                    self.place_log_with(
                        level,
                        trunk_setter,
                        random,
                        origin.above_n(y).relative_n(branch_direction, 1),
                        tree,
                        &|state| with_axis(state, axis),
                    );
                }
            }
        }
        vec![FoliageAttachment::new(
            origin.above_n(trunk_height_up_to_foliage_branches),
            0,
            false,
        )]
    }
}

#[linkme::distributed_slice(TRUNK_PLACER_TYPES)]
static POPLAR_TRUNK_PLACER: TypeEntry<dyn TrunkPlacer> = TypeEntry {
    name: "poplar_trunk_placer",
    decode: |fields, _| {
        let base = TrunkBase::decode(fields)?;
        let trunk_height_above_branches = fields.required("trunk_height_above_branches", |v| {
            IntProvider::from_json_range(v, 0, 8)
        })?;
        let branch_amount =
            fields.required("branch_amount", |v| IntProvider::from_json_range(v, 1, 4))?;
        Ok(Arc::new(PoplarTrunkPlacer {
            base,
            trunk_height_above_branches,
            branch_amount,
        }))
    },
};
