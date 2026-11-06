//! `foliageplacers.CherryFoliagePlacer`.

use super::FOLIAGE_PLACER_TYPES;
use super::foliage_placer::{FoliageAttachment, FoliageBase, FoliagePlacer};
use super::tree_feature::{TreeFeature, TreeSetter};
use mc_core::random::RandomSource;
use mc_game_core::world::level::levelgen::codec::TypeEntry;
use mc_game_core::world::level::levelgen::codec::parse_float_range;
use mc_game_core::world::level::levelgen::world_gen_level::WorldGenLevel;
use mc_worldgen::provider::IntProvider;
use std::sync::Arc;

pub struct CherryFoliagePlacer {
    base: FoliageBase,
    height: IntProvider,
    wide_bottom_layer_hole_chance: f32,
    corner_hole_chance: f32,
    hanging_leaves_chance: f32,
    hanging_leaves_extension_chance: f32,
}

impl FoliagePlacer for CherryFoliagePlacer {
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
        let current_radius = leaf_radius + foliage_attachment.radius_offset_xz - 1;
        let foliage_height_with_offset = foliage_height + foliage_attachment.foliage_height_offset;
        self.place_leaves_row(
            level,
            foliage_setter,
            random,
            tree,
            foliage_pos,
            current_radius - 2,
            foliage_height_with_offset - 3,
            double_trunk,
        );
        self.place_leaves_row(
            level,
            foliage_setter,
            random,
            tree,
            foliage_pos,
            current_radius - 1,
            foliage_height_with_offset - 4,
            double_trunk,
        );

        let mut y = foliage_height_with_offset - 5;
        while y >= 0 {
            self.place_leaves_row(
                level,
                foliage_setter,
                random,
                tree,
                foliage_pos,
                current_radius,
                y,
                double_trunk,
            );
            y -= 1;
        }

        self.place_leaves_row_with_hanging_leaves_below(
            level,
            foliage_setter,
            random,
            tree,
            foliage_pos,
            current_radius,
            -1,
            double_trunk,
            self.hanging_leaves_chance,
            self.hanging_leaves_extension_chance,
        );
        self.place_leaves_row_with_hanging_leaves_below(
            level,
            foliage_setter,
            random,
            tree,
            foliage_pos,
            current_radius - 1,
            -2,
            double_trunk,
            self.hanging_leaves_chance,
            self.hanging_leaves_extension_chance,
        );
    }

    fn foliage_height(
        &self,
        random: &mut dyn RandomSource,
        _tree_height: i32,
        _tree: &TreeFeature,
    ) -> i32 {
        self.height.sample(random)
    }

    fn should_skip_location(
        &self,
        random: &mut dyn RandomSource,
        dx: i32,
        y: i32,
        dz: i32,
        current_radius: i32,
        _double_trunk: bool,
    ) -> bool {
        if y == -1
            && (dx == current_radius || dz == current_radius)
            && random.next_float() < self.wide_bottom_layer_hole_chance
        {
            return true;
        }
        let corner = dx == current_radius && dz == current_radius;
        let wide_layer = current_radius > 2;
        if wide_layer {
            corner
                || dx + dz > current_radius * 2 - 2 && random.next_float() < self.corner_hole_chance
        } else {
            corner && random.next_float() < self.corner_hole_chance
        }
    }
}

#[linkme::distributed_slice(FOLIAGE_PLACER_TYPES)]
static CHERRY_FOLIAGE_PLACER: TypeEntry<dyn FoliagePlacer> = TypeEntry {
    name: "cherry_foliage_placer",
    decode: |fields, _| {
        let base = FoliageBase::decode(fields)?;
        let height = fields.required("height", |v| IntProvider::from_json_range(v, 4, 16))?;
        let chance = |name: &str| fields.required(name, |v| parse_float_range(v, 0.0, 1.0));
        Ok(Arc::new(CherryFoliagePlacer {
            base,
            height,
            wide_bottom_layer_hole_chance: chance("wide_bottom_layer_hole_chance")?,
            corner_hole_chance: chance("corner_hole_chance")?,
            hanging_leaves_chance: chance("hanging_leaves_chance")?,
            hanging_leaves_extension_chance: chance("hanging_leaves_extension_chance")?,
        }))
    },
};
