//! `net.minecraft.world.level.block.AbstractCandleBlock` (the shared statics and hooks of `CandleBlock` / `CandleCakeBlock`).
use super::prelude::*;
use crate::server::level::ServerLevel;
use crate::world::entity::EntityId;
use crate::world::level::explosion::Explosion;
use crate::world::level::gameevent::GameEventContext;
use mc_data::{GameEvent, SoundEvent};
use mc_protocol::packets::play::level::SoundSource;

/// `AbstractCandleBlock.isLit(state)`.
pub fn is_lit(state: BlockState) -> bool {
    state.has_property(props::LIT) && (state.is_tag("candles") || state.is_tag("candle_cakes")) && state.get(props::LIT)
}

/// `setLit(level, state, pos, lit)`.
fn set_lit(level: &mut dyn LevelAccessor, state: BlockState, pos: BlockPos, lit: bool) {
    level.set_block_flags(pos, state.set(props::LIT, lit), 11);
}

/// `AbstractCandleBlock.extinguish(player, state, level, pos)` (the smoke particles are client side).
pub fn extinguish(player: Option<EntityId>, state: BlockState, level: &mut dyn LevelAccessor, pos: BlockPos) {
    set_lit(level, state, pos, false);
    level.play_sound(None, pos, SoundEvent::BLOCK_CANDLE_EXTINGUISH, SoundSource::Blocks, 1.0, 1.0);
    level.game_event(GameEvent::BLOCK_CHANGE, mc_core::phys::Vec3::at_center_of(&pos), &GameEventContext::of_entity(player));
}

/// `AbstractCandleBlock.onProjectileHit`: `canBeLit` is the subclass' test.
pub fn on_projectile_hit(level: &mut ServerLevel, state: BlockState, hit: &mc_core::phys::BlockHitResult, projectile: &dyn crate::world::entity::Entity, can_be_lit: bool) {
    if projectile.is_on_fire() && can_be_lit {
        set_lit(level, state, hit.block_pos, true);
    }
}

/// `AbstractCandleBlock.onExplosionHit` (then `super.onExplosionHit`).
pub fn on_explosion_hit(
    state: BlockState,
    level: &mut ServerLevel,
    pos: BlockPos,
    explosion: &dyn Explosion,
    on_hit: &mut dyn FnMut(crate::world::item::ItemStack, BlockPos),
) {
    if explosion.can_trigger_blocks() && state.get(props::LIT) {
        extinguish(None, state, level, pos);
    }
    crate::world::level::explosion::default_on_explosion_hit(state, level, pos, explosion, on_hit);
}
