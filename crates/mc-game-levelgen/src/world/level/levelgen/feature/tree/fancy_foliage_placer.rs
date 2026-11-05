//! `foliageplacers.FancyFoliagePlacer` (extends `BlobFoliagePlacer`: same height).

use super::FOLIAGE_PLACER_TYPES;
use super::blob_foliage_placer::decode_blob_parts;
use super::foliage_placer::{FoliageAttachment, FoliageBase, FoliagePlacer};
use super::tree_feature::{TreeFeature, TreeSetter};
use mc_game_core::world::level::levelgen::codec::TypeEntry;
use mc_game_core::world::level::levelgen::world_gen_level::WorldGenLevel;
use mc_core::math::mth;
use mc_core::random::RandomSource;
use std::sync::Arc;

pub struct FancyFoliagePlacer {
    base: FoliageBase,
    height: i32,
}

impl FoliagePlacer for FancyFoliagePlacer {
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
        let mut yo = offset;
        while yo >= offset - foliage_height {
            let current_radius = leaf_radius + i32::from(yo != offset && yo != offset - foliage_height);
            self.place_leaves_row(level, foliage_setter, random, tree, foliage_attachment.pos, current_radius, yo, foliage_attachment.double_trunk());
            yo -= 1;
        }
    }

    fn foliage_height(&self, _random: &mut dyn RandomSource, _tree_height: i32, _tree: &TreeFeature) -> i32 {
        self.height
    }

    fn should_skip_location(&self, _random: &mut dyn RandomSource, dx: i32, _y: i32, dz: i32, current_radius: i32, _double_trunk: bool) -> bool {
        mth::square_f32(dx as f32 + 0.5f32) + mth::square_f32(dz as f32 + 0.5f32) > current_radius.wrapping_mul(current_radius) as f32
    }
}

#[linkme::distributed_slice(FOLIAGE_PLACER_TYPES)]
static FANCY_FOLIAGE_PLACER: TypeEntry<dyn FoliagePlacer> = TypeEntry {
    name: "fancy_foliage_placer",
    decode: |fields, _| {
        let (base, height) = decode_blob_parts(fields)?;
        Ok(Arc::new(FancyFoliagePlacer { base, height }))
    },
};
