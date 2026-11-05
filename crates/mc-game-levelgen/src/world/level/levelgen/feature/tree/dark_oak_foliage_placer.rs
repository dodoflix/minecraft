//! `foliageplacers.DarkOakFoliagePlacer`.

use super::FOLIAGE_PLACER_TYPES;
use super::foliage_placer::{FoliageAttachment, FoliageBase, FoliagePlacer};
use super::tree_feature::{TreeFeature, TreeSetter};
use mc_game_core::world::level::levelgen::codec::TypeEntry;
use mc_game_core::world::level::levelgen::world_gen_level::WorldGenLevel;
use mc_core::random::RandomSource;
use std::sync::Arc;

pub struct DarkOakFoliagePlacer {
    base: FoliageBase,
}

impl FoliagePlacer for DarkOakFoliagePlacer {
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
        _foliage_height: i32,
        leaf_radius: i32,
        offset: i32,
    ) {
        let pos = foliage_attachment.pos.above_n(offset);
        let double_trunk = foliage_attachment.double_trunk();
        if double_trunk {
            self.place_leaves_row(level, foliage_setter, random, tree, pos, leaf_radius + 2, -1, double_trunk);
            self.place_leaves_row(level, foliage_setter, random, tree, pos, leaf_radius + 3, 0, double_trunk);
            self.place_leaves_row(level, foliage_setter, random, tree, pos, leaf_radius + 2, 1, double_trunk);
            if random.next_boolean() {
                self.place_leaves_row(level, foliage_setter, random, tree, pos, leaf_radius, 2, double_trunk);
            }
        } else {
            self.place_leaves_row(level, foliage_setter, random, tree, pos, leaf_radius + 2, -1, double_trunk);
            self.place_leaves_row(level, foliage_setter, random, tree, pos, leaf_radius + 1, 0, double_trunk);
        }
    }

    fn foliage_height(&self, _random: &mut dyn RandomSource, _tree_height: i32, _tree: &TreeFeature) -> i32 {
        4
    }

    fn should_skip_location_signed(&self, random: &mut dyn RandomSource, dx: i32, y: i32, dz: i32, current_radius: i32, double_trunk: bool) -> bool {
        if y != 0 || !double_trunk || dx != -current_radius && dx < current_radius || dz != -current_radius && dz < current_radius {
            // super.shouldSkipLocationSigned
            let (min_dx, min_dz) = if double_trunk { (dx.abs().min((dx - 1).abs()), dz.abs().min((dz - 1).abs())) } else { (dx.abs(), dz.abs()) };
            self.should_skip_location(random, min_dx, y, min_dz, current_radius, double_trunk)
        } else {
            true
        }
    }

    fn should_skip_location(&self, _random: &mut dyn RandomSource, dx: i32, y: i32, dz: i32, current_radius: i32, double_trunk: bool) -> bool {
        if y == -1 && !double_trunk {
            dx == current_radius && dz == current_radius
        } else {
            y == 1 && dx + dz > current_radius * 2 - 2
        }
    }
}

#[linkme::distributed_slice(FOLIAGE_PLACER_TYPES)]
static DARK_OAK_FOLIAGE_PLACER: TypeEntry<dyn FoliagePlacer> =
    TypeEntry { name: "dark_oak_foliage_placer", decode: |fields, _| Ok(Arc::new(DarkOakFoliagePlacer { base: FoliageBase::decode(fields)? })) };
