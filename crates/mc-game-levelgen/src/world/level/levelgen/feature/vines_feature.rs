//! `VinesFeature`.

use mc_core::direction::Direction;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::Block;
use mc_game_core::world::level::block::impls::vine_block;
use mc_game_core::world::level::levelgen::codec::TypeEntry;
use mc_game_core::world::level::levelgen::env::GenEnv;
use mc_game_core::world::level::levelgen::feature::{FEATURE_TYPES, Feature};
use mc_game_core::world::level::levelgen::world_gen_level::WorldGenLevel;
use std::sync::Arc;

pub struct VinesFeature;

#[linkme::distributed_slice(FEATURE_TYPES)]
static VINES: TypeEntry<dyn Feature> = TypeEntry {
    name: "vines",
    decode: |_, _| Ok(Arc::new(VinesFeature)),
};

impl Feature for VinesFeature {
    fn place(
        &self,
        level: &mut dyn WorldGenLevel,
        _generator: &GenEnv,
        _random: &mut dyn RandomSource,
        origin: BlockPos,
    ) -> bool {
        if !level.is_empty_block(origin) {
            return false;
        }
        for direction in Direction::ALL {
            if direction != Direction::Down
                && vine_block::is_acceptable_neighbour(
                    &*level,
                    origin.relative(direction),
                    direction,
                )
            {
                let state = Block::VINE
                    .default_state()
                    .set(vine_block::get_property_for_face(direction), true);
                level.set_block_flags(origin, state, 2);
                return true;
            }
        }
        false
    }
}
