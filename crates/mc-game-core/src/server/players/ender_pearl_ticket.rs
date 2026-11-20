//! The server side of `ServerPlayer`'s ender pearls: `registerAndUpdateEnderPearlTicket` / `placeEnderPearlTicket` (the
//! chunk loading tickets that keep an in-flight pearl's surroundings loaded), the owner lookup of a pearl
//! (`findOwnerIncludingDeadPlayer`, in whichever level the owner is), `deregisterEnderPearl` and `loadAndSpawnEnderPearls`
//! (the join path that puts the saved pearls back into their levels).

use crate::server::MinecraftServer;
use crate::server::level::{AnyDimensionEntity, ServerLevel, TicketType};
use crate::world::block_entity::problem_reporter;
use crate::world::entity::factory::{self, EntitySpawnRequest};
use crate::world::entity::projectile::throwableitemprojectile::thrown_enderpearl::ThrownEnderpearl;
use crate::world::entity::{EntityId, ServerPlayer};
use mc_core::ident::Identifier;
use mc_core::pos::ChunkPos;
use mc_data::enums::EntitySpawnReason;
use mc_nbt::CompoundTag;
use uuid::Uuid;

/// `ServerPlayer.ENDER_PEARL_TICKET_RADIUS`.
pub const ENDER_PEARL_TICKET_RADIUS: i32 = 2;

/// `ServerPlayer.placeEnderPearlTicket(level, chunk)`: returns `TicketType.ENDER_PEARL.timeout()`.
pub fn place_ender_pearl_ticket(level: &mut ServerLevel, chunk: ChunkPos) -> i64 {
    level.add_ticket_with_radius(TicketType::ENDER_PEARL, chunk, ENDER_PEARL_TICKET_RADIUS);
    TicketType::ENDER_PEARL.timeout
}

/// The player that owns a pearl, as `findOwnerIncludingDeadPlayer` finds it from the pearl's level.
#[derive(Clone, Copy, Debug)]
pub struct PearlOwner {
    pub uuid: Uuid,
    pub alive: bool,
    pub won_game: bool,
    /// The owner's entity id when it is in the pearl's level (a killed player is out of the level's storage but still
    /// counts); `None` when it is in another level.
    pub in_level: Option<EntityId>,
}

impl PearlOwner {
    /// `getEntityInAnyDimension(uuid)`, else the player list; only a `ServerPlayer` is a pearl owner that matters here.
    pub fn find(level: &ServerLevel, uuid: Uuid) -> Option<Self> {
        let of_player = |p: &ServerPlayer, in_level| Self {
            uuid,
            alive: crate::world::entity::Entity::is_alive(p),
            won_game: p.won_game,
            in_level,
        };
        match level.get_entity_in_any_dimension(uuid) {
            Some(AnyDimensionEntity::Here(entity)) => {
                let player = entity.as_any().downcast_ref::<ServerPlayer>()?;
                return Some(of_player(player, Some(entity.id())));
            }
            Some(AnyDimensionEntity::Elsewhere(remote)) => {
                return remote.is_player.then_some(Self {
                    uuid,
                    alive: remote.alive,
                    won_game: remote.won_game,
                    in_level: None,
                });
            }
            None => {}
        }
        if let Some(dead) = level
            .dead_players
            .values()
            .find(|p| p.uuid() == uuid)
            .and_then(|p| p.as_any().downcast_ref::<ServerPlayer>())
        {
            return Some(of_player(
                dead,
                Some(crate::world::entity::Entity::id(dead)),
            ));
        }
        // a player that is on the player list but in no level: killed and waiting for the respawn, or back from the End credits
        let handle = level.get_player_in_any_dimension(uuid)?;
        let won_game = handle.shared.lock().won_game;
        (won_game || handle.dimension() != level.dimension).then_some(Self {
            uuid,
            alive: false,
            won_game,
            in_level: None,
        })
    }
}

/// `f` on the player `uuid` of this level: in the storage or killed and out of it.
pub(super) fn with_player_here<R>(
    level: &mut ServerLevel,
    uuid: Uuid,
    f: impl FnOnce(&mut ServerLevel, &mut ServerPlayer) -> R,
) -> Option<R> {
    if let Some(id) = level
        .entity_id_by_uuid(uuid)
        .filter(|&id| level.get_entity(id).is_some())
    {
        return level.with_player(id, f);
    }
    let id = level
        .dead_players
        .iter()
        .find(|(_, p)| p.uuid() == uuid)
        .map(|(&id, _)| id)?;
    let mut dead = level.dead_players.remove(&id)?;
    let result = dead
        .as_any_mut()
        .downcast_mut::<ServerPlayer>()
        .map(|p| f(level, p));
    level.dead_players.insert(id, dead);
    result
}

