//! The base behaviour of `Block.playerWillDestroy` / `Block.playerDestroy` (what `BlockBehaviour::player_will_destroy` /
//! `player_destroy` default to, and what overrides call as their `super`) and the block entity snapshot
//! `ServerPlayerGameMode.destroyBlock` takes before the block is removed.

use super::ServerLevel;
use crate::world::block_entity::{BlockEntity, create_block_entity};
use crate::world::entity::{Entity, ServerPlayer};
use crate::world::item::ItemStack;
use crate::world::level::gameevent::GameEventContext;
use crate::world::level::{BlockGetter, LevelAccessor};
use mc_core::pos::BlockPos;
use mc_data::GameEvent;
use mc_data::block::BlockState;

/// `Block.playerWillDestroy(level, pos, state, player)`: destroy particles (level event 2001 for everyone but the
/// player), the `BLOCK_DESTROY` game event; returns `state`. (`PiglinAi.angerNearbyPiglins` for blocks guarded by
/// piglins belongs to the piglin AI, which is not ported.)
pub fn player_will_destroy(level: &mut ServerLevel, pos: BlockPos, state: BlockState, player: &ServerPlayer) -> BlockState {
    level.level_event(Some(player.id()), 2001, pos, state.id() as i32);
    level.game_event_at_block(GameEvent::BLOCK_DESTROY, pos, &GameEventContext::of(Some(player.id()), Some(state)));
    state
}

/// `Block.playerDestroy(level, player, pos, state, blockEntity, destroyedWith)`.
pub fn player_destroy(level: &mut ServerLevel, player: &mut ServerPlayer, pos: BlockPos, state: BlockState, block_entity: Option<&dyn BlockEntity>, destroyed_with: &ItemStack) {
    // SEAM(EcPlayer): `player.awardStat(Stats.BLOCK_MINED.get(this))` and `causeFoodExhaustion(0.005F)` once `Stats` and
    // `ServerPlayer::cause_food_exhaustion` land; the exhaustion part is `Player.causeFoodExhaustion`.
    if !player.player.abilities.invulnerable {
        player.player.food_data.add_exhaustion(0.005);
    }
    drop_resources_with_tool(level, state, pos, block_entity, Some(player.id()), destroyed_with);
}

/// `Block.dropResources(state, level, pos, blockEntity, breaker, tool)`.
pub fn drop_resources_with_tool(level: &mut ServerLevel, state: BlockState, pos: BlockPos, _block_entity: Option<&dyn BlockEntity>, breaker: Option<crate::world::entity::EntityId>, _tool: &ItemStack) {
    // SEAM(EcLoot): `getDrops(state, level, pos, blockEntity, breaker, tool)` (loot table with the tool/enchantments) +
    // `popResource` + `state.spawnAfterBreak(level, pos, tool, true)`; until `block_drops` lands this goes through the
    // `set_drop_resources` hook, which only knows state/pos/breaker.
    crate::world::level::block::helpers::drop_resources(level, state, pos, breaker);
}

/// A detached copy of the block entity at `pos` (`ServerPlayerGameMode.destroyBlock` keeps the object alive across
/// `removeBlock`, here the saved data is loaded into a fresh instance).
pub fn snapshot_block_entity(level: &ServerLevel, pos: BlockPos) -> Option<Box<dyn BlockEntity>> {
    let original = level.get_block_entity(pos)?;
    let tag = original.save_with_full_metadata();
    let mut copy = create_block_entity(original.get_type(), pos, original.get_block_state())?;
    copy.load_with_components(&tag);
    Some(copy)
}
