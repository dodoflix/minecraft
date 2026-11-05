//! The `PlayerList` operations that need the server and a level: saving, leaving, op/deop, distances, resource
//! reloads. Free functions over `&mut MinecraftServer` (a method cannot borrow `server.player_list` and `server`).

use super::join::{JoinHost, send_player_permission_level, send_player_permission_level_with};
use super::name_and_id::NameAndId;
use super::player_handle::PlayerHandle;
use super::server_join_host::ServerJoinHost;
use crate::server::MinecraftServer;
use crate::server::level::ServerLevel;
use crate::server::permissions::LevelBasedPermissionSet;
use crate::world::entity::entity::Entity;
use crate::world::entity::{EntityId, RemovalReason, ServerPlayer};
use mc_core::ident::Identifier;
use mc_protocol::packets::play::{ClientboundSetChunkCacheRadiusPacket, ClientboundSetSimulationDistancePacket};

/// `save(player)`: the player data file, then the statistics and advancements files.
pub fn save_player(server: &mut MinecraftServer, level: &ServerLevel, player: &ServerPlayer) {
    let Some(handle) = player.handle.clone() else { return };
    let tag = player.save_player_data(&|id| level.get_entity(id));
    server.player_list.save_player_data(&handle, tag);
    server.player_list.save_player_files(&handle);
}

/// `saveAll()`: every player that is in its level (a player whose packet is being handled right now is saved by that
/// handler's own `save_player`).
pub fn save_all(server: &mut MinecraftServer) {
    for handle in server.player_list.get_players() {
        let (id, dimension) = (handle.id, handle.dimension());
        server.with_level(&dimension, |server, level| {
            level.with_player(id, |level, player| save_player(server, level, player));
        });
    }
}

/// The ids of `root` and everything riding it (`getPassengersAndSelf`), depth first.
fn passengers_and_self(level: &ServerLevel, root: EntityId) -> Vec<EntityId> {
    let mut out = vec![root];
    let mut i = 0;
    while i < out.len() {
        if let Some(entity) = level.get_entity(out[i]) {
            out.extend(entity.base().passengers.iter().copied());
        }
        i += 1;
    }
    out
}

/// `remove(player)`: `player` is taken out of `level` (it is the player whose connection ended); the caller puts it
/// back, which applies the removal.
pub fn remove(server: &mut MinecraftServer, level: &mut ServerLevel, player: &mut ServerPlayer) {
    let Some(handle) = player.handle.clone() else { return };
    server.player_list.get_player_stats(&handle).lock().increment(crate::stats::stats::Stats::LEAVE_GAME, 1);
    save_player(server, level, player);
    if let Some(vehicle) = player.base().vehicle {
        // root vehicle with exactly one player passenger (this one): it goes down with the player
        let mut root = vehicle;
        while let Some(next) = level.get_entity(root).and_then(|e| e.base().vehicle) {
            root = next;
        }
        let members = passengers_and_self(level, root);
        let players = members.iter().filter(|&&id| level.get_entity(id).is_some_and(|e| e.is_player())).count();
        if players == 0 {
            log::debug!("Removing player mount");
            for id in members {
                level.with_entity(id, |_, e| e.remove(RemovalReason::UnloadedWithPlayer));
            }
        }
        player.base_mut().vehicle = None;
    }
    player.remove(RemovalReason::UnloadedWithPlayer);
    server.player_list.remove_player(&handle);
    server.custom_boss_events().on_player_disconnect(&handle);
    server.player_list.broadcast_player_info_remove(handle.uuid);
}

/// `sendPlayerPermissionLevel(player)` for a player that is in its level.
pub fn send_permission_level(server: &mut MinecraftServer, player: &PlayerHandle) {
    let dimension = player.dimension();
    with_host(server, &dimension, |host| send_player_permission_level(host, player));
}

fn with_host<R>(server: &mut MinecraftServer, dimension: &Identifier, f: impl FnOnce(&mut dyn JoinHost) -> R) -> Option<R> {
    server.with_level(dimension, |server, level| f(&mut ServerJoinHost { server, level }))
}

/// `op(nameAndId, permissions, canBypassPlayerLimit)`.
pub fn op(server: &mut MinecraftServer, name_and_id: NameAndId, permissions: Option<LevelBasedPermissionSet>, can_bypass_player_limit: Option<bool>) {
    let id = name_and_id.id;
    server.player_list.op_entry(name_and_id, permissions, can_bypass_player_limit);
    if let Some(player) = server.player_list.get_player(&id).cloned() {
        send_permission_level(server, &player);
    }
}

/// `deop(nameAndId)`.
pub fn deop(server: &mut MinecraftServer, name_and_id: &NameAndId) {
    if server.player_list.ops.remove(name_and_id)
        && let Some(player) = server.player_list.get_player(&name_and_id.id).cloned()
    {
        send_permission_level(server, &player);
    }
}

/// `sendPlayerPermissionLevel(player, permissions)`.
pub fn send_permission_level_with(server: &mut MinecraftServer, player: &PlayerHandle, permissions: LevelBasedPermissionSet) {
    let dimension = player.dimension();
    with_host(server, &dimension, |host| send_player_permission_level_with(host, player, permissions));
}

/// `setViewDistance(viewDistance)`.
pub fn set_view_distance(server: &mut MinecraftServer, view_distance: i32) {
    server.player_list.set_view_distance_value(view_distance);
    server.player_list.broadcast_all(&ClientboundSetChunkCacheRadiusPacket { radius: view_distance });
    for dimension in server.level_keys() {
        if let Some(level) = server.level_mut(&dimension) {
            level.chunk_source.set_view_distance(view_distance);
        }
    }
}

/// `setSimulationDistance(simulationDistance)`.
pub fn set_simulation_distance(server: &mut MinecraftServer, simulation_distance: i32) {
    server.player_list.set_simulation_distance_value(simulation_distance);
    server.player_list.broadcast_all(&ClientboundSetSimulationDistancePacket { simulation_distance });
    for dimension in server.level_keys() {
        if let Some(level) = server.level_mut(&dimension) {
            level.chunk_source.set_simulation_distance(simulation_distance);
        }
    }
}

/// `reloadResources()`: the new tags and the recipe sync (the player advancements reload with the advancement
/// manager, a later wave).
pub fn reload_resources(server: &mut MinecraftServer) {
    use crate::registry::synchronization as sync;
    use mc_protocol::packets::common::TagNetworkPayload;
    let tags = sync::serialize_tags_to_network(&server.registries)
        .into_iter()
        .map(|(registry, payload)| {
            (
                Identifier::parse(&registry).unwrap_or_else(|e| panic!("invalid registry key {registry}: {e}")),
                TagNetworkPayload { tags: payload.tags.into_iter().map(|(id, ids)| (id, ids.into_iter().map(|i| i as i32).collect())).collect() },
            )
        })
        .collect();
    server.player_list.broadcast_all(&mc_protocol::packets::play::ClientboundUpdateTagsPacket { tags });
    for handle in server.player_list.get_players() {
        let (dimension, id) = (handle.dimension(), handle.id);
        server.with_level(&dimension, |server, level| {
            handle.connection.send(&ServerJoinHost { server, level }.update_recipes_packet());
            level.with_player(id, |_, player| super::join::send_initial_recipe_book(player));
        });
    }
}
