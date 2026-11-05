//! `SculkPatchFeature` (`sculk_patch`).

use mc_game_core::world::level::levelgen::feature::{FEATURE_TYPES, Feature};
use mc_game_core::world::level::LevelAccessor;
use mc_game_core::world::level::block::BlockStateExt;
use mc_game_core::world::level::block::sculk_behaviour::sculk_behaviour_of;
use mc_game_core::world::level::block::sculk_spreader::SculkSpreader;
use mc_game_core::world::level::levelgen::codec::{Fields, LoadResult, TypeEntry, parse_int_range};
use mc_game_core::world::level::levelgen::env::GenEnv;
use mc_game_core::world::level::levelgen::registries::DecodeCtx;
use mc_game_core::world::level::levelgen::world_gen_level::WorldGenLevel;
use mc_game_core::world::level::material::FluidStateExt;
use mc_core::direction::Direction;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::Block;
use std::sync::Arc;

pub struct SculkPatchFeature {
    pub charge_count: i32,
    pub amount_per_charge: i32,
    pub spread_attempts: i32,
    pub growth_rounds: i32,
    pub spread_rounds: i32,
}

impl SculkPatchFeature {
    fn decode(fields: &Fields<'_>, _ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn Feature>> {
        Ok(Arc::new(SculkPatchFeature {
            charge_count: fields.required("charge_count", |v| parse_int_range(v, 1, 32))?,
            amount_per_charge: fields.required("amount_per_charge", |v| parse_int_range(v, 1, 500))?,
            spread_attempts: fields.required("spread_attempts", |v| parse_int_range(v, 1, 64))?,
            growth_rounds: fields.required("growth_rounds", |v| parse_int_range(v, 0, 8))?,
            spread_rounds: fields.required("spread_rounds", |v| parse_int_range(v, 0, 8))?,
        }))
    }

    /// `canSpreadFrom(level, origin)`.
    fn can_spread_from(level: &dyn LevelAccessor, origin: BlockPos) -> bool {
        let start = level.get_block_state(origin);
        if sculk_behaviour_of(start.block()).is_some() {
            return true;
        }
        if !start.is_air() && (!start.is_block(Block::WATER) || !start.get_fluid_state().is_source()) {
            return false;
        }
        Direction::ALL.into_iter().any(|d| {
            let pos = origin.relative(d);
            level.get_block_state(pos).is_collision_shape_full_block_at(level, pos)
        })
    }
}

#[linkme::distributed_slice(FEATURE_TYPES)]
static SCULK_PATCH: TypeEntry<dyn Feature> = TypeEntry { name: "sculk_patch", decode: SculkPatchFeature::decode };

impl Feature for SculkPatchFeature {
    fn place(&self, level: &mut dyn WorldGenLevel, _generator: &GenEnv, random: &mut dyn RandomSource, origin: BlockPos) -> bool {
        if !Self::can_spread_from(&*level, origin) {
            return false;
        }
        let mut spreader = SculkSpreader::create_world_gen_spreader();
        let total_rounds = self.spread_rounds + self.growth_rounds;
        for round in 0..total_rounds {
            for _ in 0..self.charge_count {
                spreader.add_cursors(origin, self.amount_per_charge);
            }
            let spread_veins = round < self.spread_rounds;
            for _ in 0..self.spread_attempts {
                spreader.update_cursors(level, origin, random, spread_veins);
            }
            spreader.clear();
        }
        true
    }
}
