//! `trunkplacers.CherryTrunkPlacer`.

use super::TRUNK_PLACER_TYPES;
use super::foliage_placer::FoliageAttachment;
use super::tree_feature::{TreeFeature, TreeSetter};
use super::trunk_placer::{TrunkBase, TrunkPlacer, place_below_trunk_block, with_axis};
use crate::world::level::levelgen::codec::{Fields, LoadError, LoadResult, TypeEntry, parse_int};
use crate::world::level::levelgen::world_gen_level::WorldGenLevel;
use mc_core::direction::{Direction, Plane};
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_worldgen::provider::IntProvider;
use serde_json::Value;
use std::sync::Arc;

pub struct CherryTrunkPlacer {
    base: TrunkBase,
    branch_count: IntProvider,
    branch_horizontal_length: IntProvider,
    branch_start_offset_from_top: IntProvider,
    second_branch_start_offset_from_top: IntProvider,
    branch_end_offset_from_top: IntProvider,
}

/// `IntProviders.validateCodec(-16, 0, BRANCH_START_CODEC)`: a `UniformInt` map (min/max fields, no `type`) with at least
/// two blocks of variation, inside `[-16, 0]`.
fn parse_branch_start(value: &Value) -> LoadResult<(i32, i32)> {
    let fields = Fields::new(value)?;
    let min = fields.required("min_inclusive", parse_int)?;
    let max = fields.required("max_inclusive", parse_int)?;
    if max < min {
        return Err(LoadError::new(format!("Max must be at least min, min_inclusive: {min}, max_inclusive: {max}")));
    }
    if max.wrapping_sub(min) < 1 {
        return Err(LoadError::new("Need at least 2 blocks variation for the branch starts to fit both branches"));
    }
    if min < -16 {
        return Err(LoadError::new(format!("Value provider too low: -16 [{min}-{max}]")));
    }
    if max > 0 {
        return Err(LoadError::new(format!("Value provider too high: 0 [{min}-{max}]")));
    }
    Ok((min, max))
}

impl CherryTrunkPlacer {
    #[allow(clippy::too_many_arguments)]
    fn generate_branch(
        &self,
        level: &mut dyn WorldGenLevel,
        trunk_setter: &mut TreeSetter,
        random: &mut dyn RandomSource,
        tree_height: i32,
        origin: BlockPos,
        tree: &TreeFeature,
        sideways: &dyn Fn(mc_data::block::BlockState) -> mc_data::block::BlockState,
        branch_direction: Direction,
        offset_from_origin: i32,
        middle_continues_upwards: bool,
    ) -> FoliageAttachment {
        let mut log_pos = origin.relative_n(Direction::Up, offset_from_origin);
        let branch_end_pos_offset_from_origin = tree_height - 1 + self.branch_end_offset_from_top.sample(random);
        let extend_branch_away_from_trunk = middle_continues_upwards || branch_end_pos_offset_from_origin < offset_from_origin;
        let distance_to_trunk = self.branch_horizontal_length.sample(random) + i32::from(extend_branch_away_from_trunk);
        let branch_end_pos = origin.relative_n(branch_direction, distance_to_trunk).above_n(branch_end_pos_offset_from_origin);
        let steps_horizontally = if extend_branch_away_from_trunk { 2 } else { 1 };

        for _ in 0..steps_horizontally {
            log_pos = log_pos.relative(branch_direction);
            self.place_log_with(level, trunk_setter, random, log_pos, tree, sideways);
        }

        let vertical_direction = if branch_end_pos.y() > log_pos.y() { Direction::Up } else { Direction::Down };
        loop {
            let distance = log_pos.dist_manhattan(&branch_end_pos);
            if distance == 0 {
                return FoliageAttachment::new(branch_end_pos.above(), 0, false);
            }
            let chance_to_grow_vertically = (branch_end_pos.y() - log_pos.y()).abs() as f32 / distance as f32;
            let grow_vertically = random.next_float() < chance_to_grow_vertically;
            log_pos = log_pos.relative(if grow_vertically { vertical_direction } else { branch_direction });
            if grow_vertically {
                self.place_log(level, trunk_setter, random, log_pos, tree);
            } else {
                self.place_log_with(level, trunk_setter, random, log_pos, tree, sideways);
            }
        }
    }
}

impl TrunkPlacer for CherryTrunkPlacer {
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
        let first_branch_offset_from_origin = 0.max(tree_height - 1 + self.branch_start_offset_from_top.sample(random));
        let mut second_branch_offset_from_origin = 0.max(tree_height - 1 + self.second_branch_start_offset_from_top.sample(random));
        if second_branch_offset_from_origin >= first_branch_offset_from_origin {
            second_branch_offset_from_origin += 1;
        }

        let branch_count = self.branch_count.sample(random);
        let has_middle_branch = branch_count == 3;
        let has_both_side_branches = branch_count >= 2;
        let trunk_height = if has_middle_branch {
            tree_height
        } else if has_both_side_branches {
            first_branch_offset_from_origin.max(second_branch_offset_from_origin) + 1
        } else {
            first_branch_offset_from_origin + 1
        };

        for y in 0..trunk_height {
            self.place_log(level, trunk_setter, random, origin.above_n(y), tree);
        }

        let mut attachments = Vec::new();
        if has_middle_branch {
            attachments.push(FoliageAttachment::new(origin.above_n(trunk_height), 0, false));
        }

        let tree_direction = Plane::Horizontal.get_random_direction(random);
        let axis = tree_direction.get_axis();
        let sideways = move |state| with_axis(state, axis);
        attachments.push(self.generate_branch(
            level,
            trunk_setter,
            random,
            tree_height,
            origin,
            tree,
            &sideways,
            tree_direction,
            first_branch_offset_from_origin,
            first_branch_offset_from_origin < trunk_height - 1,
        ));
        if has_both_side_branches {
            attachments.push(self.generate_branch(
                level,
                trunk_setter,
                random,
                tree_height,
                origin,
                tree,
                &sideways,
                tree_direction.get_opposite(),
                second_branch_offset_from_origin,
                second_branch_offset_from_origin < trunk_height - 1,
            ));
        }
        attachments
    }
}

#[linkme::distributed_slice(TRUNK_PLACER_TYPES)]
static CHERRY_TRUNK_PLACER: TypeEntry<dyn TrunkPlacer> = TypeEntry {
    name: "cherry_trunk_placer",
    decode: |fields, _| {
        let base = TrunkBase::decode(fields)?;
        let branch_count = fields.required("branch_count", |v| IntProvider::from_json_range(v, 1, 3))?;
        let branch_horizontal_length = fields.required("branch_horizontal_length", |v| IntProvider::from_json_range(v, 2, 16))?;
        let (start_min, start_max) = fields.required("branch_start_offset_from_top", parse_branch_start)?;
        let branch_end_offset_from_top = fields.required("branch_end_offset_from_top", |v| IntProvider::from_json_range(v, -16, 16))?;
        Ok(Arc::new(CherryTrunkPlacer {
            base,
            branch_count,
            branch_horizontal_length,
            branch_start_offset_from_top: IntProvider::Uniform { min_inclusive: start_min, max_inclusive: start_max },
            second_branch_start_offset_from_top: IntProvider::Uniform { min_inclusive: start_min, max_inclusive: start_max - 1 },
            branch_end_offset_from_top,
        }))
    },
};
