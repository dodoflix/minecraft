//! The server side of `ServerPlayer`'s ender pearls: `registerAndUpdateEnderPearlTicket` / `placeEnderPearlTicket` (the
//! chunk loading tickets that keep an in-flight pearl's surroundings loaded) and `loadAndSpawnEnderPearls` (the join
//! path that puts the saved pearls back into their levels).

use crate::server::MinecraftServer;
use crate::server::level::{ServerLevel, TicketType};
use crate::world::entity::factory::{self, EntitySpawnRequest};
use crate::world::entity::{EntityId, ServerPlayer};
use mc_core::ident::Identifier;
use mc_core::pos::ChunkPos;
use mc_data::enums::EntitySpawnReason;
use mc_nbt::CompoundTag;

/// `ServerPlayer.ENDER_PEARL_TICKET_RADIUS`.
pub const ENDER_PEARL_TICKET_RADIUS: i32 = 2;

/// `ServerPlayer.placeEnderPearlTicket(level, chunk)`: returns `TicketType.ENDER_PEARL.timeout()`.
pub fn place_ender_pearl_ticket(level: &mut ServerLevel, chunk: ChunkPos) -> i64 {
    level.add_ticket_with_radius(TicketType::ENDER_PEARL, chunk, ENDER_PEARL_TICKET_RADIUS);
    TicketType::ENDER_PEARL.timeout
}

/// `ServerPlayer.registerAndUpdateEnderPearlTicket(pearl)`: `owner` is the player entity in `level`, `pearl` the pearl
/// entity (taken out of the level while it ticks) at `pearl_chunk`. Returns the ticks until the pearl must refresh it.
pub fn register_and_update_ender_pearl_ticket(
    level: &mut ServerLevel,
    owner: EntityId,
    pearl: EntityId,
    pearl_chunk: ChunkPos,
) -> i64 {
    // a killed player is not in the level's storage but still registers its pearls
    let entity = match level.get_entity_mut(owner) {
        Some(e) => Some(e),
        None => level.dead_players.get_mut(&owner).map(|p| &mut **p),
    };
    if let Some(player) = entity.and_then(|e| e.as_any_mut().downcast_mut::<ServerPlayer>()) {
        player.register_ender_pearl(pearl);
    }
    level.reset_empty_time();
    place_ender_pearl_ticket(level, pearl_chunk) - 1
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
    for tag in saved {
        let Some(dimension) = tag
            .get_string("ender_pearl_dimension")
            .and_then(|s| Identifier::parse(s).ok())
        else {
            continue;
        };
        if dimension == level.dimension {
            spawn_ender_pearl(level, &tag);
        } else if server
            .with_level(&dimension, |_, other| spawn_ender_pearl(other, &tag))
            .is_none()
        {
            log::warn!(
                "Trying to load ender pearl without level ({dimension}) being loaded, skipping"
            );
        }
    }
}

/// `loadAndSpawnEnderPearl`'s body: load the entity, add it with its saved UUID, place its ticket.
fn spawn_ender_pearl(level: &mut ServerLevel, tag: &CompoundTag) {
    let request = EntitySpawnRequest {
        reason: EntitySpawnReason::Load,
        ignore_checks: false,
    };
    let Some(pearl) = factory::create_and_load(tag, level, request) else {
        log::warn!(
            "Failed to spawn player ender pearl in level ({}), skipping",
            level.dimension
        );
        return;
    };
    let chunk = pearl.base().chunk_position();
    if level.add_fresh_entity(pearl) {
        place_ender_pearl_ticket(level, chunk);
    } else {
        log::warn!(
            "Failed to spawn player ender pearl in level ({}), skipping",
            level.dimension
        );
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
        level
            .get_entity_mut(PLAYER_ID)
            .and_then(|e| e.as_any_mut().downcast_mut::<ServerPlayer>())
            .unwrap()
            .register_ender_pearl(pearl_id);
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
        loaded.load(tag).expect("loaded");
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
