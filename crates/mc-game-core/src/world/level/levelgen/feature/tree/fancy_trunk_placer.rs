//! `trunkplacers.FancyTrunkPlacer`.

use super::TRUNK_PLACER_TYPES;
use super::foliage_placer::FoliageAttachment;
use super::tree_feature::{TreeFeature, TreeSetter};
use super::trunk_placer::{TrunkBase, TrunkPlacer, place_below_trunk_block, with_axis};
use crate::world::level::levelgen::codec::TypeEntry;
use crate::world::level::levelgen::world_gen_level::WorldGenLevel;
use mc_core::direction::Axis;
use mc_core::math::mth;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use std::sync::Arc;

const TRUNK_HEIGHT_SCALE: f64 = 0.618;
const CLUSTER_DENSITY_MAGIC: f64 = 1.382;
const BRANCH_SLOPE: f64 = 0.381;
const BRANCH_LENGTH_MAGIC: f64 = 0.328;

pub struct FancyTrunkPlacer {
    base: TrunkBase,
}

/// `FancyTrunkPlacer.FoliageCoords`.
struct FoliageCoords {
    attachment: FoliageAttachment,
    branch_base: i32,
}

impl FancyTrunkPlacer {
    #[allow(clippy::too_many_arguments)]
    fn make_limb(
        &self,
        level: &mut dyn WorldGenLevel,
        trunk_setter: &mut TreeSetter,
        random: &mut dyn RandomSource,
        start_pos: BlockPos,
        end_pos: BlockPos,
        do_place: bool,
        tree: &TreeFeature,
    ) -> bool {
        if !do_place && start_pos == end_pos {
            return true;
        }
        let delta = end_pos.offset(start_pos.x().wrapping_neg(), start_pos.y().wrapping_neg(), start_pos.z().wrapping_neg());
        let steps = Self::get_steps(delta);
        let dx = delta.x() as f32 / steps as f32;
        let dy = delta.y() as f32 / steps as f32;
        let dz = delta.z() as f32 / steps as f32;

        for i in 0..=steps {
            let block_pos = start_pos.offset(
                mth::floor_f32(0.5f32 + i as f32 * dx),
                mth::floor_f32(0.5f32 + i as f32 * dy),
                mth::floor_f32(0.5f32 + i as f32 * dz),
            );
            if do_place {
                let axis = Self::get_log_axis(start_pos, block_pos);
                self.place_log_with(level, trunk_setter, random, block_pos, tree, &|state| with_axis(state, axis));
            } else if !self.is_free(level, block_pos) {
                return false;
            }
        }
        true
    }

    fn get_steps(pos: BlockPos) -> i32 {
        let abs_x = pos.x().wrapping_abs();
        let abs_y = pos.y().wrapping_abs();
        let abs_z = pos.z().wrapping_abs();
        abs_x.max(abs_y.max(abs_z))
    }

    fn get_log_axis(start_pos: BlockPos, block_pos: BlockPos) -> Axis {
        let mut axis = Axis::Y;
        let xdiff = (block_pos.x() - start_pos.x()).abs();
        let zdiff = (block_pos.z() - start_pos.z()).abs();
        let maxdiff = xdiff.max(zdiff);
        if maxdiff > 0 {
            axis = if xdiff == maxdiff { Axis::X } else { Axis::Z };
        }
        axis
    }

    fn trim_branches(height: i32, local_y: i32) -> bool {
        f64::from(local_y) >= f64::from(height) * 0.2
    }

    fn make_branches(
        &self,
        level: &mut dyn WorldGenLevel,
        trunk_setter: &mut TreeSetter,
        random: &mut dyn RandomSource,
        height: i32,
        origin: BlockPos,
        foliage_coords: &[FoliageCoords],
        tree: &TreeFeature,
    ) {
        for end_coord in foliage_coords {
            let branch_base = end_coord.branch_base;
            let base_coord = BlockPos::new(origin.x(), branch_base, origin.z());
            if base_coord != end_coord.attachment.pos && Self::trim_branches(height, branch_base - origin.y()) {
                self.make_limb(level, trunk_setter, random, base_coord, end_coord.attachment.pos, true, tree);
            }
        }
    }

