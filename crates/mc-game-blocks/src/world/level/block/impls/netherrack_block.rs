//! `net.minecraft.world.level.block.NetherrackBlock`: bone meal turns it into the nylium found next to it.
use mc_game_core::register_block_class;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::level::block::bonemeal::{BonemealSource, BonemealType};
use mc_game_core::world::level::block::impls::prelude::*;

pub struct NetherrackBlock;

/// `level.findBlocksIn(pos.offset(-1,-1,-1), pos.offset(1,1,1)).filterState(is NYLIUM)`: the nylium states of the 3x3x3 box.
fn nylium_around(level: &dyn LevelReader, pos: BlockPos) -> impl Iterator<Item = BlockState> + '_ {
    (-1..=1)
        .flat_map(|dx| (-1..=1).flat_map(move |dy| (-1..=1).map(move |dz| (dx, dy, dz))))
        .map(move |(dx, dy, dz)| level.get_block_state(pos.offset(dx, dy, dz)))
        .filter(|state| state.is_tag("nylium"))
}

impl BlockBehaviour for NetherrackBlock {
    fn is_bonemealable(&self) -> bool {
        true
    }
    fn is_valid_bonemeal_target(
        &self,
        level: &dyn LevelReader,
        pos: BlockPos,
        _state: BlockState,
        _source: BonemealSource,
    ) -> bool {
        level
            .get_block_state(pos.above())
            .propagates_skylight_down()
            && nylium_around(level, pos).next().is_some()
    }
    fn perform_bonemeal(
        &self,
        level: &mut ServerLevel,
        pos: BlockPos,
        _state: BlockState,
        _source: BonemealSource,
    ) {
        let (mut found_red, mut found_blue) = (false, false);
        for state in nylium_around(&*level, pos) {
            if state.is_block(Block::WARPED_NYLIUM) {
                found_blue = true;
            } else if state.is_block(Block::CRIMSON_NYLIUM) {
                found_red = true;
            }
            if found_blue && found_red {
                break;
            }
        }
        if found_blue && found_red {
            let nylium = if level.random().next_boolean() {
                Block::WARPED_NYLIUM
            } else {
                Block::CRIMSON_NYLIUM
            };
            level.set_block_and_update(pos, nylium.default_state());
        } else if found_blue {
            level.set_block_and_update(pos, Block::WARPED_NYLIUM.default_state());
        } else if found_red {
            level.set_block_and_update(pos, Block::CRIMSON_NYLIUM.default_state());
        }
    }
    fn get_bonemeal_type(&self) -> BonemealType {
        BonemealType::NeighborSpreader
    }
}

register_block_class!("NetherrackBlock", |_| Box::new(NetherrackBlock));
