//! `HugeFungusFeature`.

use mc_core::direction::Direction;
use mc_core::math::mth;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::{Block, BlockState};
use mc_data::props;
use mc_game_core::world::level::LevelAccessor;
use mc_game_core::world::level::block::BlockStateExt;
use mc_game_core::world::level::levelgen::blockpredicates::{self, BlockPredicate};
use mc_game_core::world::level::levelgen::codec::{
    Fields, LoadResult, TypeEntry, parse_block_state, parse_bool,
};
use mc_game_core::world::level::levelgen::env::GenEnv;
use mc_game_core::world::level::levelgen::feature::{FEATURE_TYPES, Feature};
use mc_game_core::world::level::levelgen::registries::DecodeCtx;
use mc_game_core::world::level::levelgen::world_gen_level::WorldGenLevel;
use std::sync::Arc;

const HUGE_PROBABILITY: f32 = 0.06;

pub struct HugeFungusFeature {
    pub valid_base_state: BlockState,
    pub stem_state: BlockState,
    pub hat_state: BlockState,
    pub decor_state: BlockState,
    pub replaceable_blocks: Arc<dyn BlockPredicate>,
    pub planted: bool,
}

impl HugeFungusFeature {
    fn decode(fields: &Fields<'_>, ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn Feature>> {
        Ok(Arc::new(HugeFungusFeature {
            valid_base_state: fields.required("valid_base_block", parse_block_state)?,
            stem_state: fields.required("stem_state", parse_block_state)?,
            hat_state: fields.required("hat_state", parse_block_state)?,
            decor_state: fields.required("decor_state", parse_block_state)?,
            replaceable_blocks: fields
                .required("replaceable_blocks", |v| blockpredicates::decode(v, ctx))?,
            planted: fields.optional("planted", parse_bool)?.unwrap_or(false),
        }))
    }

    fn is_replaceable(
        &self,
        level: &dyn LevelAccessor,
        pos: BlockPos,
        check_non_replaceable_plants: bool,
    ) -> bool {
        if level.get_block_state(pos).can_be_replaced() {
            true
        } else {
            check_non_replaceable_plants && self.replaceable_blocks.test(level, pos)
        }
    }

    fn place_stem(
        &self,
        level: &mut dyn WorldGenLevel,
        random: &mut dyn RandomSource,
        surface_origin: BlockPos,
        total_height: i32,
        is_huge: bool,
    ) {
        let stem_radius = if is_huge { 1 } else { 0 };

        for dx in -stem_radius..=stem_radius {
            for dz in -stem_radius..=stem_radius {
                let corner_of_huge_stem =
                    is_huge && mth::abs_i32(dx) == stem_radius && mth::abs_i32(dz) == stem_radius;

                for dy in 0..total_height {
                    let block_pos = surface_origin.offset(dx, dy, dz);
                    if self.is_replaceable(&*level, block_pos, true) {
                        if self.planted {
                            if !level.get_block_state(block_pos.below()).is_air() {
                                level.destroy_block(block_pos, true, None, 512);
                            }
                            level.set_block_and_update(block_pos, self.stem_state);
                        } else if corner_of_huge_stem {
                            if random.next_float() < 0.1 {
                                level.set_block_and_update(block_pos, self.stem_state);
                            }
                        } else {
                            level.set_block_and_update(block_pos, self.stem_state);
                        }
                    }
                }
            }
        }
    }

    fn place_hat(
        &self,
        level: &mut dyn WorldGenLevel,
        random: &mut dyn RandomSource,
        surface_origin: BlockPos,
        total_height: i32,
        is_huge: bool,
    ) {
        let place_vines = self.hat_state.is_block(Block::NETHER_WART_BLOCK);
        let hat_height = (random.next_int_bounded(1 + total_height / 3) + 5).min(total_height);
        let hat_start_y = total_height - hat_height;

        for dy in hat_start_y..=total_height {
            let mut radius = if dy < total_height - random.next_int_bounded(3) {
                2
            } else {
                1
            };
            if hat_height > 8 && dy < hat_start_y + 4 {
                radius = 3;
            }
            if is_huge {
                radius += 1;
            }

            for dx in -radius..=radius {
                for dz in -radius..=radius {
                    let is_edge_x = dx == -radius || dx == radius;
                    let is_edge_z = dz == -radius || dz == radius;
                    let inside = !is_edge_x && !is_edge_z && dy != total_height;
                    let corner = is_edge_x && is_edge_z;
                    let is_hat_bottom = dy < hat_start_y + 3;
                    let block_pos = surface_origin.offset(dx, dy, dz);
                    if self.is_replaceable(&*level, block_pos, false) {
                        if self.planted && !level.get_block_state(block_pos.below()).is_air() {
                            level.destroy_block(block_pos, true, None, 512);
                        }

                        if is_hat_bottom {
                            if !inside {
                                self.place_hat_drop_block(level, random, block_pos, place_vines);
                            }
                        } else if inside {
                            self.place_hat_block(
                                level,
                                random,
                                block_pos,
                                0.1,
                                0.2,
                                if place_vines { 0.1 } else { 0.0 },
                            );
                        } else if corner {
                            self.place_hat_block(
                                level,
                                random,
                                block_pos,
                                0.01,
                                0.7,
                                if place_vines { 0.083 } else { 0.0 },
                            );
                        } else {
                            self.place_hat_block(
                                level,
                                random,
                                block_pos,
                                5.0E-4,
                                0.98,
                                if place_vines { 0.07 } else { 0.0 },
                            );
                        }
                    }
                }
            }
        }
    }

