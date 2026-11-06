//! `SpeleothemClusterFeature` (`speleothem_cluster`): a cluster of stalactites/stalagmites (dripstone cluster, sulfur spikes).

use super::speleothem_utils as utils;
use crate::world::level::levelgen::column::Column;
use mc_core::direction::{Direction, Plane};
use mc_core::math::mth;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::{Block, BlockState};
use mc_game_core::world::level::block::BlockStateExt;
use mc_game_core::world::level::levelgen::GenEnv;
use mc_game_core::world::level::levelgen::WorldGenLevel;
use mc_game_core::world::level::levelgen::codec::{
    BlockSet, Fields, LoadResult, TypeEntry, parse_block_state, parse_float_range, parse_int_range,
};
use mc_game_core::world::level::levelgen::feature::{FEATURE_TYPES, Feature};
use mc_game_core::world::level::levelgen::registries::DecodeCtx;
use mc_game_core::world::level::material::FluidStateExt;
use mc_game_core::world::level::{BlockGetter, LevelAccessor};
use mc_worldgen::provider::{FloatProvider, IntProvider};
use std::sync::Arc;

pub struct SpeleothemClusterFeature {
    pub base_block: BlockState,
    pub pointed_block: BlockState,
    pub replaceable_blocks: BlockSet,
    pub floor_to_ceiling_search_range: i32,
    pub height: IntProvider,
    pub radius: IntProvider,
    pub max_stalagmite_stalactite_height_diff: i32,
    pub height_deviation: i32,
    pub speleothem_block_layer_thickness: IntProvider,
    pub density: FloatProvider,
    pub wetness: FloatProvider,
    pub chance_of_speleothem_at_max_distance_from_center: f32,
    pub max_distance_from_edge_affecting_chance_of_speleothem: i32,
    pub max_distance_from_center_affecting_height_bias: i32,
}

impl SpeleothemClusterFeature {
    pub fn decode(fields: &Fields<'_>, _ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn Feature>> {
        Ok(Arc::new(SpeleothemClusterFeature {
            base_block: fields.required("base_block", parse_block_state)?,
            pointed_block: fields.required("pointed_block", parse_block_state)?,
            replaceable_blocks: fields.required("replaceable_blocks", BlockSet::from_json)?,
            floor_to_ceiling_search_range: fields
                .required("floor_to_ceiling_search_range", |v| {
                    parse_int_range(v, 1, 512)
                })?,
            height: fields.required("height", |v| IntProvider::from_json_range(v, 1, 128))?,
            radius: fields.required("radius", |v| IntProvider::from_json_range(v, 1, 128))?,
            max_stalagmite_stalactite_height_diff: fields
                .required("max_stalagmite_stalactite_height_diff", |v| {
                    parse_int_range(v, 0, 64)
                })?,
            height_deviation: fields.required("height_deviation", |v| parse_int_range(v, 1, 64))?,
            speleothem_block_layer_thickness: fields
                .required("speleothem_block_layer_thickness", |v| {
                    IntProvider::from_json_range(v, 0, 128)
                })?,
            density: fields.required("density", |v| FloatProvider::from_json_range(v, 0.0, 2.0))?,
            wetness: fields.required("wetness", |v| FloatProvider::from_json_range(v, 0.0, 2.0))?,
            chance_of_speleothem_at_max_distance_from_center: fields
                .required("chance_of_speleothem_at_max_distance_from_center", |v| {
                    parse_float_range(v, 0.0, 1.0)
                })?,
            max_distance_from_edge_affecting_chance_of_speleothem: fields.required(
                "max_distance_from_edge_affecting_chance_of_speleothem",
                |v| parse_int_range(v, 1, 64),
            )?,
            max_distance_from_center_affecting_height_bias: fields
                .required("max_distance_from_center_affecting_height_bias", |v| {
                    parse_int_range(v, 1, 64)
                })?,
        }))
    }