/// Runs `f` on the player `uuid` in whichever level it is: `level` is the level of the running action (out of `server`).
/// A level that a caller further up holds (so it is out of the server too) cannot be reached now: the work is deferred
/// on `level` and tried again the next time the server runs its queue.
pub fn deliver(
    server: &mut MinecraftServer,
    level: &mut ServerLevel,
    uuid: Uuid,
    f: impl FnOnce(&mut ServerLevel, &mut ServerPlayer) + Send + 'static,
) {
    let Some(dimension) = level
        .server_state
        .entities
        .get(uuid)
        .map(|remote| remote.dimension)
        .or_else(|| {
            level
                .get_player_in_any_dimension(uuid)
                .map(|handle| handle.dimension())
        })
    else {
        return;
    };
    if dimension == level.dimension {
        with_player_here(level, uuid, f);
        return;
    }
    let mut f = Some(f);
    let reached = server.with_level(&dimension, |_, other| {
        with_player_here(other, uuid, f.take().expect("runs once"))
    });
    if reached.is_none()
        && let Some(f) = f
    {
        level.defer_server_action(move |server, level| deliver(server, level, uuid, f));
    }
}

/// `ServerPlayer.registerAndUpdateEnderPearlTicket(pearl)`: `pearl` is the pearl entity (taken out of `level` while it
/// ticks) at `pearl_chunk`. An owner in another level registers it when the pearl's tick is over. Returns the ticks
/// until the pearl must refresh the ticket.
pub fn register_and_update_ender_pearl_ticket(
    level: &mut ServerLevel,
    owner: &PearlOwner,
    pearl: EntityId,
    pearl_chunk: ChunkPos,
) -> i64 {
    register_ender_pearl(level, owner.uuid, owner.in_level.is_some(), pearl);
    level.reset_empty_time();
    place_ender_pearl_ticket(level, pearl_chunk) - 1
}

/// The player `uuid` of this level that is in storage (not out of its slot) or killed and out of the storage.
fn player_mut(level: &mut ServerLevel, uuid: Uuid) -> Option<&mut ServerPlayer> {
    let live = level
        .entity_id_by_uuid(uuid)
        .filter(|&id| level.get_entity(id).is_some());
    if let Some(id) = live {
        return level
            .get_entity_mut(id)?
            .as_any_mut()
            .downcast_mut::<ServerPlayer>();
    }
    let dead = level
        .dead_players
        .iter()
        .find(|(_, p)| p.uuid() == uuid)
        .map(|(&id, _)| id)?;
    level
        .dead_players
        .get_mut(&dead)?
        .as_any_mut()
        .downcast_mut::<ServerPlayer>()
}

/// `owner.registerEnderPearl(pearl)` for `pearl` of `level`: at once when the owner is in `level` and reachable, else
/// when the server runs the queued action.
pub fn register_ender_pearl(level: &mut ServerLevel, owner: Uuid, in_level: bool, pearl: EntityId) {
    let dimension = level.dimension.clone();
    if in_level && let Some(player) = player_mut(level, owner) {
        player.register_ender_pearl(pearl, &dimension);
        return;
    }
    level.queue_server_action(move |server, level| {
        deliver(server, level, owner, move |_, p| {
            p.register_ender_pearl(pearl, &dimension)
        });
    });
}

/// `ThrownEnderpearl.deregisterFromCurrentOwner()` for the pearl `pearl` of `level` that `owner` threw.
pub fn deregister_ender_pearl(level: &mut ServerLevel, owner: Uuid, pearl: EntityId) {
    if let Some(player) = player_mut(level, owner) {
        player.deregister_ender_pearl(pearl);
        return;
    }
    level.queue_server_action(move |server, level| {
        deliver(server, level, owner, move |_, p| {
            p.deregister_ender_pearl(pearl)
        });
    });
}

/// `ServerPlayer.loadAndSpawnEnderPearls(input)`: spawns the pearls of the saved data of `player` (an entity of `level`)
/// into the level each one names. A pearl of another level is skipped (with a warning) when that level is unavailable.
pub fn load_and_spawn_ender_pearls(
    server: &mut MinecraftServer,
    level: &mut ServerLevel,
    player: EntityId,
) {
    let saved = level
        .get_entity_mut(player)
        .and_then(|e| e.as_any_mut().downcast_mut::<ServerPlayer>())
        .map(|p| std::mem::take(&mut p.saved_ender_pearls))
        .unwrap_or_default();
    for (index, tag) in saved {
        let Some(dimension) = tag
            .get_string("ender_pearl_dimension")
            .and_then(|s| Identifier::parse(s).ok())
        else {
            continue;
        };
        let spawned = if dimension == level.dimension {
            spawn_ender_pearl(level, &tag, index)
        } else if let Some(spawned) =
            server.with_level(&dimension, |_, other| spawn_ender_pearl(other, &tag, index))
        {
            spawned
        } else {
            log::warn!(
                "Trying to load ender pearl without level ({dimension}) being loaded, skipping"
            );
            continue;
        };
        // `ThrownEnderpearl.setOwner` registers the loaded pearl with its owner (the joining player)
        if let Some((pearl, owner)) = spawned
            && let Some(p) = level
                .get_entity_mut(player)
                .and_then(|e| e.as_any_mut().downcast_mut::<ServerPlayer>())
            && owner == Some(crate::world::entity::Entity::uuid(p))
        {
            p.register_ender_pearl(pearl, &dimension);
        }
    }
}

