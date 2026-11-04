//! `ChunkMap.entityMap` + `ChunkMap.TrackedEntity`: which players see which entity, pairing/unpairing, per-tick
//! `ServerEntity.sendChanges`.

use super::server_entity::{Payload, ServerEntity, Synchronizer, UpdateInterval, bundle};
use crate::server::network::ConnectionSender;
use crate::world::entity::{Entity, EntityId, ServerPlayer};
use crate::world::level::entity::PersistentEntitySectionManager;
use indexmap::IndexMap;
use mc_core::phys::Vec3;
use mc_core::pos::SectionPos;

impl TrackingEnv for super::chunk_map::ChunkMap {
    fn player_view_distance(&self, _player_id: EntityId, requested_view_distance: i32) -> i32 {
        mc_core::math::mth::clamp_i32(requested_view_distance, 2, self.server_view_distance())
    }
    fn is_chunk_tracked(&self, player_id: EntityId, chunk_x: i32, chunk_z: i32) -> bool {
        // inherent `ChunkMap::is_chunk_tracked` takes precedence over this trait method
        self.is_chunk_tracked(player_id, chunk_x, chunk_z)
    }
    /// `MinecraftServer.getScaledTrackingDistance` is the identity in 26.3.
    fn scaled_tracking_distance(&self, range: i32) -> i32 {
        range
    }
    fn in_entity_ticking_range(&self, chunk_key: i64) -> bool {
        self.distance_manager.in_entity_ticking_range(chunk_key)
    }
}

/// What the tracker needs from the chunk map / server (`ChunkMap.getPlayerViewDistance`, `isChunkTracked`,
/// `MinecraftServer.getScaledTrackingDistance`, `DistanceManager.inEntityTickingRange`).
pub trait TrackingEnv {
    /// `getPlayerViewDistance(player)` for a player requesting `requested_view_distance`.
    fn player_view_distance(&self, player_id: EntityId, requested_view_distance: i32) -> i32;
    /// `isChunkTracked(player, chunkX, chunkZ)`.
    fn is_chunk_tracked(&self, player_id: EntityId, chunk_x: i32, chunk_z: i32) -> bool;
    /// `MinecraftServer.getScaledTrackingDistance(range)`.
    fn scaled_tracking_distance(&self, range: i32) -> i32;
    /// `DistanceManager.inEntityTickingRange(chunkKey)`.
    fn in_entity_ticking_range(&self, chunk_key: i64) -> bool;
}

/// The player data `updatePlayer` reads (the player may be taken out of the level while it is handled).
#[derive(Clone)]
pub struct TrackerPlayer {
    pub id: EntityId,
    pub position: Vec3,
    pub requested_view_distance: i32,
    pub connection: Option<ConnectionSender>,
}

impl TrackerPlayer {
    pub fn of(player: &ServerPlayer) -> Self {
        Self { id: player.base_id(), position: player.player.living.entity.position(), requested_view_distance: player.requested_view_distance, connection: player.connection.clone() }
    }
}

struct TrackedEntity {
    server_entity: ServerEntity,
    entity_id: EntityId,
    is_player: bool,
    range: i32,
    last_section_pos: SectionPos,
    /// `seenBy`: tracking players (insertion order) with their connections.
    seen_by: IndexMap<EntityId, Option<ConnectionSender>>,
    self_connection: Option<ConnectionSender>,
}

fn send(connection: &Option<ConnectionSender>, payload: &Payload) {
    if let Some(c) = connection {
        c.send_payload(payload.clone());
    }
}

struct SeenBySync<'a> {
    seen_by: &'a IndexMap<EntityId, Option<ConnectionSender>>,
    self_connection: &'a Option<ConnectionSender>,
}

impl Synchronizer for SeenBySync<'_> {
    fn send_to_tracking_players(&mut self, payload: Payload) {
        for c in self.seen_by.values() {
            send(c, &payload);
        }
    }
    fn send_to_tracking_players_and_self(&mut self, payload: Payload) {
        self.send_to_tracking_players(payload.clone());
        send(self.self_connection, &payload);
    }
}

impl TrackedEntity {
    fn effective_range(&self, entities: &PersistentEntitySectionManager, env: &dyn TrackingEnv) -> i32 {
        let mut effective = self.range;
        let mut stack: Vec<EntityId> = entities.get_any(self.entity_id).map(|e| e.base().passengers.clone()).unwrap_or_default();
        while let Some(id) = stack.pop() {
            if let Some(p) = entities.get_any(id) {
                effective = effective.max(p.get_type().data().client_tracking_range * 16);
                stack.extend(p.base().passengers.iter().copied());
            }
        }
        env.scaled_tracking_distance(effective)
    }

