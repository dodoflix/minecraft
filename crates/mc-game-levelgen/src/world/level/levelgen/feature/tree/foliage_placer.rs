//! `foliageplacers.FoliagePlacer`: the abstract class as a trait whose provided methods are vanilla's concrete ones.

use super::tree_feature::{self, TreeFeature, TreeSetter, fluid_is_water_source};
use crate::world::level::levelgen::codec::{Fields, LoadResult};
use crate::world::level::levelgen::world_gen_level::WorldGenLevel;
use mc_core::direction::{AxisDirection, Direction, Plane};
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::props;
use mc_worldgen::provider::IntProvider;

/// `FoliagePlacer.FoliageAttachment`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FoliageAttachment {
    pub pos: BlockPos,
    pub radius_offset_xz: i32,
    pub foliage_height_offset: i32,
    pub size_x: i32,
    pub size_z: i32,
}

impl FoliageAttachment {
    /// `FoliageAttachment(pos, radiusOffsetXZ, doubleTrunk)`.
    pub fn new(pos: BlockPos, radius_offset_xz: i32, double_trunk: bool) -> Self {
        let size_xz = if double_trunk { 2 } else { 1 };
        Self::with_size(pos, radius_offset_xz, size_xz, size_xz)
    }

    /// `FoliageAttachment(pos, radiusOffsetXZ, sizeX, sizeZ)`.
    pub fn with_size(pos: BlockPos, radius_offset_xz: i32, size_x: i32, size_z: i32) -> Self {
        FoliageAttachment { pos, radius_offset_xz, foliage_height_offset: 0, size_x, size_z }
    }

    pub fn double_trunk(&self) -> bool {
        self.size_x == 2 && self.size_z == 2
    }
}

/// The fields every foliage placer shares (`foliagePlacerParts`).
#[derive(Clone, Debug)]
pub struct FoliageBase {
    pub radius: IntProvider,
    pub offset: IntProvider,
}

impl FoliageBase {
    pub fn decode(fields: &Fields<'_>) -> LoadResult<FoliageBase> {
        Ok(FoliageBase {
            radius: fields.required("radius", |v| IntProvider::from_json_range(v, 0, 16))?,
            offset: fields.required("offset", |v| IntProvider::from_json_range(v, 0, 16))?,
        })
    }
}

pub trait FoliagePlacer: Send + Sync {
    fn base(&self) -> &FoliageBase;

    /// The protected abstract `createFoliage(.., offset)`.
    #[allow(clippy::too_many_arguments)]
    fn create_foliage_with_offset(
        &self,
        level: &mut dyn WorldGenLevel,
        foliage_setter: &mut TreeSetter,
        random: &mut dyn RandomSource,
        tree: &TreeFeature,
        tree_height: i32,
        foliage_attachment: FoliageAttachment,
        foliage_height: i32,
        leaf_radius: i32,
        offset: i32,
    );

    /// `foliageHeight(random, treeHeight, tree)`.
    fn foliage_height(&self, random: &mut dyn RandomSource, tree_height: i32, tree: &TreeFeature) -> i32;

    /// `shouldSkipLocation(random, dx, y, dz, currentRadius, doubleTrunk)`.
    fn should_skip_location(&self, random: &mut dyn RandomSource, dx: i32, y: i32, dz: i32, current_radius: i32, double_trunk: bool) -> bool;

    /// `createFoliage(level, foliageSetter, random, tree, treeHeight, attachment, foliageHeight, leafRadius)`.
    #[allow(clippy::too_many_arguments)]
    fn create_foliage(
        &self,
        level: &mut dyn WorldGenLevel,
        foliage_setter: &mut TreeSetter,
        random: &mut dyn RandomSource,
        tree: &TreeFeature,
        tree_height: i32,
        foliage_attachment: FoliageAttachment,
        foliage_height: i32,
        leaf_radius: i32,
    ) {
        let offset = self.base().offset.sample(random);
        self.create_foliage_with_offset(level, foliage_setter, random, tree, tree_height, foliage_attachment, foliage_height, leaf_radius, offset);
    }

    /// `foliageRadius(random, trunkHeight)`.
    fn foliage_radius(&self, random: &mut dyn RandomSource, _trunk_height: i32) -> i32 {
        self.base().radius.sample(random)
    }

