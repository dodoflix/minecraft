//! `net.minecraft.world.level.block.WetSpongeBlock`.
use super::prelude::*;
use crate::server::level::ServerLevel;
use mc_data::SoundEvent;
use mc_protocol::packets::play::level::SoundSource;

pub struct WetSpongeBlock;

impl BlockBehaviour for WetSpongeBlock {
    fn on_place(&self, _state: BlockState, level: &mut ServerLevel, pos: BlockPos, _old_state: BlockState, _moved_by_piston: bool) {
        if level.environment_bool("gameplay/water_evaporates") {
            level.set_block_and_update(pos, Block::SPONGE.default_state());
            level.level_event(None, 2009, pos, 0);
            let pitch = (1.0f32 + level.random().next_float() * 0.2f32) * 0.7f32;
            level.play_sound(None, pos, SoundEvent::BLOCK_WET_SPONGE_DRIES, SoundSource::Blocks, 1.0, pitch);
        }
    }
}

register_block_class!("WetSpongeBlock", |_| Box::new(WetSpongeBlock));