    fn place_hat_block(
        &self,
        level: &mut dyn WorldGenLevel,
        random: &mut dyn RandomSource,
        block_pos: BlockPos,
        decor_block_probability: f32,
        hat_block_probability: f32,
        vines_probability: f32,
    ) {
        if random.next_float() < decor_block_probability {
            level.set_block_and_update(block_pos, self.decor_state);
        } else if random.next_float() < hat_block_probability {
            level.set_block_and_update(block_pos, self.hat_state);
            if random.next_float() < vines_probability {
                try_place_weeping_vines(block_pos, level, random);
            }
        }
    }

    fn place_hat_drop_block(
        &self,
        level: &mut dyn WorldGenLevel,
        random: &mut dyn RandomSource,
        block_pos: BlockPos,
        place_vines: bool,
    ) {
        if level
            .get_block_state(block_pos.below())
            .is_block(self.hat_state.block())
        {
            level.set_block_and_update(block_pos, self.hat_state);
        // `nextFloat() < 0.15` compares against a double literal
        } else if f64::from(random.next_float()) < 0.15 {
            level.set_block_and_update(block_pos, self.hat_state);
            if place_vines && random.next_int_bounded(11) == 0 {
                try_place_weeping_vines(block_pos, level, random);
            }
        }
    }
}

fn try_place_weeping_vines(
    hat_block_pos: BlockPos,
    level: &mut dyn WorldGenLevel,
    random: &mut dyn RandomSource,
) {
    let place_pos = hat_block_pos.relative(Direction::Down);
    if level.is_empty_block(place_pos) {
        let mut goal_vine_height = mth::next_int(random, 1, 5);
        if random.next_int_bounded(7) == 0 {
            goal_vine_height *= 2;
        }
        place_weeping_vines_column(level, random, place_pos, goal_vine_height, 23, 25);
    }
}

fn place_weeping_vines_column(
    level: &mut dyn WorldGenLevel,
    random: &mut dyn RandomSource,
    origin: BlockPos,
    total_height: i32,
    min_age: i32,
    max_age: i32,
) {
    let mut place_pos = origin;
    for height in 0..=total_height {
        if level.is_empty_block(place_pos) {
            if height == total_height || !level.is_empty_block(place_pos.below()) {
                let age = mth::next_int(random, min_age, max_age);
                level.set_block_flags(
                    place_pos,
                    Block::WEEPING_VINES.default_state().set(props::AGE_25, age),
                    2,
                );
                break;
            }
            level.set_block_flags(place_pos, Block::WEEPING_VINES_PLANT.default_state(), 2);
        }
        place_pos = place_pos.relative(Direction::Down);
    }
}

#[linkme::distributed_slice(FEATURE_TYPES)]
static HUGE_FUNGUS: TypeEntry<dyn Feature> = TypeEntry {
    name: "huge_fungus",
    decode: HugeFungusFeature::decode,
};

impl Feature for HugeFungusFeature {
    fn place(
        &self,
        level: &mut dyn WorldGenLevel,
        generator: &GenEnv,
        random: &mut dyn RandomSource,
        origin: BlockPos,
    ) -> bool {
        let allowed_base_block = self.valid_base_state.block();
        let below_state = level.get_block_state(origin.below());
        if !below_state.is_block(allowed_base_block) {
            return false;
        }
        let new_origin = origin;

        let mut total_height = mth::next_int(random, 4, 13);
        if random.next_int_bounded(12) == 0 {
            total_height *= 2;
        }

        if !self.planted {
            let max_height = generator.get_gen_depth();
            if new_origin.y() + total_height + 1 >= max_height {
                return false;
            }
        }

        let is_huge = !self.planted && random.next_float() < HUGE_PROBABILITY;
        level.set_block_flags(origin, Block::AIR.default_state(), 260);
        self.place_stem(level, random, new_origin, total_height, is_huge);
        self.place_hat(level, random, new_origin, total_height, is_huge);
        true
    }
}
