//! The base behaviour of `Block.playerWillDestroy` / `Block.playerDestroy` (what `BlockBehaviour::player_will_destroy` /
//! `player_destroy` default to, and what overrides call as their `super`) and the block entity snapshot
//! `ServerPlayerGameMode.destroyBlock` takes before the block is removed.

use super::ServerLevel;
use crate::world::block_entity::{BlockEntity, create_block_entity};
use crate::world::entity::{Entity, ServerPlayer};
use crate::world::item::ItemStack;
use crate::world::level::BlockGetter;
use crate::world::level::block::{BlockStateExt, behaviour};
use crate::world::level::gameevent::GameEventContext;
use mc_core::pos::BlockPos;
use mc_data::GameEvent;
use mc_data::block::BlockState;

/// `PiglinAi.angerNearbyPiglins(level, player, onlyIfTheySeeThePlayer)` for every registered body (the piglin AI
/// registers its implementation here; `mc-game-core` does not depend on it). `player` may be out of the level.
#[linkme::distributed_slice]
pub static PIGLIN_ANGER: [fn(&mut ServerLevel, &dyn Entity, bool)];

/// `PiglinAi.angerNearbyPiglins(level, player, onlyIfTheySeeThePlayer)`.
pub fn anger_nearby_piglins(
    level: &mut ServerLevel,
    player: &dyn Entity,
    only_if_they_see_the_player: bool,
) {
    for anger in PIGLIN_ANGER {
        anger(level, player, only_if_they_see_the_player);
    }
}

/// `Block.playerWillDestroy(level, pos, state, player)`: destroy particles (level event 2001 for everyone but the
/// player), `PiglinAi.angerNearbyPiglins` for blocks guarded by piglins, the `BLOCK_DESTROY` game event; returns `state`.
pub fn player_will_destroy(
    level: &mut ServerLevel,
    pos: BlockPos,
    state: BlockState,
    player: &ServerPlayer,
) -> BlockState {
    behaviour(state.block()).spawn_destroy_by_entity_particles(
        level,
        Some(player.id()),
        pos,
        state,
    );
    if state.is_tag("guarded_by_piglins") {
        anger_nearby_piglins(level, player, false);
    }
    level.game_event_at_block(
        GameEvent::BLOCK_DESTROY,
        pos,
        &GameEventContext::of_entity_ref(level, player, Some(state)),
    );
    state
}

/// `Block.playerDestroy(level, player, pos, state, blockEntity, destroyedWith)`.
pub fn player_destroy(
    level: &mut ServerLevel,
    player: &mut ServerPlayer,
    pos: BlockPos,
    state: BlockState,
    block_entity: Option<&dyn BlockEntity>,
    destroyed_with: &ItemStack,
) {
    player.award_stat_once(level, crate::stats::Stat::Mined(state.block()));
    player.cause_food_exhaustion(0.005);
    crate::world::level::storage::loot::block_drops::drop_resources_with_tool(
        level,
        state,
        pos,
        block_entity,
        Some(&*player),
        destroyed_with,
    );
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