    fn remove_player(&mut self, player_id: EntityId, player_conn: Option<&ConnectionSender>) {
        if self.seen_by.shift_remove(&player_id).is_some() {
            if let Some(c) = player_conn {
                c.send_payload(ServerEntity::remove_pairing_packet(self.entity_id));
            }
        }
    }

    fn update_player(&mut self, entities: &mut PersistentEntitySectionManager, env: &dyn TrackingEnv, player: &TrackerPlayer) {
        if player.id == self.entity_id {
            return;
        }
        let Some(entity) = entities.get_any(self.entity_id) else { return };
        let delta = player.position.subtract(entity.position());
        let view_distance = env.player_view_distance(player.id, player.requested_view_distance);
        let visible_range = f64::from(self.effective_range(entities, env).min(view_distance * 16));
        let dist_sqr = delta.x * delta.x + delta.z * delta.z;
        let entity = entities.get_any(self.entity_id).expect("checked above");
        let chunk = entity.chunk_position();
        let visible = dist_sqr <= visible_range * visible_range && entity.broadcast_to_player(player.id) && env.is_chunk_tracked(player.id, chunk.x, chunk.z);
        if visible {
            if !self.seen_by.contains_key(&player.id) {
                self.seen_by.insert(player.id, player.connection.clone());
                self.add_pairing(entities, player);
            }
        } else {
            self.remove_player(player.id, player.connection.as_ref());
        }
    }

    /// `ServerEntity.addPairing(player)`.
    fn add_pairing(&mut self, entities: &mut PersistentEntitySectionManager, player: &TrackerPlayer) {
        let vehicle = entities
            .get_any(self.entity_id)
            .and_then(|e| e.base().vehicle)
            .and_then(|v| entities.get_any(v).map(|v| (v.id(), v.base().passengers.clone())));
        let leash = entities.get_any(self.entity_id).and_then(|e| e.leash_holder());
        let Some(entity) = entities.get_any_mut(self.entity_id) else { return };
        let packets = self.server_entity.send_pairing_data(entity, vehicle, leash);
        if let Some(c) = &player.connection {
            for p in bundle(packets) {
                c.send_payload(p);
            }
        }
        if let Some(p) = entities.get_any_mut(self.entity_id) {
            p.start_seen_by_player(player.id);
        }
    }

    fn update_players(&mut self, entities: &mut PersistentEntitySectionManager, env: &dyn TrackingEnv, players: &[TrackerPlayer]) {
        for p in players {
            self.update_player(entities, env, p);
        }
    }
}

/// `ChunkMap.entityMap`.
#[derive(Default)]
pub struct EntityTracker {
    entity_map: IndexMap<EntityId, TrackedEntity>,
}

/// Snapshot of the players of a level for tracker updates (all players must be present in storage).
pub fn tracker_players(entities: &PersistentEntitySectionManager, player_ids: &[EntityId]) -> Vec<TrackerPlayer> {
    player_ids.iter().filter_map(|&id| entities.get_any(id).and_then(|e| e.as_any().downcast_ref::<ServerPlayer>()).map(TrackerPlayer::of)).collect()
}

impl EntityTracker {
    pub fn has_entity_with_id(&self, id: EntityId) -> bool {
        self.entity_map.contains_key(&id)
    }

    /// `ChunkMap.addEntity(entity)` (the `updatePlayerStatus` part belongs to the chunk map).
    pub fn add_entity(&mut self, entities: &mut PersistentEntitySectionManager, env: &dyn TrackingEnv, players: &[TrackerPlayer], entity_id: EntityId) {
        let Some(entity) = entities.get_any(entity_id) else { return };
        if entity.is_ender_dragon_part() {
            return;
        }
        let ty = entity.get_type();
        let range = ty.data().client_tracking_range * 16;
        if range == 0 {
            return;
        }
        assert!(!self.entity_map.contains_key(&entity_id), "Entity is already tracked!");
        let interval = if ty.data().update_interval != i32::MAX { UpdateInterval::Periodic(ty.data().update_interval) } else { UpdateInterval::Never };
        let player = entity.as_any().downcast_ref::<ServerPlayer>().map(TrackerPlayer::of);
        let mut tracked = TrackedEntity {
            server_entity: ServerEntity::new(entity, interval, ty.data().track_deltas),
            entity_id,
            is_player: player.is_some(),
            range,
            last_section_pos: SectionPos::of_block_pos(&entity.block_position()),
            seen_by: IndexMap::new(),
            self_connection: player.as_ref().and_then(|p| p.connection.clone()),
        };
        tracked.update_players(entities, env, players);
        self.entity_map.insert(entity_id, tracked);
        if let Some(player) = player {
            let others: Vec<EntityId> = self.entity_map.keys().copied().filter(|&id| id != entity_id).collect();
            for id in others {
                if let Some(t) = self.entity_map.get_mut(&id) {
                    t.update_player(entities, env, &player);
                }
            }
        }
    }

