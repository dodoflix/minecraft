//! `net.minecraft.world.level.block.CopperBulbBlock` (also the base of `WeatheringCopperBulbBlock`, whose weathering is
//! dispatched by `weathering_copper::random_tick`).
use mc_game_core::register_block_class;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::world::level::redstone::Orientation;
use mc_game_core::world::level::signal_getter::SignalGetter;
use mc_protocol::packets::play::level::SoundSource;

pub struct CopperBulbBlock;

/// `CopperBulbBlock.checkAndFlip`.
fn check_and_flip(state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
    let signal = level.has_neighbor_signal(pos);
    if signal != state.get(props::POWERED) {
        let mut new_state = state;
        if !state.get(props::POWERED) {
            new_state = state.cycle(props::LIT);
            let sound = if new_state.get(props::LIT) {
                mc_data::SoundEvent::BLOCK_COPPER_BULB_TURN_ON
            } else {
                mc_data::SoundEvent::BLOCK_COPPER_BULB_TURN_OFF
            };
            level.play_sound(None, pos, sound, SoundSource::Blocks, 1.0, 1.0);
        }
        level.set_block_and_update(pos, new_state.set(props::POWERED, signal));
    }
}

impl BlockBehaviour for CopperBulbBlock {
    fn on_place(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        old_state: BlockState,
        _moved_by_piston: bool,
    ) {
        if old_state.block() != state.block() {
            check_and_flip(state, level, pos);
        }
    }
    fn neighbor_changed(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        _block: Block,
        _orientation: Option<Orientation>,
        _moved_by_piston: bool,
    ) {
        check_and_flip(state, level, pos);
    }
    fn get_analog_output_signal(
        &self,
        _state: BlockState,
        level: &ServerLevel,
        pos: BlockPos,
        _direction: Direction,
    ) -> i32 {
        if level.get_block_state(pos).get(props::LIT) {
            15
        } else {
            0
        }
    }
}

register_block_class!("CopperBulbBlock", |_| Box::new(CopperBulbBlock));
