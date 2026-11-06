//! `LakeFeature` (deprecated in vanilla, still used by `lake_lava`): a blobby pool in the 16x16x8 box below the origin.

use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::{Block, BlockState};
use mc_game_core::world::level::biome_ext::should_freeze_checked;
use mc_game_core::world::level::levelgen::blockpredicates::{self, BlockPredicate};
use mc_game_core::world::level::levelgen::codec::TypeEntry;
use mc_game_core::world::level::levelgen::env::GenEnv;
use mc_game_core::world::level::levelgen::feature::{Feature, mark_above_for_post_processing};
use mc_game_core::world::level::levelgen::holder::Holder;
use mc_game_core::world::level::levelgen::stateproviders::BlockStateProvider;
use mc_game_core::world::level::levelgen::world_gen_level::WorldGenLevel;
use mc_game_core::world::level::material::FluidStateExt;
use mc_world::ticks::TickPriority;
use std::sync::Arc;

pub struct LakeFeature {
    pub fluid: Holder<dyn BlockStateProvider>,
    pub barrier: Holder<dyn BlockStateProvider>,
    pub can_place_feature: Arc<dyn BlockPredicate>,
    pub can_replace_with_air_or_fluid: Arc<dyn BlockPredicate>,
    pub can_replace_with_barrier: Arc<dyn BlockPredicate>,
}

/// `grid[(xx * 16 + zz) * 8 + yy]`.
fn idx(xx: i32, zz: i32, yy: i32) -> usize {
    ((xx * 16 + zz) * 8 + yy) as usize
}

/// The "edge of the lake" test shared by the check and barrier passes.
fn is_edge(grid: &[bool; 2048], xx: i32, zz: i32, yy: i32) -> bool {
    !grid[idx(xx, zz, yy)]
        && (xx < 15 && grid[idx(xx + 1, zz, yy)]
            || xx > 0 && grid[idx(xx - 1, zz, yy)]
            || zz < 15 && grid[idx(xx, zz + 1, yy)]
            || zz > 0 && grid[idx(xx, zz - 1, yy)]
            || yy < 7 && grid[idx(xx, zz, yy + 1)]
            || yy > 0 && grid[idx(xx, zz, yy - 1)])
}

impl Feature for LakeFeature {
    fn place(
        &self,
        level: &mut dyn WorldGenLevel,
        generator: &GenEnv,
        random: &mut dyn RandomSource,
        origin: BlockPos,
    ) -> bool {
        if origin.y() <= level.get_min_y() + 4 {
            return false;
        }
        let origin = origin.offset(-8, -4, -8);
        let air = Block::CAVE_AIR.default_state();
        let mut grid = [false; 2048];
        let spots = random.next_int_bounded(4) + 4;

        for _ in 0..spots {
            let xr = random.next_double() * 6.0 + 3.0;
            let yr = random.next_double() * 4.0 + 2.0;
            let zr = random.next_double() * 6.0 + 3.0;
            let xp = random.next_double() * (16.0 - xr - 2.0) + 1.0 + xr / 2.0;
            let yp = random.next_double() * (8.0 - yr - 4.0) + 2.0 + yr / 2.0;
            let zp = random.next_double() * (16.0 - zr - 2.0) + 1.0 + zr / 2.0;

            for xx in 1..15 {
                for zz in 1..15 {
                    for yy in 1..7 {
                        let xd = (f64::from(xx) - xp) / (xr / 2.0);
                        let yd = (f64::from(yy) - yp) / (yr / 2.0);
                        let zd = (f64::from(zz) - zp) / (zr / 2.0);
                        let d = xd * xd + yd * yd + zd * zd;
                        if d < 1.0 {
                            grid[idx(xx, zz, yy)] = true;
                        }
                    }
                }
            }
        }

        let fluid: BlockState = self.fluid.value().get_state(&*level, random, origin);

        for xx in 0..16 {
            for zz in 0..16 {
                for yy in 0..8 {
                    if is_edge(&grid, xx, zz, yy) {
                        let offset_pos = origin.offset(xx, yy, zz);
                        let block_state = level.get_block_state(offset_pos);
                        if yy >= 4 && block_state.liquid() {
                            return false;
                        }
                        if yy < 4 && !block_state.is_solid() && block_state != fluid {
                            return false;
                        }
                        if !self.can_place_feature.test(&*level, offset_pos) {
                            return false;
                        }
                    }
                }
            }
        }

        for xx in 0..16 {
            for zz in 0..16 {
                for yy in 0..8 {
                    if grid[idx(xx, zz, yy)] {
                        let place_pos = origin.offset(xx, yy, zz);
                        if self.can_replace_with_air_or_fluid.test(&*level, place_pos) {
                            let place_air = yy >= 4;
                            level.set_block_flags(
                                place_pos,
                                if place_air { air } else { fluid },
                                2,
                            );
                            if place_air {
                                level.schedule_block_tick(
                                    place_pos,
                                    air.block(),
                                    0,
                                    TickPriority::Normal,
                                );
                                mark_above_for_post_processing(level, place_pos);
                            }
                        }
                    }
                }
            }
        }

        let barrier = self.barrier.value().get_state(&*level, random, origin);
        if !barrier.is_air() {
            for xx in 0..16 {
                for zz in 0..16 {
                    for yy in 0..8 {
                        if is_edge(&grid, xx, zz, yy) && (yy < 4 || random.next_int_bounded(2) != 0)
                        {
                            let offset = origin.offset(xx, yy, zz);
                            let block_state = level.get_block_state(offset);
                            if block_state.is_solid()
                                && self.can_replace_with_barrier.test(&*level, offset)
                            {
                                level.set_block_flags(offset, barrier, 2);
                                mark_above_for_post_processing(level, offset);
                            }
                        }
                    }
                }
            }
        }

        if fluid.get_fluid_state().is_tag("water") {
            for xx in 0..16 {
                for zz in 0..16 {
                    let offset = origin.offset(xx, 4, zz);
                    let biome = generator.biomes.get(level.get_biome(offset));
                    if should_freeze_checked(biome, &*level, offset, false)
                        && self.can_replace_with_air_or_fluid.test(&*level, offset)
                    {
                        level.set_block_flags(offset, Block::ICE.default_state(), 2);
                    }
                }
            }
        }
        true
    }
}

#[linkme::distributed_slice(mc_game_core::world::level::levelgen::feature::FEATURE_TYPES)]
static LAKE: TypeEntry<dyn Feature> = TypeEntry {
    name: "lake",
    decode: |fields, ctx| {
        Ok(Arc::new(LakeFeature {
            fluid: fields.required("fluid", |v| ctx.block_state_provider(v))?,
            barrier: fields.required("barrier", |v| ctx.block_state_provider(v))?,
            can_place_feature: fields
                .required("can_place_feature", |v| blockpredicates::decode(v, ctx))?,
            can_replace_with_air_or_fluid: fields
                .required("can_replace_with_air_or_fluid", |v| {
                    blockpredicates::decode(v, ctx)
                })?,
            can_replace_with_barrier: fields.required("can_replace_with_barrier", |v| {
                blockpredicates::decode(v, ctx)
            })?,
        }))
    },
};
