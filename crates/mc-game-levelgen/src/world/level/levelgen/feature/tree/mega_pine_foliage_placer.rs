//! `foliageplacers.MegaPineFoliagePlacer`.

use super::FOLIAGE_PLACER_TYPES;
use super::foliage_placer::{FoliageAttachment, FoliageBase, FoliagePlacer};
use super::tree_feature::{TreeFeature, TreeSetter};
use mc_core::math::mth;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_game_core::world::level::levelgen::codec::TypeEntry;
use mc_game_core::world::level::levelgen::world_gen_level::WorldGenLevel;
use mc_worldgen::provider::IntProvider;
use std::sync::Arc;

pub struct MegaPineFoliagePlacer {
    base: FoliageBase,
    crown_height: IntProvider,
}

impl FoliagePlacer for MegaPineFoliagePlacer {
    fn base(&self) -> &FoliageBase {
        &self.base
    }

    fn create_foliage_with_offset(
        &self,
        level: &mut dyn WorldGenLevel,
        foliage_setter: &mut TreeSetter,
        random: &mut dyn RandomSource,
        tree: &TreeFeature,
        _tree_height: i32,
        foliage_attachment: FoliageAttachment,
        foliage_height: i32,
        leaf_radius: i32,
        offset: i32,
    ) {
        let foliage_pos = foliage_attachment.pos;
        let mut prev_radius = 0;
        let foliage_height_with_offset = foliage_height + foliage_attachment.foliage_height_offset;

        let mut yy = foliage_pos.y() - foliage_height_with_offset + offset;
        while yy <= foliage_pos.y() + offset {
            let yo = foliage_pos.y() - yy;
            let smooth_radius = leaf_radius
                + foliage_attachment.radius_offset_xz
                + mth::floor_f32(yo as f32 / foliage_height_with_offset as f32 * 3.5f32);
            let jagged_radius = if yo > 0 && smooth_radius == prev_radius && (yy & 1) == 0 {
                smooth_radius + 1
            } else {
                smooth_radius
            };
            self.place_leaves_row(
                level,
                foliage_setter,
                random,
                tree,
                BlockPos::new(foliage_pos.x(), yy, foliage_pos.z()),
                jagged_radius,
                0,
                foliage_attachment.double_trunk(),
            );
            prev_radius = smooth_radius;
            yy += 1;
        }
    }

    fn foliage_height(
        &self,
        random: &mut dyn RandomSource,
        _tree_height: i32,
        _tree: &TreeFeature,
    ) -> i32 {
        self.crown_height.sample(random)
    }

    fn should_skip_location(
        &self,
        _random: &mut dyn RandomSource,
        dx: i32,
        _y: i32,
        dz: i32,
        current_radius: i32,
        _double_trunk: bool,
    ) -> bool {
        dx + dz >= 7 || dx * dx + dz * dz > current_radius * current_radius
    }
}

#[linkme::distributed_slice(FOLIAGE_PLACER_TYPES)]
static MEGA_PINE_FOLIAGE_PLACER: TypeEntry<dyn FoliagePlacer> = TypeEntry {
    name: "mega_pine_foliage_placer",
    decode: |fields, _| {
        let base = FoliageBase::decode(fields)?;
        let crown_height =
            fields.required("crown_height", |v| IntProvider::from_json_range(v, 0, 24))?;
        Ok(Arc::new(MegaPineFoliagePlacer { base, crown_height }))
    },
};