    #[allow(clippy::too_many_arguments)]
    fn place_column(
        &self,
        level: &mut dyn WorldGenLevel,
        random: &mut dyn RandomSource,
        pos: BlockPos,
        dx: i32,
        dz: i32,
        chance_of_water: f32,
        chance_of_stalagmite_or_stalactite: f64,
        cluster_height: i32,
        density: f32,
    ) {
        let Some(base_column) = Column::scan(
            level,
            pos,
            self.floor_to_ceiling_search_range,
            utils::is_empty_or_water,
            utils::is_neither_empty_nor_water,
        ) else {
            return;
        };
        let ceiling = base_column.get_ceiling();
        let base_floor = base_column.get_floor();
        if ceiling.is_none() && base_floor.is_none() {
            return;
        }
        let want_pool = random.next_float() < chance_of_water;
        let column;
        if want_pool
            && let Some(base_floor_y) = base_floor
            && self.can_place_pool(level, pos.at_y(base_floor_y))
        {
            column = base_column.with_floor(Some(base_floor_y - 1));
            level.set_block(pos.at_y(base_floor_y), Block::WATER.default_state(), 2, 512);
        } else {
            column = base_column;
        }

        let floor = column.get_floor();
        let want_stalactite = random.next_double() < chance_of_stalagmite_or_stalactite;
        let stalactite_height;
        if let Some(ceiling_y) = ceiling
            && want_stalactite
            && !self.is_lava(level, pos.at_y(ceiling_y))
        {
            let ceiling_thickness = self.speleothem_block_layer_thickness.sample(random);
            self.replace_blocks_with_base_blocks(
                level,
                pos.at_y(ceiling_y),
                ceiling_thickness,
                Direction::Up,
            );
            let max_height_for_this_column = match floor {
                Some(floor_y) => cluster_height.min(ceiling_y - floor_y),
                None => cluster_height,
            };
            stalactite_height =
                self.get_speleothem_height(random, dx, dz, density, max_height_for_this_column);
        } else {
            stalactite_height = 0;
        }

        let want_stalagmite = random.next_double() < chance_of_stalagmite_or_stalactite;
        let stalagmite_height;
        if let Some(floor_y) = floor
            && want_stalagmite
            && !self.is_lava(level, pos.at_y(floor_y))
        {
            let floor_thickness = self.speleothem_block_layer_thickness.sample(random);
            self.replace_blocks_with_base_blocks(
                level,
                pos.at_y(floor_y),
                floor_thickness,
                Direction::Down,
            );
            if ceiling.is_some() {
                stalagmite_height = 0.max(
                    stalactite_height
                        + mth::random_between_inclusive(
                            random,
                            -self.max_stalagmite_stalactite_height_diff,
                            self.max_stalagmite_stalactite_height_diff,
                        ),
                );
            } else {
                stalagmite_height =
                    self.get_speleothem_height(random, dx, dz, density, cluster_height);
            }
        } else {
            stalagmite_height = 0;
        }

        let (actual_stalagmite_height, actual_stalactite_height);
        if let (Some(ceiling_y), Some(floor_y)) = (ceiling, floor)
            && ceiling_y - stalactite_height <= floor_y + stalagmite_height
        {
            let lowest_stalactite_bottom = (ceiling_y - stalactite_height).max(floor_y + 1);
            let highest_stalagmite_top = (floor_y + stalagmite_height).min(ceiling_y - 1);
            let actual_stalactite_bottom = mth::random_between_inclusive(
                random,
                lowest_stalactite_bottom,
                highest_stalagmite_top + 1,
            );
            let actual_stalagmite_top = actual_stalactite_bottom - 1;
            actual_stalactite_height = ceiling_y - actual_stalactite_bottom;
            actual_stalagmite_height = actual_stalagmite_top - floor_y;
        } else {
            actual_stalactite_height = stalactite_height;
            actual_stalagmite_height = stalagmite_height;
        }

        let merge_tips = random.next_boolean()
            && actual_stalactite_height > 0
            && actual_stalagmite_height > 0
            && column
                .get_height()
                .is_some_and(|h| actual_stalactite_height + actual_stalagmite_height == h);
        if let Some(ceiling_y) = ceiling {
            utils::grow_speleothem(
                level,
                pos.at_y(ceiling_y - 1),
                Direction::Down,
                actual_stalactite_height,
                merge_tips,
                self.base_block.block(),
                self.pointed_block.block(),
                &self.replaceable_blocks,
            );
        }
        if let Some(floor_y) = floor {
            utils::grow_speleothem(
                level,
                pos.at_y(floor_y + 1),
                Direction::Up,
                actual_stalagmite_height,
                merge_tips,
                self.base_block.block(),
                self.pointed_block.block(),
                &self.replaceable_blocks,
            );
        }
    }

