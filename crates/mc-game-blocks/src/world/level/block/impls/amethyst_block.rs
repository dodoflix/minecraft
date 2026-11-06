//! `net.minecraft.world.level.block.AmethystBlock` (superclass of `BuddingAmethystBlock` and `AmethystClusterBlock`).
use mc_data::SoundEvent;
use mc_game_core::register_block_class;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::level::block::impls::prelude::*;
use mc_protocol::packets::play::level::SoundSource;

pub struct AmethystBlock;

/// `AmethystBlock.onProjectileHit`: the chime.
pub fn on_projectile_hit(level: &mut ServerLevel, hit: &mc_core::phys::BlockHitResult) {
    let pitch = 0.5f32 + level.random().next_float() * 1.2f32;
    level.play_sound(
        None,
        hit.block_pos,
        SoundEvent::BLOCK_AMETHYST_BLOCK_CHIME,
        SoundSource::Blocks,
        1.0,
        pitch,
    );
}

impl BlockBehaviour for AmethystBlock {
    fn on_projectile_hit(
        &self,
        level: &mut ServerLevel,
        _state: BlockState,
        hit: &mc_core::phys::BlockHitResult,
        _projectile: &mut dyn mc_game_core::world::entity::projectile::Projectile,
    ) {
        on_projectile_hit(level, hit);
    }
}

register_block_class!("AmethystBlock", |_| Box::new(AmethystBlock));
