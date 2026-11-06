//! `BlueIceFeature`.

use mc_core::direction::Direction;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::Block;
use mc_game_core::world::level::block::BlockStateExt;
use mc_game_core::world::level::levelgen::codec::TypeEntry;
use mc_game_core::world::level::levelgen::env::GenEnv;
use mc_game_core::world::level::levelgen::feature::{FEATURE_TYPES, Feature};
use mc_game_core::world::level::levelgen::world_gen_level::WorldGenLevel;
use std::sync::Arc;

pub struct BlueIceFeature;

#[linkme::distributed_slice(FEATURE_TYPES)]
static BLUE_ICE: TypeEntry<dyn Feature> = TypeEntry {
    name: "blue_ice",
    decode: |_, _| Ok(Arc::new(BlueIceFeature)),
};

impl Feature for BlueIceFeature {
    fn place(
        &self,
        level: &mut dyn WorldGenLevel,
        _generator: &GenEnv,
        random: &mut dyn RandomSource,
        origin: BlockPos,
    ) -> bool {
        if origin.y() > level.get_sea_level() - 1 {
            return false;
        }
        if !level.get_block_state(origin).is_block(Block::WATER)
            && !level.get_block_state(origin.below()).is_block(Block::WATER)
        {
            return false;
        }
        let mut found_packed_ice = false;
        for direction in Direction::ALL {
            if direction != Direction::Down
                && level
                    .get_block_state(origin.relative(direction))
                    .is_block(Block::PACKED_ICE)
            {
                found_packed_ice = true;
                break;
            }
        }
        if !found_packed_ice {
            return false;
        }

        level.set_block_flags(origin, Block::BLUE_ICE.default_state(), 2);

        for _ in 0..200 {
            let y_off = random.next_int_bounded(5) - random.next_int_bounded(6);
            let mut xz_diff = 3;
            if y_off < 2 {
                xz_diff += y_off / 2;
            }

            if xz_diff >= 1 {
                let dx = random.next_int_bounded(xz_diff) - random.next_int_bounded(xz_diff);
                let dz = random.next_int_bounded(xz_diff) - random.next_int_bounded(xz_diff);
                let place_pos = origin.offset(dx, y_off, dz);
                let place_state = level.get_block_state(place_pos);
                if place_state.is_air()
                    || place_state.is_block(Block::WATER)
                    || place_state.is_block(Block::PACKED_ICE)
                    || place_state.is_block(Block::ICE)
                {
                    for direction in Direction::ALL {
                        let relative_block_state =
                            level.get_block_state(place_pos.relative(direction));
                        if relative_block_state.is_block(Block::BLUE_ICE) {
                            level.set_block_flags(place_pos, Block::BLUE_ICE.default_state(), 2);
                            break;
                        }
                    }
                }
            }
        }
        true
    }
}
