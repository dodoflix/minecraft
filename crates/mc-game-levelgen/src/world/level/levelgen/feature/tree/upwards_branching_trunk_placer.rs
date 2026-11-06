//! `trunkplacers.UpwardsBranchingTrunkPlacer`.

use super::TRUNK_PLACER_TYPES;
use super::foliage_placer::FoliageAttachment;
use super::tree_feature::{TreeFeature, TreeSetter, valid_tree_pos};
use super::trunk_placer::{TrunkBase, TrunkPlacer};
use mc_core::direction::{Direction, Plane};
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_game_core::world::level::LevelAccessor;
use mc_game_core::world::level::levelgen::codec::{BlockSet, TypeEntry, parse_float_range};
use mc_game_core::world::level::levelgen::world_gen_level::WorldGenLevel;
use mc_worldgen::provider::IntProvider;
use std::sync::Arc;

pub struct UpwardsBranchingTrunkPlacer {
    base: TrunkBase,
    extra_branch_steps: IntProvider,
    place_branch_per_log_probability: f32,
    extra_branch_length: IntProvider,
    can_grow_through: BlockSet,
}

impl UpwardsBranchingTrunkPlacer {
    fn place_branch(
        &self,
        level: &mut dyn WorldGenLevel,
        trunk_setter: &mut TreeSetter,
        random: &mut dyn RandomSource,
        tree_height: i32,
        tree: &TreeFeature,
        attachments: &mut Vec<FoliageAttachment>,
        log_pos: BlockPos,
        current_height: i32,
        branch_dir: Direction,
        branch_pos: i32,
        mut branch_steps: i32,
    ) {
        let mut height_along_branch = current_height + branch_pos;
        let mut log_x = log_pos.x();
        let mut log_z = log_pos.z();
        let mut branch_placement_index = branch_pos;

        while branch_placement_index < tree_height && branch_steps > 0 {
            if branch_placement_index >= 1 {
                let placement_height = current_height + branch_placement_index;
                log_x += branch_dir.get_step_x();
                log_z += branch_dir.get_step_z();
                height_along_branch = placement_height;
                let pos = BlockPos::new(log_x, placement_height, log_z);
                if self.place_log(level, trunk_setter, random, pos, tree) {
                    height_along_branch = placement_height + 1;
                }
                attachments.push(FoliageAttachment::new(pos, 0, false));
            }
            branch_placement_index += 1;
            branch_steps -= 1;
        }

        if height_along_branch - current_height > 1 {
            let foliage_pos = BlockPos::new(log_x, height_along_branch, log_z);
            attachments.push(FoliageAttachment::new(foliage_pos, 0, false));
            attachments.push(FoliageAttachment::new(foliage_pos.below_n(2), 0, false));
        }
    }
}

impl TrunkPlacer for UpwardsBranchingTrunkPlacer {
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
        for height_pos in 0..tree_height {
            let current_height = origin.y() + height_pos;
            let log_pos = BlockPos::new(origin.x(), current_height, origin.z());
            if self.place_log(level, trunk_setter, random, log_pos, tree)
                && height_pos < tree_height - 1
                && random.next_float() < self.place_branch_per_log_probability
            {
                let branch_dir = Plane::Horizontal.get_random_direction(random);
                let branch_len = self.extra_branch_length.sample(random);
                let branch_pos = 0.max(branch_len - self.extra_branch_length.sample(random) - 1);
                let branch_steps = self.extra_branch_steps.sample(random);
                self.place_branch(
                    level,
                    trunk_setter,
                    random,
                    tree_height,
                    tree,
                    &mut attachments,
                    log_pos,
                    current_height,
                    branch_dir,
                    branch_pos,
                    branch_steps,
                );
            }
            if height_pos == tree_height - 1 {
                attachments.push(FoliageAttachment::new(
                    BlockPos::new(origin.x(), current_height + 1, origin.z()),
                    0,
                    false,
                ));
            }
        }
        attachments
    }

    fn valid_tree_pos(&self, level: &dyn LevelAccessor, pos: BlockPos) -> bool {
        valid_tree_pos(level, pos)
            || self
                .can_grow_through
                .contains(level.get_block_state(pos).block())
    }
}

#[linkme::distributed_slice(TRUNK_PLACER_TYPES)]
static UPWARDS_BRANCHING_TRUNK_PLACER: TypeEntry<dyn TrunkPlacer> = TypeEntry {
    name: "upwards_branching_trunk_placer",
    decode: |fields, _| {
        let base = TrunkBase::decode(fields)?;
        let extra_branch_steps =
            fields.required("extra_branch_steps", IntProvider::positive_from_json)?;
        let place_branch_per_log_probability = fields
            .required("place_branch_per_log_probability", |v| {
                parse_float_range(v, 0.0, 1.0)
            })?;
        let extra_branch_length =
            fields.required("extra_branch_length", IntProvider::non_negative_from_json)?;
        let can_grow_through = fields.required("can_grow_through", BlockSet::from_json)?;
        Ok(Arc::new(UpwardsBranchingTrunkPlacer {
            base,
            extra_branch_steps,
            place_branch_per_log_probability,
            extra_branch_length,
            can_grow_through,
        }))
    },
};
