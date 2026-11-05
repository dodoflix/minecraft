//! `LargeDripstoneFeature` (`large_dripstone`): a stalactite/stalagmite column pair, optionally bent by a "wind".

use super::speleothem_utils as utils;
use mc_game_core::world::level::levelgen::feature::{FEATURE_TYPES, Feature};
use mc_game_core::world::level::block::BlockStateExt;
use mc_game_core::world::level::levelgen::GenEnv;
use mc_game_core::world::level::levelgen::WorldGenLevel;
use mc_game_core::world::level::levelgen::codec::{BlockSet, Fields, LoadResult, TypeEntry, parse_float_range, parse_int_range};
use crate::world::level::levelgen::column::Column;
use mc_game_core::world::level::levelgen::registries::DecodeCtx;
use mc_game_core::world::level::BlockGetter;
use mc_core::direction::Direction;
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::Block;
use mc_world::heightmap::HeightmapType;
use mc_worldgen::provider::{FloatProvider, IntProvider};
use std::sync::Arc;

pub struct LargeDripstoneFeature {
    pub replaceable_blocks: BlockSet,
    pub floor_to_ceiling_search_range: i32,
    pub column_radius: IntProvider,
    pub height_scale: FloatProvider,
    pub max_column_radius_to_cave_height_ratio: f32,
    pub stalactite_bluntness: FloatProvider,
    pub stalagmite_bluntness: FloatProvider,
    pub wind_speed: FloatProvider,
    pub min_radius_for_wind: i32,
    pub min_bluntness_for_wind: f32,
}

impl LargeDripstoneFeature {
    pub fn decode(fields: &Fields<'_>, _ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn Feature>> {
        Ok(Arc::new(LargeDripstoneFeature {
            replaceable_blocks: fields.required("replaceable_blocks", BlockSet::from_json)?,
            floor_to_ceiling_search_range: fields.optional("floor_to_ceiling_search_range", |v| parse_int_range(v, 1, 512))?.unwrap_or(30),
            column_radius: fields.required("column_radius", |v| IntProvider::from_json_range(v, 1, 16))?,
            height_scale: fields.required("height_scale", |v| FloatProvider::from_json_range(v, 0.0, 20.0))?,
            max_column_radius_to_cave_height_ratio: fields.required("max_column_radius_to_cave_height_ratio", |v| parse_float_range(v, 0.1, 1.0))?,
            stalactite_bluntness: fields.required("stalactite_bluntness", |v| FloatProvider::from_json_range(v, 0.1, 10.0))?,
            stalagmite_bluntness: fields.required("stalagmite_bluntness", |v| FloatProvider::from_json_range(v, 0.1, 10.0))?,
            wind_speed: fields.required("wind_speed", |v| FloatProvider::from_json_range(v, 0.0, 2.0))?,
            min_radius_for_wind: fields.required("min_radius_for_wind", |v| parse_int_range(v, 0, 100))?,
            min_bluntness_for_wind: fields.required("min_bluntness_for_wind", |v| parse_float_range(v, 0.0, 5.0))?,
        }))
    }
}

fn make_dripstone(root: BlockPos, pointing_up: bool, random: &mut dyn RandomSource, radius: i32, bluntness: &FloatProvider, height_scale: &FloatProvider) -> LargeDripstone {
    let bluntness = bluntness.sample(random) as f64;
    let scale = height_scale.sample(random) as f64;
    LargeDripstone { root, pointing_up, radius, bluntness, scale }
}

impl Feature for LargeDripstoneFeature {
    fn place(&self, level: &mut dyn WorldGenLevel, _generator: &GenEnv, random: &mut dyn RandomSource, origin: BlockPos) -> bool {
        if !utils::is_empty_or_water_at(level, origin) {
            return false;
        }
        let column = Column::scan(level, origin, self.floor_to_ceiling_search_range, utils::is_empty_or_water, |state| {
            utils::is_base_or_lava(state, Block::DRIPSTONE_BLOCK, &self.replaceable_blocks)
        });
        let Some(Column::Range(column_range)) = column else {
            return false;
        };
        if column_range.height() < 4 {
            return false;
        }
        let max_column_radius_based_on_column_height = (column_range.height() as f32 * self.max_column_radius_to_cave_height_ratio) as i32;
        let max_column_radius = mth::clamp_i32(max_column_radius_based_on_column_height, self.column_radius.min_inclusive(), self.column_radius.max_inclusive());
        let radius = mth::random_between_inclusive(random, self.column_radius.min_inclusive(), max_column_radius);
        let mut stalactite = make_dripstone(origin.at_y(column_range.ceiling() - 1), false, random, radius, &self.stalactite_bluntness, &self.height_scale);
        let mut stalagmite = make_dripstone(origin.at_y(column_range.floor() + 1), true, random, radius, &self.stalagmite_bluntness, &self.height_scale);
        let wind = if stalactite.is_suitable_for_wind(self.min_radius_for_wind, self.min_bluntness_for_wind)
            && stalagmite.is_suitable_for_wind(self.min_radius_for_wind, self.min_bluntness_for_wind)
        {
            WindOffsetter::new(origin.y(), random, &self.wind_speed, 16 - radius)
        } else {
            WindOffsetter::no_wind()
        };
        let stalactite_base_embedded_in_stone = stalactite.move_back_until_base_is_inside_stone_and_shrink_radius_if_necessary(level, &wind);
        let stalagmite_base_embedded_in_stone = stalagmite.move_back_until_base_is_inside_stone_and_shrink_radius_if_necessary(level, &wind);
        if stalactite_base_embedded_in_stone {
            stalactite.place_blocks(level, random, &wind);
        }
        if stalagmite_base_embedded_in_stone {
            stalagmite.place_blocks(level, random, &wind);
        }
        true
    }
}

