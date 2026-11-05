//! `foliageplacers.PineFoliagePlacer`.

use super::FOLIAGE_PLACER_TYPES;
use super::foliage_placer::{FoliageAttachment, FoliageBase, FoliagePlacer};
use super::tree_feature::{TreeFeature, TreeSetter};
use mc_game_core::world::level::levelgen::codec::TypeEntry;
use mc_game_core::world::level::levelgen::world_gen_level::WorldGenLevel;
use mc_core::random::RandomSource;
use std::sync::Arc;
use mc_worldgen::provider::IntProvider;

pub struct PineFoliagePlacer {
    base: FoliageBase,
    height: IntProvider,
}

impl FoliagePlacer for PineFoliagePlacer {
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
        let mut current_radius = 0;
        let foliage_height_with_offset = foliage_height + foliage_attachment.foliage_height_offset;

        let mut yo = offset;
        while yo >= offset - foliage_height_with_offset {
            self.place_leaves_row(level, foliage_setter, random, tree, foliage_attachment.pos, current_radius, yo, foliage_attachment.double_trunk());
            if current_radius >= 1 && yo == offset - foliage_height_with_offset + 1 {
                current_radius -= 1;
            } else if current_radius < leaf_radius + foliage_attachment.radius_offset_xz {
                current_radius += 1;
            }
            yo -= 1;
        }
    }

    fn foliage_radius(&self, random: &mut dyn RandomSource, trunk_height: i32) -> i32 {
        let radius = self.base.radius.sample(random);
        radius + random.next_int_bounded((trunk_height + 1).max(1))
    }

    fn foliage_height(&self, random: &mut dyn RandomSource, _tree_height: i32, _tree: &TreeFeature) -> i32 {
        self.height.sample(random)
    }

    fn should_skip_location(&self, _random: &mut dyn RandomSource, dx: i32, _y: i32, dz: i32, current_radius: i32, _double_trunk: bool) -> bool {
        dx == current_radius && dz == current_radius && current_radius > 0
    }
}

#[linkme::distributed_slice(FOLIAGE_PLACER_TYPES)]
static PINE_FOLIAGE_PLACER: TypeEntry<dyn FoliagePlacer> = TypeEntry {
    name: "pine_foliage_placer",
    decode: |fields, _| {
        let base = FoliageBase::decode(fields)?;
        let height = fields.required("height", |v| IntProvider::from_json_range(v, 0, 24))?;
        Ok(Arc::new(PineFoliagePlacer { base, height }))
    },
};