    /// `shouldSkipLocationSigned(random, dx, y, dz, currentRadius, doubleTrunk)`.
    fn should_skip_location_signed(&self, random: &mut dyn RandomSource, dx: i32, y: i32, dz: i32, current_radius: i32, double_trunk: bool) -> bool {
        let (min_dx, min_dz) = if double_trunk {
            (dx.abs().min((dx - 1).abs()), dz.abs().min((dz - 1).abs()))
        } else {
            (dx.abs(), dz.abs())
        };
        self.should_skip_location(random, min_dx, y, min_dz, current_radius, double_trunk)
    }

    /// `placeLeavesRow(level, foliageSetter, random, tree, origin, currentRadius, y, doubleTrunk)`.
    #[allow(clippy::too_many_arguments)]
    fn place_leaves_row(
        &self,
        level: &mut dyn WorldGenLevel,
        foliage_setter: &mut TreeSetter,
        random: &mut dyn RandomSource,
        tree: &TreeFeature,
        origin: BlockPos,
        current_radius: i32,
        y: i32,
        double_trunk: bool,
    ) {
        let offset = i32::from(double_trunk);
        for dx in -current_radius..=current_radius + offset {
            for dz in -current_radius..=current_radius + offset {
                if !self.should_skip_location_signed(random, dx, y, dz, current_radius, double_trunk) {
                    try_place_leaf(level, foliage_setter, random, tree, origin.offset(dx, y, dz));
                }
            }
        }
    }

    /// `placeLeavesRowWithHangingLeavesBelow(..)`.
    #[allow(clippy::too_many_arguments)]
    fn place_leaves_row_with_hanging_leaves_below(
        &self,
        level: &mut dyn WorldGenLevel,
        foliage_setter: &mut TreeSetter,
        random: &mut dyn RandomSource,
        tree: &TreeFeature,
        origin: BlockPos,
        current_radius: i32,
        y: i32,
        double_trunk: bool,
        hanging_leaves_chance: f32,
        hanging_leaves_extension_chance: f32,
    ) {
        self.place_leaves_row(level, foliage_setter, random, tree, origin, current_radius, y, double_trunk);
        let offset = i32::from(double_trunk);
        let log_pos = origin.below();

        for along_edge in Plane::Horizontal {
            let to_edge = along_edge.get_clockwise();
            let offset_to_edge = if to_edge.get_axis_direction() == AxisDirection::Positive { current_radius + offset } else { current_radius };
            let mut pos = origin.offset(0, y - 1, 0).relative_n(to_edge, offset_to_edge).relative_n(along_edge, -current_radius);
            let mut offset_along_edge = -current_radius;

            while offset_along_edge < current_radius + offset {
                let leaves_above = foliage_setter.is_set(&pos.relative(Direction::Up));
                if leaves_above && try_place_extension(level, foliage_setter, random, tree, hanging_leaves_chance, log_pos, pos) {
                    pos = pos.relative(Direction::Down);
                    try_place_extension(level, foliage_setter, random, tree, hanging_leaves_extension_chance, log_pos, pos);
                    pos = pos.relative(Direction::Up);
                }
                offset_along_edge += 1;
                pos = pos.relative(along_edge);
            }
        }
    }
}

fn try_place_extension(
    level: &mut dyn WorldGenLevel,
    foliage_setter: &mut TreeSetter,
    random: &mut dyn RandomSource,
    tree: &TreeFeature,
    chance: f32,
    log_pos: BlockPos,
    pos: BlockPos,
) -> bool {
    if pos.dist_manhattan(&log_pos) >= 7 {
        false
    } else if random.next_float() > chance {
        false
    } else {
        try_place_leaf(level, foliage_setter, random, tree, pos)
    }
}

/// `FoliagePlacer.tryPlaceLeaf(level, foliageSetter, random, tree, pos)`.
pub fn try_place_leaf(level: &mut dyn WorldGenLevel, foliage_setter: &mut TreeSetter, random: &mut dyn RandomSource, tree: &TreeFeature, pos: BlockPos) -> bool {
    let is_persistent = level.get_block_state(pos).try_get(props::PERSISTENT).unwrap_or(false);
    if !is_persistent && tree_feature::valid_tree_pos(level, pos) {
        let mut foliage_state = tree.foliage_provider.value().get_state(level, random, pos);
        if foliage_state.has_property(props::WATERLOGGED) {
            foliage_state = foliage_state.set(props::WATERLOGGED, fluid_is_water_source(level.get_fluid_state(pos)));
        }
        foliage_setter.set(level, pos, foliage_state);
        true
    } else {
        false
    }
}
