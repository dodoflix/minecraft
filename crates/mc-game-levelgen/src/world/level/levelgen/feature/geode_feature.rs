//! `GeodeFeature` (`geode`): amethyst geodes.

use crate::world::level::levelgen::geode_block_settings::GeodeBlockSettings;
use crate::world::level::levelgen::geode_crack_settings::GeodeCrackSettings;
use crate::world::level::levelgen::geode_layer_settings::GeodeLayerSettings;
use mc_core::direction::Direction;
use mc_core::math::mth;
use mc_core::pos::BlockPos;
use mc_core::random::{LegacyRandomSource, RandomSource, WorldgenRandom};
use mc_data::block::{Block, BlockState};
use mc_data::props;
use mc_game_core::world::level::block::BlockStateExt;
use mc_game_core::world::level::levelgen::GenEnv;
use mc_game_core::world::level::levelgen::WorldGenLevel;
use mc_game_core::world::level::levelgen::codec::{
    Fields, LoadResult, TypeEntry, parse_bool, parse_double_range, parse_int,
};
use mc_game_core::world::level::levelgen::feature::{FEATURE_TYPES, Feature, safe_set_block};
use mc_game_core::world::level::levelgen::registries::DecodeCtx;
use mc_game_core::world::level::material::FluidStateExt;
use mc_world::ticks::TickPriority;
use mc_worldgen::provider::IntProvider;
use mc_worldgen::synth::NormalNoise;
use std::sync::{Arc, OnceLock};

pub struct GeodeFeature {
    pub block_settings: GeodeBlockSettings,
    pub layer_settings: GeodeLayerSettings,
    pub crack_settings: GeodeCrackSettings,
    pub use_potential_placements_chance: f64,
    pub use_alternate_layer0_chance: f64,
    pub placements_require_layer0_alternate: bool,
    pub outer_wall_distance: IntProvider,
    pub distribution_points: IntProvider,
    pub point_offset: IntProvider,
    pub min_gen_offset: i32,
    pub max_gen_offset: i32,
    pub noise_multiplier: f64,
    pub invalid_blocks_threshold: i32,
}

fn chance_range(value: &serde_json::Value) -> LoadResult<f64> {
    parse_double_range(value, 0.0, 1.0)
}

impl GeodeFeature {
    pub fn decode(fields: &Fields<'_>, ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn Feature>> {
        let uniform = |min_inclusive, max_inclusive| IntProvider::Uniform {
            min_inclusive,
            max_inclusive,
        };
        Ok(Arc::new(GeodeFeature {
            block_settings: fields.required("blocks", |v| GeodeBlockSettings::from_json(v, ctx))?,
            layer_settings: fields.required("layers", GeodeLayerSettings::from_json)?,
            crack_settings: fields.required("crack", GeodeCrackSettings::from_json)?,
            use_potential_placements_chance: fields
                .optional("use_potential_placements_chance", chance_range)?
                .unwrap_or(0.35),
            use_alternate_layer0_chance: fields
                .optional("use_alternate_layer0_chance", chance_range)?
                .unwrap_or(0.0),
            placements_require_layer0_alternate: fields
                .optional("placements_require_layer0_alternate", parse_bool)?
                .unwrap_or(true),
            outer_wall_distance: fields
                .optional("outer_wall_distance", |v| {
                    IntProvider::from_json_range(v, 1, 20)
                })?
                .unwrap_or(uniform(4, 5)),
            distribution_points: fields
                .optional("distribution_points", |v| {
                    IntProvider::from_json_range(v, 1, 20)
                })?
                .unwrap_or(uniform(3, 4)),
            point_offset: fields
                .optional("point_offset", |v| IntProvider::from_json_range(v, 0, 10))?
                .unwrap_or(uniform(1, 2)),
            min_gen_offset: fields.optional("min_gen_offset", parse_int)?.unwrap_or(-16),
            max_gen_offset: fields.optional("max_gen_offset", parse_int)?.unwrap_or(16),
            noise_multiplier: fields
                .optional("noise_multiplier", chance_range)?
                .unwrap_or(0.05),
            invalid_blocks_threshold: fields.required("invalid_blocks_threshold", parse_int)?,
        }))
    }
}

