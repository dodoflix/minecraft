//! `trunkplacers.BendingTrunkPlacer`.

use super::TRUNK_PLACER_TYPES;
use super::foliage_placer::FoliageAttachment;
use super::tree_feature::{TreeFeature, TreeSetter, valid_tree_pos};
use super::trunk_placer::{TrunkBase, TrunkPlacer, place_below_trunk_block};
use mc_core::direction::{Direction, Plane};
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_game_core::world::level::levelgen::codec::{TypeEntry, parse_positive_int};
use mc_game_core::world::level::levelgen::world_gen_level::WorldGenLevel;
use mc_worldgen::provider::IntProvider;
use std::sync::Arc;

pub struct BendingTrunkPlacer {
    base: TrunkBase,
    min_height_for_leaves: i32,
    bend_length: IntProvider,
}

impl TrunkPlacer for BendingTrunkPlacer {
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
        let direction = Plane::Horizontal.get_random_direction(random);
        let log_height = tree_height - 1;
        let mut pos = origin;
        place_below_trunk_block(level, trunk_setter, random, pos.below(), tree);
        let mut foliage_points = Vec::new();

        for i in 0..=log_height {
            if i + 1 >= log_height + random.next_int_bounded(2) {
                pos = pos.relative(direction);
            }
            if valid_tree_pos(level, pos) {
                self.place_log(level, trunk_setter, random, pos, tree);
            }
            if i >= self.min_height_for_leaves {
                foliage_points.push(FoliageAttachment::new(pos, 0, false));
            }
            pos = pos.relative(Direction::Up);
        }

        let dir_length = self.bend_length.sample(random);
        for _ in 0..=dir_length {
            if valid_tree_pos(level, pos) {
                self.place_log(level, trunk_setter, random, pos, tree);
            }
            foliage_points.push(FoliageAttachment::new(pos, 0, false));
            pos = pos.relative(direction);
        }
        foliage_points
    }
}

#[linkme::distributed_slice(TRUNK_PLACER_TYPES)]
static BENDING_TRUNK_PLACER: TypeEntry<dyn TrunkPlacer> = TypeEntry {
    name: "bending_trunk_placer",
    decode: |fields, _| {
        let base = TrunkBase::decode(fields)?;
        let min_height_for_leaves = fields
            .optional("min_height_for_leaves", parse_positive_int)?
            .unwrap_or(1);
        let bend_length =
            fields.required("bend_length", |v| IntProvider::from_json_range(v, 1, 64))?;
        Ok(Arc::new(BendingTrunkPlacer {
            base,
            min_height_for_leaves,
            bend_length,
        }))
    },
};