    /// `ChunkMap.removeEntity(entity)`; `player` is the removed entity if it was a player.
    pub fn remove_entity(&mut self, entity_id: EntityId, player: Option<&TrackerPlayer>) {
        if let Some(player) = player {
            for t in self.entity_map.values_mut() {
                t.remove_player(player.id, player.connection.as_ref());
            }
        }
        if let Some(t) = self.entity_map.shift_remove(&entity_id) {
            // `broadcastRemoved`
            for c in t.seen_by.values() {
                send(c, &ServerEntity::remove_pairing_packet(entity_id));
            }
        }
    }

    /// `ChunkMap.move(player)` tracker part: `player` just moved.
    pub fn move_player(&mut self, entities: &mut PersistentEntitySectionManager, env: &dyn TrackingEnv, all_players: &[TrackerPlayer], player: &TrackerPlayer) {
        let ids: Vec<EntityId> = self.entity_map.keys().copied().collect();
        for id in ids {
            let Some(t) = self.entity_map.get_mut(&id) else { continue };
            if t.entity_id == player.id {
                t.update_players(entities, env, all_players);
            } else {
                t.update_player(entities, env, player);
            }
        }
    }

    /// `ChunkMap.tick()` entity part (without `updateChunkTracking`).
    pub fn tick(&mut self, entities: &mut PersistentEntitySectionManager, env: &dyn TrackingEnv, players: &[TrackerPlayer]) {
        let mut moved_players: Vec<TrackerPlayer> = Vec::new();
        let ids: Vec<EntityId> = self.entity_map.keys().copied().collect();
        for id in ids {
            let Some(t) = self.entity_map.get_mut(&id) else { continue };
            let Some(entity) = entities.get_any(id) else { continue };
            let new_pos = SectionPos::of_block_pos(&entity.block_position());
            let section_changed = t.last_section_pos != new_pos;
            let needs_sync = entity.base().needs_sync;
            if section_changed {
                t.update_players(entities, env, players);
                if t.is_player {
                    if let Some(p) = entities.get_any(id).and_then(|e| e.as_any().downcast_ref::<ServerPlayer>()).map(TrackerPlayer::of) {
                        moved_players.push(p);
                    }
                }
                t.last_section_pos = new_pos;
            }
            if section_changed || needs_sync || env.in_entity_ticking_range(new_pos.chunk().pack()) {
                let Some(entity) = entities.get_any_mut(id) else { continue };
                let TrackedEntity { server_entity, seen_by, self_connection, .. } = t;
                let mut sync = SeenBySync { seen_by, self_connection };
                server_entity.send_changes(entity, &mut sync);
            }
        }
        if !moved_players.is_empty() {
            for t in self.entity_map.values_mut() {
                t.update_players(entities, env, &moved_players);
            }
        }
    }

    /// `ChunkMap.sendToTrackingPlayers(entity, packet)`.
    pub fn send_to_tracking_players(&self, entity_id: EntityId, payload: &Payload) {
        if let Some(t) = self.entity_map.get(&entity_id) {
            for c in t.seen_by.values() {
                send(c, payload);
            }
        }
    }

    /// `ChunkMap.sendToTrackingPlayersFiltered(entity, packet, predicate on the player id)`.
    pub fn send_to_tracking_players_filtered(&self, entity_id: EntityId, payload: &Payload, mut predicate: impl FnMut(EntityId) -> bool) {
        if let Some(t) = self.entity_map.get(&entity_id) {
            for (&pid, c) in &t.seen_by {
                if predicate(pid) {
                    send(c, payload);
                }
            }
        }
    }

    /// `ChunkMap.sendToTrackingPlayersAndSelf(entity, packet)`.
    pub fn send_to_tracking_players_and_self(&self, entity_id: EntityId, payload: &Payload) {
        if let Some(t) = self.entity_map.get(&entity_id) {
            for c in t.seen_by.values() {
                send(c, payload);
            }
            send(&t.self_connection, payload);
        }
    }

    /// `ChunkMap.isTrackedByAnyPlayer(entity)`.
    pub fn is_tracked_by_any_player(&self, entity_id: EntityId) -> bool {
        self.entity_map.get(&entity_id).is_some_and(|t| !t.seen_by.is_empty())
    }

