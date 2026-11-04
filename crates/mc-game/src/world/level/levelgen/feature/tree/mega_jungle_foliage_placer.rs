//! `foliageplacers.MegaJungleFoliagePlacer` (type `jungle_foliage_placer`).

use super::FOLIAGE_PLACER_TYPES;
use super::foliage_placer::{FoliageAttachment, FoliageBase, FoliagePlacer};
use super::tree_feature::{TreeFeature, TreeSetter};
use crate::world::level::levelgen::codec::TypeEntry;
use crate::world::level::levelgen::world_gen_level::WorldGenLevel;
use mc_core::random::RandomSource;
use std::sync::Arc;
use crate::world::level::levelgen::codec::parse_int_range;

pub struct MegaJungleFoliagePlacer {
    base: FoliageBase,
    height: i32,
}

impl FoliagePlacer for MegaJungleFoliagePlacer {
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
        let leaf_height = (if foliage_attachment.double_trunk() { foliage_height } else { 1 + random.next_int_bounded(2) }) + foliage_attachment.foliage_height_offset;
        let mut yo = offset;
        while yo >= offset - leaf_height {
            let current_radius = leaf_radius + foliage_attachment.radius_offset_xz + 1 - yo;
            self.place_leaves_row(level, foliage_setter, random, tree, foliage_attachment.pos, current_radius, yo, foliage_attachment.double_trunk());
            yo -= 1;
        }
    }

    fn foliage_height(&self, _random: &mut dyn RandomSource, _tree_height: i32, _tree: &TreeFeature) -> i32 {
        self.height
    }

    fn should_skip_location(&self, _random: &mut dyn RandomSource, dx: i32, _y: i32, dz: i32, current_radius: i32, _double_trunk: bool) -> bool {
        dx + dz >= 7 || dx * dx + dz * dz > current_radius * current_radius
    }
}

#[linkme::distributed_slice(FOLIAGE_PLACER_TYPES)]
static MEGA_JUNGLE_FOLIAGE_PLACER: TypeEntry<dyn FoliagePlacer> = TypeEntry {
    name: "jungle_foliage_placer",
    decode: |fields, _| {
        let base = FoliageBase::decode(fields)?;
        let height = fields.required("height", |v| parse_int_range(v, 0, 16))?;
        Ok(Arc::new(MegaJungleFoliagePlacer { base, height }))
    },
};
