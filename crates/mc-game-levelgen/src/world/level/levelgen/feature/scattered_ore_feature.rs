//! `ScatteredOreFeature`: ancient debris style ore, single blocks scattered around the origin.

use super::abstract_ore_feature::AbstractOreFeature;
use mc_core::math::java;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_game_core::world::level::levelgen::codec::TypeEntry;
use mc_game_core::world::level::levelgen::env::GenEnv;
use mc_game_core::world::level::levelgen::feature::Feature;
use mc_game_core::world::level::levelgen::world_gen_level::WorldGenLevel;
use std::sync::Arc;

const MAX_DIST_FROM_ORIGIN: i32 = 7;

pub struct ScatteredOreFeature(pub AbstractOreFeature);

impl Feature for ScatteredOreFeature {
    fn place(
        &self,
        level: &mut dyn WorldGenLevel,
        _generator: &GenEnv,
        random: &mut dyn RandomSource,
        origin: BlockPos,
    ) -> bool {
        let number_of_tries = random.next_int_bounded(self.0.size + 1);
        for i in 0..number_of_tries {
            let target_pos = offset_target_pos(random, origin, i.min(MAX_DIST_FROM_ORIGIN));
            let block_state = level.get_block_state(target_pos);
            for target_state in &self.0.target_states {
                if self.0.can_place_ore(
                    block_state,
                    &|p| level.get_block_state(p),
                    random,
                    target_state,
                    target_pos,
                ) {
                    level.set_block_flags(target_pos, target_state.state, 2);
                    break;
                }
            }
        }
        true
    }
}

fn offset_target_pos(
    random: &mut dyn RandomSource,
    origin: BlockPos,
    max_dist_from_origin_for_this_try: i32,
) -> BlockPos {
    let xd = random_placement_in_one_axis(random, max_dist_from_origin_for_this_try);
    let yd = random_placement_in_one_axis(random, max_dist_from_origin_for_this_try);
    let zd = random_placement_in_one_axis(random, max_dist_from_origin_for_this_try);
    origin.offset(xd, yd, zd)
}

fn random_placement_in_one_axis(
    random: &mut dyn RandomSource,
    max_distance_from_origin: i32,
) -> i32 {
    java::round_f32((random.next_float() - random.next_float()) * max_distance_from_origin as f32)
}

#[linkme::distributed_slice(mc_game_core::world::level::levelgen::feature::FEATURE_TYPES)]
static SCATTERED_ORE: TypeEntry<dyn Feature> = TypeEntry {
    name: "scattered_ore",
    decode: |fields, ctx| {
        Ok(Arc::new(ScatteredOreFeature(AbstractOreFeature::decode(
            fields, ctx,
        )?)))
    },
};