/// `NOISE_PARAMETERS`.
fn noise_parameters() -> &'static NormalNoise {
    static NOISE: OnceLock<NormalNoise> = OnceLock::new();
    NOISE.get_or_init(|| NormalNoise::create_parity(-4, &[1.0]))
}

/// `BuddingAmethystBlock.canClusterGrowAtState(state)`.
pub fn can_cluster_grow_at_state(state: BlockState) -> bool {
    state.is_air() || (state.is_block(Block::WATER) && state.get_fluid_state().is_full())
}

impl Feature for GeodeFeature {
    fn place(
        &self,
        level: &mut dyn WorldGenLevel,
        _generator: &GenEnv,
        random: &mut dyn RandomSource,
        origin: BlockPos,
    ) -> bool {
        let mut points: Vec<(BlockPos, i32)> = Vec::new();
        let num_points = self.distribution_points.sample(random);
        let mut noise_random =
            WorldgenRandom::new(Box::new(LegacyRandomSource::new(level.get_seed())));
        let noise = noise_parameters().create(&mut noise_random);
        let mut crack_points: Vec<BlockPos> = Vec::new();
        let crack_size_adjustment =
            num_points as f64 / self.outer_wall_distance.max_inclusive() as f64;
        let layers = &self.layer_settings;
        let inner_air = 1.0 / layers.filling.sqrt();
        let innermost_block_layer = 1.0 / (layers.inner_layer + crack_size_adjustment).sqrt();
        let inner_crust = 1.0 / (layers.middle_layer + crack_size_adjustment).sqrt();
        let outer_crust = 1.0 / (layers.outer_layer + crack_size_adjustment).sqrt();
        let crack_size = 1.0
            / (self.crack_settings.base_crack_size
                + random.next_double() / 2.0
                + if num_points > 3 {
                    crack_size_adjustment
                } else {
                    0.0
                })
            .sqrt();
        let should_generate_crack =
            (random.next_float() as f64) < self.crack_settings.generate_crack_chance;
        let mut num_invalid_points = 0;

        for _ in 0..num_points {
            let x = self.outer_wall_distance.sample(random);
            let y = self.outer_wall_distance.sample(random);
            let z = self.outer_wall_distance.sample(random);
            let pos = origin.offset(x, y, z);
            let state = level.get_block_state(pos);
            if state.is_air() || self.block_settings.invalid_blocks.contains(state.block()) {
                num_invalid_points += 1;
                if num_invalid_points > self.invalid_blocks_threshold {
                    return false;
                }
            }
            points.push((pos, self.point_offset.sample(random)));
        }

        if should_generate_crack {
            let offset_index = random.next_int_bounded(4);
            let crack_offset = num_points * 2 + 1;
            let (dx, dz) = match offset_index {
                0 => (crack_offset, 0),
                1 => (0, crack_offset),
                2 => (crack_offset, crack_offset),
                _ => (0, 0),
            };
            crack_points.push(origin.offset(dx, 7, dz));
            crack_points.push(origin.offset(dx, 5, dz));
            crack_points.push(origin.offset(dx, 1, dz));
        }

        let mut potential_crystal_placements: Vec<BlockPos> = Vec::new();
        let cannot_replace = &self.block_settings.cannot_replace;
        let can_replace = |s: BlockState| !cannot_replace.contains(s.block());

        for point_inside in BlockPos::between_closed(
            &origin.offset(
                self.min_gen_offset,
                self.min_gen_offset,
                self.min_gen_offset,
            ),
            &origin.offset(
                self.max_gen_offset,
                self.max_gen_offset,
                self.max_gen_offset,
            ),
        ) {
            let noise_offset = noise.get3(
                point_inside.x() as f64,
                point_inside.y() as f64,
                point_inside.z() as f64,
            ) as f64
                * self.noise_multiplier;
            let mut dist_sum_shell = 0.0;
            for (point, offset) in &points {
                dist_sum_shell +=
                    mth::inv_sqrt_f64(point_inside.dist_sqr(point) + *offset as f64) + noise_offset;
            }
            if dist_sum_shell < outer_crust {
                continue;
            }
            if dist_sum_shell >= inner_air {
                let state = self.block_settings.filling_provider.value().get_state(
                    level,
                    random,
                    point_inside,
                );
                safe_set_block(level, point_inside, state, can_replace);
            } else {
                let mut dist_sum_crack = 0.0;
                for point in &crack_points {
                    dist_sum_crack += mth::inv_sqrt_f64(
                        point_inside.dist_sqr(point)
                            + self.crack_settings.crack_point_offset as f64,
                    ) + noise_offset;
                }
                if should_generate_crack && dist_sum_crack >= crack_size {
                    safe_set_block(level, point_inside, Block::AIR.default_state(), can_replace);
                    for direction in Direction::ALL {
                        let adjacent_pos = point_inside.relative(direction);
                        let adjacent_fluid_state = level.get_fluid_state(adjacent_pos);
                        if !adjacent_fluid_state.is_empty() {
                            level.schedule_fluid_tick(
                                adjacent_pos,
                                adjacent_fluid_state.get_type(),
                                0,
                                TickPriority::Normal,
                            );
                        }
                    }
                } else if dist_sum_shell >= innermost_block_layer {
                    let use_alternate_layer =
                        (random.next_float() as f64) < self.use_alternate_layer0_chance;
                    let provider = if use_alternate_layer {
                        &self.block_settings.alternate_inner_layer_provider
                    } else {
                        &self.block_settings.inner_layer_provider
                    };
                    let state = provider.value().get_state(level, random, point_inside);
                    safe_set_block(level, point_inside, state, can_replace);
                    if (!self.placements_require_layer0_alternate || use_alternate_layer)
                        && (random.next_float() as f64) < self.use_potential_placements_chance
                    {
                        potential_crystal_placements.push(point_inside);
                    }
                } else if dist_sum_shell >= inner_crust {
                    let state = self.block_settings.middle_layer_provider.value().get_state(
                        level,
                        random,
                        point_inside,
                    );
                    safe_set_block(level, point_inside, state, can_replace);
                } else if dist_sum_shell >= outer_crust {
                    let state = self.block_settings.outer_layer_provider.value().get_state(
                        level,
                        random,
                        point_inside,
                    );
                    safe_set_block(level, point_inside, state, can_replace);
                }
            }
        }

        let inner_placements = &self.block_settings.inner_placements;
        for crystal_pos in potential_crystal_placements {
            let mut block_state =
                inner_placements[random.next_int_bounded(inner_placements.len() as i32) as usize];
            for direction in Direction::ALL {
                if block_state.has_property(props::FACING) {
                    block_state = block_state.set(props::FACING, direction);
                }
                let place_pos = crystal_pos.relative(direction);
                let place_state = level.get_block_state(place_pos);
                if block_state.has_property(props::WATERLOGGED) {
                    block_state = block_state.set(
                        props::WATERLOGGED,
                        place_state.get_fluid_state().is_source(),
                    );
                }
                if can_cluster_grow_at_state(place_state) {
                    safe_set_block(level, place_pos, block_state, can_replace);
                    break;
                }
            }
        }
        true
    }
}

#[linkme::distributed_slice(FEATURE_TYPES)]
static GEODE: TypeEntry<dyn Feature> = TypeEntry {
    name: "geode",
    decode: GeodeFeature::decode,
};
