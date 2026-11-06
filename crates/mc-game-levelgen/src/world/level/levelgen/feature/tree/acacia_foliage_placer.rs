//! `foliageplacers.AcaciaFoliagePlacer`.

use super::FOLIAGE_PLACER_TYPES;
use super::foliage_placer::{FoliageAttachment, FoliageBase, FoliagePlacer};
use super::tree_feature::{TreeFeature, TreeSetter};
use mc_core::random::RandomSource;
use mc_game_core::world::level::levelgen::codec::TypeEntry;
use mc_game_core::world::level::levelgen::world_gen_level::WorldGenLevel;
use std::sync::Arc;

pub struct AcaciaFoliagePlacer {
    base: FoliageBase,
}

impl FoliagePlacer for AcaciaFoliagePlacer {
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
        let double_trunk = foliage_attachment.double_trunk();
        let foliage_pos = foliage_attachment.pos.above_n(offset);
        let foliage_height_with_offset = foliage_height + foliage_attachment.foliage_height_offset;
        self.place_leaves_row(
            level,
            foliage_setter,
            random,
            tree,
            foliage_pos,
            leaf_radius + foliage_attachment.radius_offset_xz,
            -1 - foliage_height_with_offset,
            double_trunk,
        );
        self.place_leaves_row(
            level,
            foliage_setter,
            random,
            tree,
            foliage_pos,
            leaf_radius - 1,
            -foliage_height_with_offset,
            double_trunk,
        );
        self.place_leaves_row(
            level,
            foliage_setter,
            random,
            tree,
            foliage_pos,
            leaf_radius + foliage_attachment.radius_offset_xz - 1,
            0,
            double_trunk,
        );
    }

    fn foliage_height(
        &self,
        _random: &mut dyn RandomSource,
        _tree_height: i32,
        _tree: &TreeFeature,
    ) -> i32 {
        0
    }

    fn should_skip_location(
        &self,
        _random: &mut dyn RandomSource,
        dx: i32,
        y: i32,
        dz: i32,
        current_radius: i32,
        _double_trunk: bool,
    ) -> bool {
        if y == 0 {
            (dx > 1 || dz > 1) && dx != 0 && dz != 0
        } else {
            dx == current_radius && dz == current_radius && current_radius > 0
        }
    }
}

#[linkme::distributed_slice(FOLIAGE_PLACER_TYPES)]
static ACACIA_FOLIAGE_PLACER: TypeEntry<dyn FoliagePlacer> = TypeEntry {
    name: "acacia_foliage_placer",
    decode: |fields, _| {
        Ok(Arc::new(AcaciaFoliagePlacer {
            base: FoliageBase::decode(fields)?,
        }))
    },
};
