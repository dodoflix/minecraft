//! `foliageplacers.BushFoliagePlacer` (extends `BlobFoliagePlacer`: same height).

use super::FOLIAGE_PLACER_TYPES;
use super::blob_foliage_placer::decode_blob_parts;
use super::foliage_placer::{FoliageAttachment, FoliageBase, FoliagePlacer};
use super::tree_feature::{TreeFeature, TreeSetter};
use mc_core::random::RandomSource;
use mc_game_core::world::level::levelgen::codec::TypeEntry;
use mc_game_core::world::level::levelgen::world_gen_level::WorldGenLevel;
use std::sync::Arc;

pub struct BushFoliagePlacer {
    base: FoliageBase,
    height: i32,
}

impl FoliagePlacer for BushFoliagePlacer {
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
        let foliage_height_with_offset = foliage_height + foliage_attachment.foliage_height_offset;
        let mut yo = offset;
        while yo >= offset - foliage_height_with_offset {
            let current_radius = leaf_radius + foliage_attachment.radius_offset_xz - 1 - yo;
            self.place_leaves_row(
                level,
                foliage_setter,
                random,
                tree,
                foliage_attachment.pos,
                current_radius,
                yo,
                foliage_attachment.double_trunk(),
            );
            yo -= 1;
        }
    }

    fn foliage_height(
        &self,
        _random: &mut dyn RandomSource,
        _tree_height: i32,
        _tree: &TreeFeature,
    ) -> i32 {
        self.height
    }

    fn should_skip_location(
        &self,
        random: &mut dyn RandomSource,
        dx: i32,
        _y: i32,
        dz: i32,
        current_radius: i32,
        _double_trunk: bool,
    ) -> bool {
        dx == current_radius && dz == current_radius && random.next_int_bounded(2) == 0
    }
}

#[linkme::distributed_slice(FOLIAGE_PLACER_TYPES)]
static BUSH_FOLIAGE_PLACER: TypeEntry<dyn FoliagePlacer> = TypeEntry {
    name: "bush_foliage_placer",
    decode: |fields, _| {
        let (base, height) = decode_blob_parts(fields)?;
        Ok(Arc::new(BushFoliagePlacer { base, height }))
    },
};