/// `loadAndSpawnEnderPearl`'s body: load the entity, add it with its saved UUID, place its ticket. The pearl's id and owner.
fn spawn_ender_pearl(
    level: &mut ServerLevel,
    tag: &CompoundTag,
    index: usize,
) -> Option<(EntityId, Option<Uuid>)> {
    let request = EntitySpawnRequest {
        reason: EntitySpawnReason::Load,
        ignore_checks: false,
    };
    let Some(pearl) = problem_reporter::with_child(
        || format!(".ender_pearls[{index}]"),
        || factory::create_and_load(tag, level, request),
    ) else {
        log::warn!(
            "Failed to spawn player ender pearl in level ({}), skipping",
            level.dimension
        );
        return None;
    };
    let chunk = pearl.base().chunk_position();
    let id = pearl.id();
    let owner = pearl
        .as_any()
        .downcast_ref::<ThrownEnderpearl>()
        .and_then(ThrownEnderpearl::owner_uuid);
    if level.add_fresh_entity(pearl) {
        place_ender_pearl_ticket(level, chunk);
        Some((id, owner))
    } else {
        log::warn!(
            "Failed to spawn player ender pearl in level ({}), skipping",
            level.dimension
        );
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::entity::Entity;
    use crate::world::entity::projectile::throwableitemprojectile::thrown_enderpearl::ThrownEnderpearl;
    use crate::world::item::ItemStack;
    use mc_data::{EntityType, Item};
    use mc_protocol::codec::GameProfile;

    const PLAYER_ID: EntityId = 100;

    fn level() -> ServerLevel {
        ServerLevel::detached(Identifier::parse("minecraft:overworld").unwrap())
    }

    /// A level with a player and one registered pearl at (3.5, 70, -2.5): `(level, pearl id)`.
    fn level_with_pearl() -> (ServerLevel, EntityId) {
        let mut level = level();
        let player = ServerPlayer::new(
            PLAYER_ID,
            level.dimension.clone(),
            GameProfile {
                id: uuid::Uuid::from_u128(7),
                name: "Steve".into(),
                properties: vec![],
            },
            Default::default(),
        );
        let mut pearl =
            ThrownEnderpearl::with_owner(&level, &player, &ItemStack::new(Item::ENDER_PEARL));
        pearl.set_pos(3.5, 70.0, -2.5);
        let pearl_id = pearl.id();
        level.chunk_blocking_mut(0, 0);
        level.chunk_blocking_mut(0, -1);
        level.add_player(Box::new(player));
        assert!(level.add_fresh_entity(Box::new(pearl)));
        let dimension = level.dimension.clone();
        level
            .get_entity_mut(PLAYER_ID)
            .and_then(|e| e.as_any_mut().downcast_mut::<ServerPlayer>())
            .unwrap()
            .register_ender_pearl(pearl_id, &dimension);
        (level, pearl_id)
    }

    fn saved_player_data(level: &ServerLevel) -> CompoundTag {
        level
            .get_entity(PLAYER_ID)
            .and_then(|e| e.as_any().downcast_ref::<ServerPlayer>())
            .unwrap()
            .save_player_data(&|id| level.entity_manager.get_any(id))
    }

    #[test]
    fn a_saved_pearl_loads_again_with_its_uuid_and_position() {
        let (level, pearl_id) = level_with_pearl();
        let uuid = level.entity_manager.get_any(pearl_id).unwrap().uuid();
        let data = saved_player_data(&level);
        let saved = data.get_list("ender_pearls").unwrap();
        assert_eq!(saved.len(), 1);
        let tag = saved.iter().next().unwrap().as_compound().unwrap();
        assert_eq!(
            tag.get_string("ender_pearl_dimension"),
            Some("minecraft:overworld")
        );

        let fresh = self::level();
        let mut loaded = factory::create(
            EntityType::ENDER_PEARL,
            &fresh,
            EntitySpawnRequest {
                reason: EntitySpawnReason::Load,
                ignore_checks: false,
            },
        )
        .expect("created");
        loaded.load(&fresh, tag).expect("loaded");
        assert_eq!(loaded.uuid(), uuid);
        assert_eq!(loaded.base().x(), 3.5);
        assert_eq!(loaded.base().y(), 70.0);
        assert_eq!(loaded.base().z(), -2.5);
    }

    #[test]
    fn a_removed_pearl_is_not_saved() {
        let (mut level, pearl_id) = level_with_pearl();
        level.with_entity(pearl_id, |_, e| e.discard());
        assert!(saved_player_data(&level).get_list("ender_pearls").is_none());
    }
}
