//! `foliageplacers.BlobFoliagePlacer`.

use super::FOLIAGE_PLACER_TYPES;
use super::foliage_placer::{FoliageAttachment, FoliageBase, FoliagePlacer};
use super::tree_feature::{TreeFeature, TreeSetter};
use crate::world::level::levelgen::codec::{Fields, LoadResult, TypeEntry, parse_int_range};
use crate::world::level::levelgen::world_gen_level::WorldGenLevel;
use mc_core::random::RandomSource;
use std::sync::Arc;

pub struct BlobFoliagePlacer {
    base: FoliageBase,
    height: i32,
}

/// `blobParts(instance)`.
pub fn decode_blob_parts(fields: &Fields<'_>) -> LoadResult<(FoliageBase, i32)> {
    let base = FoliageBase::decode(fields)?;
    let height = fields.required("height", |v| parse_int_range(v, 0, 16))?;
    Ok((base, height))
}

impl BlobFoliagePlacer {
    pub fn new(base: FoliageBase, height: i32) -> Self {
        BlobFoliagePlacer { base, height }
    }
}

impl FoliagePlacer for BlobFoliagePlacer {
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
            let current_radius = (leaf_radius + foliage_attachment.radius_offset_xz - 1 - yo / 2).max(0);
            self.place_leaves_row(level, foliage_setter, random, tree, foliage_attachment.pos, current_radius, yo, foliage_attachment.double_trunk());
            yo -= 1;
        }
    }

    fn foliage_height(&self, _random: &mut dyn RandomSource, _tree_height: i32, _tree: &TreeFeature) -> i32 {
        self.height
    }

    fn should_skip_location(&self, random: &mut dyn RandomSource, dx: i32, y: i32, dz: i32, current_radius: i32, _double_trunk: bool) -> bool {
        dx == current_radius && dz == current_radius && (random.next_int_bounded(2) == 0 || y == 0)
    }
}

#[linkme::distributed_slice(FOLIAGE_PLACER_TYPES)]
static BLOB_FOLIAGE_PLACER: TypeEntry<dyn FoliagePlacer> = TypeEntry {
    name: "blob_foliage_placer",
    decode: |fields, _| {
        let (base, height) = decode_blob_parts(fields)?;
        Ok(Arc::new(BlobFoliagePlacer::new(base, height)))
    },
};