    /// `ChunkMap.forEachEntityTrackedBy(player)`.
    pub fn entities_tracked_by(&self, player_id: EntityId) -> Vec<EntityId> {
        self.entity_map.values().filter(|t| t.seen_by.contains_key(&player_id)).map(|t| t.entity_id).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::network::Outbound;
    use crate::world::entity::plain::PlainEntity;
    use crate::world::level::entity::Visibility;
    use mc_core::pos::chunk_pos::ChunkPos;
    use mc_data::EntityType;
    use mc_protocol::codec::{GameProfile, read_varint};
    use mc_protocol::packet::Packet;
    use mc_protocol::packets::play::{
        ClientboundAddEntityPacket, ClientboundBundleDelimiterPacket, ClientboundMoveEntityPosPacket, ClientboundRemoveEntitiesPacket, ClientboundTeleportEntityPacket,
    };
    use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

    struct Env;
    impl TrackingEnv for Env {
        fn player_view_distance(&self, _: EntityId, requested: i32) -> i32 {
            requested.max(2)
        }
        fn is_chunk_tracked(&self, _: EntityId, _: i32, _: i32) -> bool {
            true
        }
        fn scaled_tracking_distance(&self, range: i32) -> i32 {
            range
        }
        fn in_entity_ticking_range(&self, _: i64) -> bool {
            true
        }
    }

    fn ids(rx: &mut UnboundedReceiver<Outbound>) -> Vec<i32> {
        let mut out = Vec::new();
        while let Ok(Outbound::Payload(p)) = rx.try_recv() {
            out.push(read_varint(&mut &p[..]).unwrap());
        }
        out
    }

    fn setup() -> (PersistentEntitySectionManager, UnboundedReceiver<Outbound>, TrackerPlayer) {
        let mut m = PersistentEntitySectionManager::new(None);
        m.update_chunk_status(ChunkPos::new(0, 0), Visibility::Ticking);
        let (tx, rx) = unbounded_channel();
        let mut player = ServerPlayer::new(1, mc_core::ident::Identifier::parse("minecraft:overworld").unwrap(), GameProfile { id: uuid::Uuid::from_u128(7), name: "Steve".into(), properties: vec![] }, Default::default());
        player.connection = Some(ConnectionSender::new(tx));
        player.set_pos(10.0, 64.0, 0.0);
        let tp = TrackerPlayer::of(&player);
        m.add_new_entity(Box::new(player)).ok().unwrap();
        (m, rx, tp)
    }

    #[test]
    fn pairing_then_movement_deltas_then_removal() {
        let (mut m, mut rx, tp) = setup();
        let mut pig = PlainEntity::new(EntityType::PIG, 2);
        pig.set_pos(1.0, 64.0, 1.0);
        m.add_new_entity(Box::new(pig)).ok().unwrap();
        let mut tracker = EntityTracker::default();
        tracker.add_entity(&mut m, &Env, std::slice::from_ref(&tp), 2);
        assert!(tracker.is_tracked_by_any_player(2));
        // bundle delimiter, add_entity, bundle delimiter
        assert_eq!(ids(&mut rx), vec![ClientboundBundleDelimiterPacket::ID, ClientboundAddEntityPacket::ID, ClientboundBundleDelimiterPacket::ID]);

        // plain entities are not living: ticking with no change sends nothing until the update interval (3) hits, then a delta
        let mut e = m.take(2).unwrap();
        e.set_pos(2.0, 64.0, 1.0);
        m.put_back(2, e);
        for _ in 0..3 {
            tracker.tick(&mut m, &Env, std::slice::from_ref(&tp));
        }
        let got = ids(&mut rx);
        assert!(got.contains(&ClientboundMoveEntityPosPacket::ID), "{got:?}");
        assert!(!got.contains(&ClientboundTeleportEntityPacket::ID));

        tracker.remove_entity(2, None);
        assert_eq!(ids(&mut rx), vec![ClientboundRemoveEntitiesPacket::ID]);
    }

    #[test]
    fn out_of_range_player_does_not_track() {
        let (mut m, mut rx, mut tp) = setup();
        tp.position = Vec3::new(5000.0, 64.0, 0.0);
        let mut pig = PlainEntity::new(EntityType::PIG, 2);
        pig.set_pos(1.0, 64.0, 1.0);
        m.add_new_entity(Box::new(pig)).ok().unwrap();
        let mut tracker = EntityTracker::default();
        tracker.add_entity(&mut m, &Env, std::slice::from_ref(&tp), 2);
        assert!(!tracker.is_tracked_by_any_player(2));
        assert!(ids(&mut rx).is_empty());
    }
}
