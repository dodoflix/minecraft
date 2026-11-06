//! `foliageplacers.PoplarFoliagePlacer`.

use super::FOLIAGE_PLACER_TYPES;
use super::foliage_placer::{FoliageAttachment, FoliageBase, FoliagePlacer, try_place_leaf};
use super::tree_feature::{TreeFeature, TreeSetter};
use super::trunk_placer::with_axis;
use mc_core::direction::Axis;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_game_core::world::level::levelgen::codec::{TypeEntry, parse_float_range};
use mc_game_core::world::level::levelgen::world_gen_level::WorldGenLevel;
use mc_worldgen::provider::IntProvider;
use std::sync::Arc;

pub struct PoplarFoliagePlacer {
    base: FoliageBase,
    height: IntProvider,
    side_hole_chance: f32,
}

impl PoplarFoliagePlacer {
    #[allow(clippy::too_many_arguments)]
    fn replace_leaves_with_log(
        &self,
        level: &mut dyn WorldGenLevel,
        foliage_setter: &mut TreeSetter,
        tree: &TreeFeature,
        random: &mut dyn RandomSource,
        origin: BlockPos,
        current_radius: i32,
        y: i32,
        double_trunk: bool,
        foliage_height: i32,
        flip_rhombus_shape: bool,
    ) {
        let offset = i32::from(double_trunk);
        for dx in -current_radius..=current_radius + offset {
            for dz in -current_radius..=current_radius + offset {
                let abs_dz = dz.abs();
                let abs_dx = dx.abs();
                if is_within_rhombus_shape(
                    current_radius,
                    abs_dx,
                    abs_dz,
                    Self::corner_blocks_to_cut_for_rhombus_shape(
                        dx,
                        dz,
                        current_radius,
                        Self::should_row_be_partial_rhombus_shape(foliage_height, y),
                        flip_rhombus_shape,
                    ),
                    2,
                ) && (abs_dz == 0 && current_radius - abs_dx >= 4
                    || abs_dx == 0 && current_radius - abs_dz >= 4)
                {
                    let axis = if abs_dz == 0 { Axis::X } else { Axis::Z };
                    try_place_log(
                        level,
                        foliage_setter,
                        random,
                        tree,
                        origin.offset(dx, y, dz),
                        axis,
                    );
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn place_leaves_row_shaped(
        &self,
        level: &mut dyn WorldGenLevel,
        foliage_setter: &mut TreeSetter,
        random: &mut dyn RandomSource,
        tree: &TreeFeature,
        origin: BlockPos,
        current_radius: i32,
        y: i32,
        double_trunk: bool,
        foliage_height: i32,
        flip_rhombus_shape: bool,
    ) {
        let offset = i32::from(double_trunk);
        for dx in -current_radius..=current_radius + offset {
            for dz in -current_radius..=current_radius + offset {
                if !self.should_skip_location_shaped(
                    random,
                    dx,
                    y,
                    dz,
                    current_radius,
                    foliage_height,
                    flip_rhombus_shape,
                ) {
                    try_place_leaf(
                        level,
                        foliage_setter,
                        random,
                        tree,
                        origin.offset(dx, y, dz),
                    );
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn should_skip_location_shaped(
        &self,
        random: &mut dyn RandomSource,
        dx: i32,
        y: i32,
        dz: i32,
        current_radius: i32,
        foliage_height: i32,
        flip_rhombus_shape: bool,
    ) -> bool {
        let should_row_be_partial_rhombus_shape =
            Self::should_row_be_partial_rhombus_shape(foliage_height, y);
        let corner_blocks_to_cut = Self::corner_blocks_to_cut_for_rhombus_shape(
            dx,
            dz,
            current_radius,
            should_row_be_partial_rhombus_shape,
            flip_rhombus_shape,
        );
        let abs_dx = dx.abs();
        let abs_dz = dz.abs();
        let is_rhombus_edge_block = abs_dx == current_radius || abs_dz == current_radius;
        if should_row_be_partial_rhombus_shape && is_rhombus_edge_block {
            true
        } else {
            let additional_side_removal = i32::from(random.next_float() <= self.side_hole_chance);
            !is_within_rhombus_shape(
                current_radius,
                abs_dx,
                abs_dz,
                corner_blocks_to_cut,
                additional_side_removal,
            )
        }
    }

    fn corner_blocks_to_cut_for_rhombus_shape(
        dx: i32,
        dz: i32,
        current_radius: i32,
        should_row_be_partial_rhombus_shape: bool,
        flip_rhombus_shape: bool,
    ) -> i32 {
        let is_small_corner_of_shape = if flip_rhombus_shape {
            is_left_top_corner_or_right_lower_corner(dx, dz)
        } else {
            is_left_lower_corner_or_right_top_corner(dx, dz)
        };
        if is_small_corner_of_shape {
            current_radius - 1
        } else if should_row_be_partial_rhombus_shape {
            current_radius + 1
        } else {
            current_radius
        }
    }

    fn should_row_be_partial_rhombus_shape(foliage_height: i32, y: i32) -> bool {
        foliage_height - 1 == y || foliage_height - 2 == y
    }
}

fn try_place_log(
    level: &mut dyn WorldGenLevel,
    foliage_setter: &mut TreeSetter,
    random: &mut dyn RandomSource,
    tree: &TreeFeature,
    pos: BlockPos,
    axis: Axis,
) {
    let current = level.get_block_state(pos);
    // the predicate always evaluates the foliage provider (it may consume randomness)
    if current == tree.foliage_provider.value().get_state(level, random, pos) {
        let state = tree.trunk_provider.value().get_state(level, random, pos);
        foliage_setter.set(level, pos, with_axis(state, axis));
    }
}

fn is_within_rhombus_shape(
    current_radius: i32,
    abs_dx: i32,
    abs_dz: i32,
    corner_blocks_to_cut_for_rhombus_shape: i32,
    additional_side_removal: i32,
) -> bool {
    abs_dx + abs_dz
        <= current_radius * 2 - (corner_blocks_to_cut_for_rhombus_shape + additional_side_removal)
}

fn is_left_lower_corner_or_right_top_corner(dx: i32, dz: i32) -> bool {
    dx > 0 && dz < 0 || dz > 0 && dx < 0
}

fn is_left_top_corner_or_right_lower_corner(dx: i32, dz: i32) -> bool {
    dx > 0 && dz > 0 || dz < 0 && dx < 0
}

impl FoliagePlacer for PoplarFoliagePlacer {
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
        let flip = random.next_boolean();
        let h = foliage_height + foliage_attachment.foliage_height_offset;
        self.place_leaves_row_shaped(
            level,
            foliage_setter,
            random,
            tree,
            foliage_pos,
            current_radius - 2,
            h - 1,
            double_trunk,
            h,
            flip,
        );
        self.place_leaves_row_shaped(
            level,
            foliage_setter,
            random,
            tree,
            foliage_pos,
            current_radius - 1,
            h - 2,
            double_trunk,
            h,
            flip,
        );
        self.place_leaves_row_shaped(
            level,
            foliage_setter,
            random,
            tree,
            foliage_pos,
            current_radius - 1,
            h - 3,
            double_trunk,
            h,
            flip,
        );

        let mut y = h - 4;
        while y >= 1 {
            self.place_leaves_row_shaped(
                level,
                foliage_setter,
                random,
                tree,
                foliage_pos,
                current_radius,
                y,
                double_trunk,
                h,
                flip,
            );
            y -= 1;
        }

        self.replace_leaves_with_log(
            level,
            foliage_setter,
            tree,
            random,
            foliage_pos,
            current_radius,
            h - 4,
            double_trunk,
            h,
            flip,
        );
        self.place_leaves_row_shaped(
            level,
            foliage_setter,
            random,
            tree,
            foliage_pos,
            current_radius - 1,
            0,
            double_trunk,
            h,
            flip,
        );
        self.place_leaves_row_shaped(
            level,
            foliage_setter,
            random,
            tree,
            foliage_pos,
            (current_radius - 2).clamp(1, 2),
            -1,
            double_trunk,
            h,
            flip,
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

    fn should_skip_location_signed(
        &self,
        _random: &mut dyn RandomSource,
        _dx: i32,
        _y: i32,
        _dz: i32,
        _current_radius: i32,
        _double_trunk: bool,
    ) -> bool {
        panic!("Overridden method needs more context")
    }

    fn should_skip_location(
        &self,
        _random: &mut dyn RandomSource,
        _dx: i32,
        _y: i32,
        _dz: i32,
        _current_radius: i32,
        _double_trunk: bool,
    ) -> bool {
        panic!("Overridden method needs more context")
    }
}

#[linkme::distributed_slice(FOLIAGE_PLACER_TYPES)]
static POPLAR_FOLIAGE_PLACER: TypeEntry<dyn FoliagePlacer> = TypeEntry {
    name: "poplar_foliage_placer",
    decode: |fields, _| {
        let base = FoliageBase::decode(fields)?;
        let height = fields.required("height", |v| IntProvider::from_json_range(v, 5, 16))?;
        let side_hole_chance =
            fields.required("side_hole_chance", |v| parse_float_range(v, 0.0, 1.0))?;
        Ok(Arc::new(PoplarFoliagePlacer {
            base,
            height,
            side_hole_chance,
        }))
    },
};
