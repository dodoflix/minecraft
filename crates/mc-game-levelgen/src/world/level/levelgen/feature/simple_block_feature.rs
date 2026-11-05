//! `SimpleBlockFeature`.

use mc_game_core::world::level::levelgen::feature::{FEATURE_TYPES, Feature};
use mc_game_core::world::level::block::BlockStateExt;
use mc_game_core::world::level::block::impls::{double_plant_block, mossy_carpet_block};
use mc_game_core::world::level::levelgen::codec::{Fields, LoadResult, TypeEntry, parse_bool};
use mc_game_core::world::level::levelgen::env::GenEnv;
use mc_game_core::world::level::levelgen::holder::Holder;
use mc_game_core::world::level::levelgen::registries::DecodeCtx;
use mc_game_core::world::level::levelgen::stateproviders::BlockStateProvider;
use mc_game_core::world::level::levelgen::world_gen_level::WorldGenLevel;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_world::ticks::TickPriority;
use std::sync::Arc;

pub struct SimpleBlockFeature {
    pub to_place: Holder<dyn BlockStateProvider>,
    pub schedule_tick: bool,
}

impl SimpleBlockFeature {
    fn decode(fields: &Fields<'_>, ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn Feature>> {
        Ok(Arc::new(SimpleBlockFeature {
            to_place: fields.required("to_place", |v| ctx.block_state_provider(v))?,
            schedule_tick: fields.optional("schedule_tick", parse_bool)?.unwrap_or(false),
        }))
    }
}

#[linkme::distributed_slice(FEATURE_TYPES)]
static SIMPLE_BLOCK: TypeEntry<dyn Feature> = TypeEntry { name: "simple_block", decode: SimpleBlockFeature::decode };

impl Feature for SimpleBlockFeature {
    fn place(&self, level: &mut dyn WorldGenLevel, _generator: &GenEnv, random: &mut dyn RandomSource, origin: BlockPos) -> bool {
        let Some(state_to_place) = self.to_place.value().get_optional_state(&*level, random, origin) else {
            return false;
        };
        if !state_to_place.can_survive(&*level, origin) {
            return false;
        }

        let classes = state_to_place.block().data().classes;
        if classes.contains(&"DoublePlantBlock") {
            let above_state = level.get_block_state(origin.above());
            if !above_state.is_air() && (state_to_place.get_fluid_state() != above_state.get_fluid_state() || !above_state.can_be_replaced()) {
                return false;
            }
            double_plant_block::place_at(level, state_to_place, origin, 2);
        } else if classes.contains(&"MossyCarpetBlock") {
            // draws its side chances from `level.getRandom()` (the region random, not the feature random)
            mossy_carpet_block::place_at(level, origin, 2);
        } else {
            level.set_block_flags(origin, state_to_place, 2);
        }

        if self.schedule_tick {
            let block = level.get_block_state(origin).block();
            level.schedule_block_tick(origin, block, 1, TickPriority::Normal);
        }
        true
    }
}