    fn is_lava(&self, level: &dyn BlockGetter, pos: BlockPos) -> bool {
        level.get_block_state(pos).is_block(Block::LAVA)
    }

    /// `getSpeleothemHeight(random, dx, dz, density, maxHeight)`.
    fn get_speleothem_height(
        &self,
        random: &mut dyn RandomSource,
        dx: i32,
        dz: i32,
        density: f32,
        max_height: i32,
    ) -> i32 {
        if random.next_float() > density {
            0
        } else {
            let distance_from_center = dx.abs() + dz.abs();
            let height_mean = mth::clamped_map_f64(
                distance_from_center as f64,
                0.0,
                self.max_distance_from_center_affecting_height_bias as f64,
                max_height as f64 / 2.0,
                0.0,
            ) as f32;
            FloatProvider::sample_clamped_normal(
                random,
                height_mean,
                self.height_deviation as f32,
                0.0,
                max_height as f32,
            ) as i32
        }
    }

    fn can_place_pool(&self, level: &dyn BlockGetter, pos: BlockPos) -> bool {
        let state = level.get_block_state(pos);
        if state.is_block(Block::WATER)
            || state.is_block(self.base_block.block())
            || state.is_block(self.pointed_block.block())
        {
            return false;
        }
        if level
            .get_block_state(pos.above())
            .get_fluid_state()
            .is_tag("water")
        {
            return false;
        }
        for direction in Plane::Horizontal {
            if !self.can_be_adjacent_to_water(level, pos.relative(direction)) {
                return false;
            }
        }
        self.can_be_adjacent_to_water(level, pos.below())
    }

    fn can_be_adjacent_to_water(&self, level: &dyn BlockGetter, pos: BlockPos) -> bool {
        let state = level.get_block_state(pos);
        state.is_tag("base_stone_overworld") || state.get_fluid_state().is_tag("water")
    }

    fn replace_blocks_with_base_blocks(
        &self,
        level: &mut dyn LevelAccessor,
        first_pos: BlockPos,
        max_count: i32,
        direction: Direction,
    ) {
        let mut pos = first_pos;
        for _ in 0..max_count {
            if !utils::place_base_block_if_possible(
                level,
                pos,
                self.base_block.block(),
                &self.replaceable_blocks,
            ) {
                return;
            }
            pos = pos.relative(direction);
        }
    }

    fn get_chance_of_stalagmite_or_stalactite(
        &self,
        x_radius: i32,
        z_radius: i32,
        dx: i32,
        dz: i32,
    ) -> f64 {
        let x_distance_from_edge = x_radius - dx.abs();
        let z_distance_from_edge = z_radius - dz.abs();
        let distance_from_edge = x_distance_from_edge.min(z_distance_from_edge);
        mth::clamped_map_f32(
            distance_from_edge as f32,
            0.0,
            self.max_distance_from_edge_affecting_chance_of_speleothem as f32,
            self.chance_of_speleothem_at_max_distance_from_center,
            1.0,
        ) as f64
    }
}

impl Feature for SpeleothemClusterFeature {
    fn place(
        &self,
        level: &mut dyn WorldGenLevel,
        _generator: &GenEnv,
        random: &mut dyn RandomSource,
        origin: BlockPos,
    ) -> bool {
        if !utils::is_empty_or_water_at(level, origin) {
            return false;
        }
        let height = self.height.sample(random);
        let wetness = self.wetness.sample(random);
        let density = self.density.sample(random);
        let x_radius = self.radius.sample(random);
        let z_radius = self.radius.sample(random);
        for dx in -x_radius..=x_radius {
            for dz in -z_radius..=z_radius {
                let chance =
                    self.get_chance_of_stalagmite_or_stalactite(x_radius, z_radius, dx, dz);
                let pos = origin.offset(dx, 0, dz);
                self.place_column(level, random, pos, dx, dz, wetness, chance, height, density);
            }
        }
        true
    }
}

#[linkme::distributed_slice(FEATURE_TYPES)]
static SPELEOTHEM_CLUSTER: TypeEntry<dyn Feature> = TypeEntry {
    name: "speleothem_cluster",
    decode: SpeleothemClusterFeature::decode,
};