struct LargeDripstone {
    root: BlockPos,
    pointing_up: bool,
    radius: i32,
    bluntness: f64,
    scale: f64,
}

impl LargeDripstone {
    fn get_height(&self) -> i32 {
        self.get_height_at_radius(0.0)
    }

    fn move_back_until_base_is_inside_stone_and_shrink_radius_if_necessary(&mut self, level: &dyn BlockGetter, wind: &WindOffsetter) -> bool {
        while self.radius > 1 {
            let mut new_root = self.root;
            let max_tries = 10.min(self.get_height());
            for _ in 0..max_tries {
                if level.get_block_state(new_root).is_block(Block::LAVA) {
                    return false;
                }
                if utils::is_circle_mostly_embedded_in_stone(level, wind.offset(new_root), self.radius) {
                    self.root = new_root;
                    return true;
                }
                new_root = new_root.relative(if self.pointing_up { Direction::Down } else { Direction::Up });
            }
            self.radius /= 2;
        }
        false
    }

    fn get_height_at_radius(&self, check_radius: f32) -> i32 {
        utils::get_speleothem_height(check_radius as f64, self.radius as f64, self.scale, self.bluntness) as i32
    }

    fn place_blocks(&self, level: &mut dyn WorldGenLevel, random: &mut dyn RandomSource, wind: &WindOffsetter) {
        for dx in -self.radius..=self.radius {
            for dz in -self.radius..=self.radius {
                let current_radius = mth::sqrt((dx * dx + dz * dz) as f32);
                if current_radius > self.radius as f32 {
                    continue;
                }
                let mut height = self.get_height_at_radius(current_radius);
                if height > 0 {
                    if (random.next_float() as f64) < 0.2 {
                        height = (height as f32 * mth::random_between(random, 0.8, 1.0)) as i32;
                    }
                    let mut pos = self.root.offset(dx, 0, dz);
                    let mut has_been_out_of_stone = false;
                    let max_y = if self.pointing_up { level.get_height_at(HeightmapType::WorldSurfaceWg, pos.x(), pos.z()) } else { i32::MAX };
                    let mut i = 0;
                    while i < height && pos.y() < max_y {
                        let wind_adjusted_pos = wind.offset(pos);
                        if utils::is_empty_or_water_or_lava_at(level, wind_adjusted_pos) {
                            has_been_out_of_stone = true;
                            level.set_block(wind_adjusted_pos, Block::DRIPSTONE_BLOCK.default_state(), 2, 512);
                        } else if has_been_out_of_stone && level.get_block_state(wind_adjusted_pos).is_tag("base_stone_overworld") {
                            break;
                        }
                        pos = pos.relative(if self.pointing_up { Direction::Up } else { Direction::Down });
                        i += 1;
                    }
                }
            }
        }
    }

    fn is_suitable_for_wind(&self, min_radius_for_wind: i32, min_bluntness_for_wind: f32) -> bool {
        self.radius >= min_radius_for_wind && self.bluntness >= min_bluntness_for_wind as f64
    }
}

struct WindOffsetter {
    origin_y: i32,
    wind_speed: Option<Vec3>,
    max_offset: i32,
}

impl WindOffsetter {
    fn new(origin_y: i32, random: &mut dyn RandomSource, wind_speed_range: &FloatProvider, max_offset: i32) -> WindOffsetter {
        let speed = wind_speed_range.sample(random);
        let direction = mth::random_between(random, 0.0, std::f32::consts::PI);
        WindOffsetter {
            origin_y,
            max_offset,
            wind_speed: Some(Vec3::new((mth::cos(direction as f64) * speed) as f64, 0.0, (mth::sin(direction as f64) * speed) as f64)),
        }
    }

    fn no_wind() -> WindOffsetter {
        WindOffsetter { origin_y: 0, wind_speed: None, max_offset: 0 }
    }

    fn offset(&self, pos: BlockPos) -> BlockPos {
        match &self.wind_speed {
            None => pos,
            Some(wind_speed) => {
                let dy = self.origin_y.wrapping_sub(pos.y()) as f64;
                let dx = mth::clamp_i32(mth::floor_f64(wind_speed.x * dy), -self.max_offset, self.max_offset);
                let dz = mth::clamp_i32(mth::floor_f64(wind_speed.z * dy), -self.max_offset, self.max_offset);
                pos.offset(dx, 0, dz)
            }
        }
    }
}

#[linkme::distributed_slice(FEATURE_TYPES)]
static LARGE_DRIPSTONE: TypeEntry<dyn Feature> = TypeEntry { name: "large_dripstone", decode: LargeDripstoneFeature::decode };