    fn tree_shape(height: i32, y: i32) -> f32 {
        if (y as f32) < height as f32 * 0.3f32 {
            return -1.0;
        }
        let radius = height as f32 / 2.0f32;
        let adjacent = radius - y as f32;
        let mut distance = mth::sqrt(radius * radius - adjacent * adjacent);
        if adjacent == 0.0 {
            distance = radius;
        } else if adjacent.abs() >= radius {
            return 0.0;
        }
        distance * 0.5f32
    }
}

impl TrunkPlacer for FancyTrunkPlacer {
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
        let height = tree_height + 2;
        let trunk_height = mth::floor_f64(f64::from(height) * TRUNK_HEIGHT_SCALE);
        place_below_trunk_block(level, trunk_setter, random, origin.below(), tree);
        let ratio = 1.0 * f64::from(height) / 13.0;
        let clusters_per_y = 1.min(mth::floor_f64(CLUSTER_DENSITY_MAGIC + ratio * ratio));
        let trunk_top = origin.y() + trunk_height;
        let mut relative_y = height - 5;
        let mut foliage_coords = vec![FoliageCoords { attachment: FoliageAttachment::new(origin.above_n(relative_y), 0, false), branch_base: trunk_top }];

        while relative_y >= 0 {
            let tree_shape = Self::tree_shape(height, relative_y);
            if !(tree_shape < 0.0) {
                for _ in 0..clusters_per_y {
                    let radius = 1.0 * f64::from(tree_shape) * (f64::from(random.next_float()) + BRANCH_LENGTH_MAGIC);
                    let angle = f64::from(random.next_float() * 2.0f32) * std::f64::consts::PI;
                    let x = radius * angle.sin() + 0.5;
                    let z = radius * angle.cos() + 0.5;
                    let check_start = origin.offset(mth::floor_f64(x), relative_y - 1, mth::floor_f64(z));
                    let check_end = check_start.above_n(5);
                    if self.make_limb(level, trunk_setter, random, check_start, check_end, false, tree) {
                        let dx = origin.x() - check_start.x();
                        let dz = origin.z() - check_start.z();
                        let branch_height = f64::from(check_start.y()) - f64::from(dx.wrapping_mul(dx).wrapping_add(dz.wrapping_mul(dz))).sqrt() * BRANCH_SLOPE;
                        let branch_top = if branch_height > f64::from(trunk_top) { trunk_top } else { branch_height as i32 };
                        let check_branch_base = BlockPos::new(origin.x(), branch_top, origin.z());
                        if self.make_limb(level, trunk_setter, random, check_branch_base, check_start, false, tree) {
                            foliage_coords.push(FoliageCoords { attachment: FoliageAttachment::new(check_start, 0, false), branch_base: check_branch_base.y() });
                        }
                    }
                }
            }
            relative_y -= 1;
        }

        self.make_limb(level, trunk_setter, random, origin, origin.above_n(trunk_height), true, tree);
        self.make_branches(level, trunk_setter, random, height, origin, &foliage_coords, tree);
        foliage_coords
            .iter()
            .filter(|coord| Self::trim_branches(height, coord.branch_base - origin.y()))
            .map(|coord| coord.attachment)
            .collect()
    }
}

#[linkme::distributed_slice(TRUNK_PLACER_TYPES)]
static FANCY_TRUNK_PLACER: TypeEntry<dyn TrunkPlacer> =
    TypeEntry { name: "fancy_trunk_placer", decode: |fields, _| Ok(Arc::new(FancyTrunkPlacer { base: TrunkBase::decode(fields)? })) };
