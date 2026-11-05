//! `net.minecraft.world.level.gameevent.vibrations`: the vibration system (data, selector, info, listener, ticker).

mod vibration_info;
mod vibration_selector;
mod vibration_system;

pub use vibration_info::VibrationInfo;
pub use vibration_selector::VibrationSelector;
pub use vibration_system::*;

use crate::server::level::ServerLevel;
use crate::world::entity::item::ItemEntity;
use crate::world::entity::player::ServerPlayer;
use crate::world::entity::{Entity, EntityId};
use crate::world::level::gameevent::SourceEntityInfo;

/// Capture what the vibration system reads from `entity` (see [`SourceEntityInfo`]).
pub fn capture_source_entity(level: &ServerLevel, entity: &dyn Entity) -> SourceEntityInfo {
    let is_server_player = entity.as_any().is::<ServerPlayer>();
    let projectile_owner = entity.as_projectile().and_then(|projectile| projectile.get_owner(level)).map(|owner| owner.uuid());
    SourceEntityInfo {
        id: entity.id(),
        uuid: entity.uuid(),
        is_server_player,
        spectator: entity.is_spectator(),
        stepping_carefully: entity.is_stepping_carefully(),
        dampens_vibrations: entity.dampens_vibrations(),
        player: try_get_player(level, entity, is_server_player),
        projectile_owner,
    }
}

/// `SculkShriekerBlockEntity.tryGetPlayer(sourceEntity)`: the player itself, the player controlling the entity, the
/// owning player of a projectile, the player that threw an item.
fn try_get_player(level: &ServerLevel, entity: &dyn Entity, is_server_player: bool) -> Option<EntityId> {
    let is_player = |id: EntityId| level.get_entity(id).is_some_and(|e| e.as_any().is::<ServerPlayer>());
    if is_server_player {
        return Some(entity.id());
    }
    if let Some(passenger) = entity.get_controlling_passenger().filter(|id| is_player(*id)) {
        return Some(passenger);
    }
    if let Some(owner) = entity.as_projectile().and_then(|projectile| projectile.get_owner(level)).filter(|owner| owner.as_any().is::<ServerPlayer>()) {
        return Some(owner.id());
    }
    entity.as_any().downcast_ref::<ItemEntity>().and_then(|item| item.get_owner(level)).filter(|id| is_player(*id))
}

/// `CriteriaTriggers.AVOID_VIBRATION.trigger(player)`.
// SEAM(advancements): `CriteriaTriggers::AVOID_VIBRATION.trigger(level, player)` once the trigger API exists.
pub fn trigger_avoid_vibration(_level: &mut ServerLevel, _player: EntityId) {}
