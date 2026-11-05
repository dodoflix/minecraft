//! The entity half of `ServerLevel`: id allocation, adding, lookups, queries, the take-out loop and the
//! `ServerLevel.EntityCallbacks` (applied from the manager's event queue).

use super::entity_tracker::{Entities, TrackerPlayer};
use super::ServerLevel;
use crate::world::entity::{Entity, EntityId, RemovalReason};
use crate::world::level::entity::EntityLevelEvent;
use mc_core::phys::AABB;
use mc_data::EntityType;
use std::ops::ControlFlow;
use std::sync::atomic::{AtomicI32, Ordering};
use uuid::Uuid;

/// `ServerLevel.ENTITY_COUNTER` (static: shared by all levels).
static ENTITY_COUNTER: AtomicI32 = AtomicI32::new(0);

/// `ServerLevel.ENTITY_COUNTER.incrementAndGet()` for entities created off the game thread (worldgen): no level to
/// check for taken ids (they are saved as NBT and get a fresh id when loaded).
pub fn next_detached_entity_id() -> EntityId {
    ENTITY_COUNTER.fetch_add(1, Ordering::Relaxed).wrapping_add(1)
}

impl ServerLevel {
    /// `getNextEntityId()`.
    pub fn next_entity_id(&self) -> EntityId {
        let mut id = 0;
        while id == 0 || self.entity_manager.has_entity_with_id(id) {
            id = ENTITY_COUNTER.fetch_add(1, Ordering::Relaxed).wrapping_add(1);
        }
        id
    }

    /// `EntityType.canSpawn(level)`: the type's required features are enabled and (unless allowed in peaceful) the
    /// difficulty is not peaceful.
    pub fn can_spawn_entity_type(&self, ty: EntityType) -> bool {
        let data = ty.data();
        let required: Vec<mc_core::ident::Identifier> = data.required_features.iter().filter_map(|n| mc_core::ident::Identifier::try_parse(n)).collect();
        if !mc_world::level_storage::FeatureFlagSet::from_names(required.iter(), |_| {}).is_subset_of(self.tick_state.enabled_features) {
            return false;
        }
        data.allowed_in_peaceful || self.difficulty() != mc_world::level_storage::Difficulty::Peaceful
    }

    // ---- adding ---------------------------------------------------------------------------------------------

    /// `addFreshEntity(entity)` (= `addEntity`): false if the entity is already marked removed or its UUID exists.
    pub fn add_fresh_entity(&mut self, entity: Box<dyn Entity>) -> bool {
        if entity.is_removed() {
            log::warn!("Tried to add entity {} but it was marked as removed already", entity.get_type().data().name);
            return false;
        }
        let ok = self.entity_manager.add_new_entity(entity).is_ok();
        self.process_entity_events();
        ok
    }

    /// `addFreshEntityWithPassengers(entity)`: `entities` is the entity followed by its passengers (as produced by
    /// the loader, `getSelfAndPassengers()` order).
    pub fn add_fresh_entity_with_passengers(&mut self, entities: Vec<Box<dyn Entity>>) {
        for e in entities {
            self.add_fresh_entity(e);
        }
    }

    /// `tryAddFreshEntityWithPassengers(entity)`.
    pub fn try_add_fresh_entity_with_passengers(&mut self, entities: Vec<Box<dyn Entity>>) -> bool {
        if entities.iter().any(|e| self.entity_manager.is_loaded(e.uuid())) {
            return false;
        }
        self.add_fresh_entity_with_passengers(entities);
        true
    }

    /// `addPlayer(player)` (private in vanilla): a duplicate UUID force-removes the old player first.
    pub fn add_player(&mut self, player: Box<dyn Entity>) {
        if let Some(existing) = self.entity_manager.id_by_uuid(player.uuid()) {
            log::warn!("Force-added player with duplicate UUID {}", player.uuid());
            self.remove_player_immediately(existing, RemovalReason::Discarded);
        }
        let _ = self.entity_manager.add_new_entity(player);
        self.process_entity_events();
    }

    /// `removePlayerImmediately(player, reason)` = `player.remove(reason)`.
    pub fn remove_player_immediately(&mut self, id: EntityId, reason: RemovalReason) {
        self.with_entity(id, |_, e| e.remove(reason));
    }

    // ---- lookups ----------------------------------------------------------------------------------------------

    /// `getEntity(id)`: a tracked entity that is not currently taken out.
    pub fn get_entity(&self, id: EntityId) -> Option<&dyn Entity> {
        self.entity_manager.get(id)
    }

    pub fn get_entity_by_uuid(&self, uuid: Uuid) -> Option<&dyn Entity> {
        self.entity_manager.get_by_uuid(uuid)
    }

    /// Mutable lookup; call [`ServerLevel::sync_entity`] after changing position/removal state, or use
    /// [`ServerLevel::with_entity`].
    pub fn get_entity_mut(&mut self, id: EntityId) -> Option<&mut dyn Entity> {
        self.entity_manager.get_mut(id)
    }

    pub fn is_entity_loaded(&self, uuid: Uuid) -> bool {
        self.entity_manager.is_loaded(uuid)
    }

    /// `players()`: ids of the players of this level.
    pub fn players(&self) -> &[EntityId] {
        &self.players
    }

    // ---- take-out / sync --------------------------------------------------------------------------------------

    /// Take an entity out of its slot (lookups return `None` while it is out). Pair with [`ServerLevel::put_back_entity`].
    pub fn take_entity(&mut self, id: EntityId) -> Option<Box<dyn Entity>> {
        let entity = self.entity_manager.take(id)?;
        if let Some(p) = entity.as_any().downcast_ref::<crate::world::entity::ServerPlayer>() {
            self.taken_out_players.insert(id, super::TrackerPlayer::of(p));
        }
        Some(entity)
    }

    /// Put a taken-out entity back and apply removal / section moves (`EntityInLevelCallback`).
    pub fn put_back_entity(&mut self, id: EntityId, entity: Box<dyn Entity>) {
        self.taken_out_players.remove(&id);
        self.entity_manager.put_back(id, entity);
        self.process_entity_events();
        if let Some(ops) = self.deferred_level_ops.remove(&id)
            && let Some(mut entity) = self.take_entity(id)
        {
            for op in ops {
                op(self, entity.as_mut());
            }
            self.put_back_entity(id, entity);
        }
    }

    /// Run `f` on entity `id` now if it is in storage (returns its result); if the entity is taken out (it is the
    /// one running, e.g. the attacker whose hit kills the victim) `f` runs when it is put back and `None` is returned.
    pub fn with_entity_or_defer<R: 'static>(&mut self, id: EntityId, f: impl FnOnce(&mut ServerLevel, &mut dyn Entity) -> R + 'static) -> Option<R> {
        match self.take_entity(id) {
            Some(mut entity) => {
                let result = f(self, entity.as_mut());
                self.put_back_entity(id, entity);
                Some(result)
            }
            None => {
                if self.entity_manager.has_entity_with_id(id) {
                    self.deferred_level_ops.entry(id).or_default().push(Box::new(move |level, entity| {
                        f(level, entity);
                    }));
                }
                None
            }
        }
    }

    /// Apply pending removal / section changes of an entity that was mutated in place.
    pub fn sync_entity(&mut self, id: EntityId) {
        self.entity_manager.sync_entity(id);
        self.process_entity_events();
    }

    /// Take `id` out, run `f` with the level (the entity is absent from lookups), put it back.
    pub fn with_entity<R>(&mut self, id: EntityId, f: impl FnOnce(&mut ServerLevel, &mut dyn Entity) -> R) -> Option<R> {
        let mut entity = self.take_entity(id)?;
        let result = f(self, entity.as_mut());
        self.put_back_entity(id, entity);
        Some(result)
    }

    /// `with_entity` for a player: `f` gets the `ServerPlayer` (taken out of the level while it runs).
    pub fn with_player<R>(&mut self, id: EntityId, f: impl FnOnce(&mut ServerLevel, &mut crate::world::entity::ServerPlayer) -> R) -> Option<R> {
        self.with_entity(id, |level, e| e.as_any_mut().downcast_mut::<crate::world::entity::ServerPlayer>().map(|p| f(level, p))).flatten()
    }

    /// `entityTickList.forEach(f)` with vanilla's snapshot semantics: every entity that was in the tick list when
    /// the walk started and still exists is taken out, passed to `f`, and put back.
    pub fn for_each_ticking(&mut self, mut f: impl FnMut(&mut ServerLevel, &mut dyn Entity)) {
        for id in self.entity_manager.tick_list.snapshot() {
            self.with_entity(id, |level, entity| f(level, entity));
        }
    }

    /// `entityManager.tick()`: finish pending loads, then process chunk unloads.
    pub fn tick_entity_management(&mut self) {
        self.process_pending_entity_loads();
        self.entity_manager.process_unloads();
        self.process_entity_events();
    }

    /// `processPendingLoads()`: instantiate the entities of chunks whose entity data arrived and add them.
    pub fn process_pending_entity_loads(&mut self) {
        for chunk in self.entity_manager.take_loaded_chunks() {
            let entities = crate::world::entity::factory::load_entities_recursive(chunk.get_entities(), self, mc_data::enums::EntitySpawnReason::Load);
            self.entity_manager.add_legacy_chunk_entities(entities);
            self.entity_manager.mark_chunk_entities_loaded(chunk.get_pos());
        }
        self.process_entity_events();
    }

    /// `ServerLevel.EntityCallbacks` for the events the manager queued.
    pub fn process_entity_events(&mut self) {
        loop {
            let events = self.entity_manager.take_events();
            if events.is_empty() {
                break;
            }
            for event in events {
                self.handle_entity_event(event);
            }
        }
        self.entity_manager.clear_limbo();
    }

    fn handle_entity_event(&mut self, event: EntityLevelEvent) {
        match event {
            EntityLevelEvent::TrackingStart(id) => {
                if self.entity_manager.get_any(id).is_some_and(|e| e.is_player()) && !self.players.contains(&id) {
                    self.players.push(id);
                    // `ServerChunkCache.addEntity(player)` -> `ChunkMap.updatePlayerStatus(player, true)`
                    let info = self
                        .entity_manager
                        .get_any(id)
                        .and_then(|e| e.as_any().downcast_ref::<crate::world::entity::ServerPlayer>())
                        .and_then(|p| Some((p, p.connection.clone()?, p.listener.as_ref()?.chunk_sender.clone())))
                        .map(|(p, connection, sink)| super::chunk_map::PlayerTrackingInfo {
                            entity_id: id,
                            uuid: p.uuid(),
                            connection,
                            sink: Box::new(sink),
                            position: p.position(),
                            requested_view_distance: p.requested_view_distance,
                            spectator: p.is_spectator(),
                            alive: p.is_alive(),
                        });
                    if let Some(info) = info {
                        self.chunk_source.chunk_map.add_player(info);
                    }
                }
                let players = self.tracker_players();
                self.entity_tracker.add_entity(&mut Entities::new(&mut self.entity_manager), &self.chunk_source.chunk_map, &players, id);
            }
            EntityLevelEvent::TrackingEnd(id) => {
                if self.players.contains(&id) {
                    self.chunk_source.chunk_map.remove_player(id);
                }
                self.players.retain(|&p| p != id);
                let player = self
                    .entity_manager
                    .get_removed(id)
                    .or_else(|| self.entity_manager.get_any(id))
                    .and_then(|e| e.as_any().downcast_ref::<crate::world::entity::ServerPlayer>())
                    .map(super::TrackerPlayer::of);
                self.entity_tracker.remove_entity(&mut Entities::new(&mut self.entity_manager), id, player.as_ref());
            }
            // tick list is maintained by the manager; the other callbacks have no state of their own yet
            EntityLevelEvent::Created(_)
            | EntityLevelEvent::Destroyed(_)
            | EntityLevelEvent::TickingStart(_)
            | EntityLevelEvent::TickingEnd(_)
            | EntityLevelEvent::SectionChange(_) => {}
        }
    }

    /// `level.players()` as the tracker reads them: live players from storage, players that are currently taken out
    /// (their packet or tick is being handled) from the snapshot taken when they were taken out.
    pub fn tracker_players(&self) -> Vec<super::TrackerPlayer> {
        self.players
            .iter()
            .filter_map(|&id| {
                match self.entity_manager.get_any(id).and_then(|e| e.as_any().downcast_ref::<crate::world::entity::ServerPlayer>()) {
                    Some(p) => Some(super::TrackerPlayer::of(p)),
                    None => self.taken_out_players.get(&id).cloned(),
                }
            })
            .collect()
    }

    /// `ChunkMap.tick()`'s entity half (`updateTrackedEntities` + `sendChanges`).
    pub fn tick_entity_tracker(&mut self) {
        let players = self.tracker_players();
        self.entity_tracker.tick(&mut Entities::new(&mut self.entity_manager), &self.chunk_source.chunk_map, &players);
    }

    /// `ChunkMap.move(player)` entity half for a player that is currently taken out of the level (its packet or tick
    /// is being handled): the player is passed explicitly. Call it BEFORE the chunk map's own `move_player` (vanilla
    /// runs the tracker loop first, with the previous chunk tracking view).
    pub fn entity_tracker_move_taken_out_player(&mut self, player: &mut crate::world::entity::ServerPlayer) {
        let mut players = self.tracker_players();
        let snapshot = TrackerPlayer::of(player);
        match players.iter_mut().find(|p| p.id == snapshot.id) {
            Some(p) => *p = snapshot.clone(),
            None => players.push(snapshot.clone()),
        }
        self.taken_out_players.insert(snapshot.id, snapshot.clone());
        let mut entities = Entities::with_extra(&mut self.entity_manager, player);
        self.entity_tracker.move_player(&mut entities, &self.chunk_source.chunk_map, &players, &snapshot);
    }

    // ---- queries ----------------------------------------------------------------------------------------------

    /// `getEntities(except, bb, selector)`: ids in vanilla iteration order (sections by key, insertion order inside).
    pub fn get_entities(&self, except: Option<EntityId>, bb: &AABB, mut selector: impl FnMut(&dyn Entity) -> bool) -> Vec<EntityId> {
        let mut out = Vec::new();
        self.entity_manager.for_each_in_aabb(bb, |id, e| {
            if Some(id) != except && selector(e) {
                out.push(id);
            }
            ControlFlow::Continue(())
        });
        out
    }

    /// `getEntities(EntityTypeTest, bb, selector)` / `getEntitiesOfClass`: `test` is the type test (`instanceof`).
    pub fn get_entities_of_class(&self, bb: &AABB, test: impl Fn(&dyn Entity) -> bool, mut selector: impl FnMut(&dyn Entity) -> bool) -> Vec<EntityId> {
        let mut out = Vec::new();
        self.entity_manager.for_each_in_aabb(bb, |id, e| {
            if test(e) && selector(e) {
                out.push(id);
            }
            ControlFlow::Continue(())
        });
        out
    }

    /// `getNearestPlayer(x, y, z, range, predicate)`.
    pub fn get_nearest_player(&self, x: f64, y: f64, z: f64, range: f64, predicate: impl Fn(&dyn Entity) -> bool) -> Option<EntityId> {
        let mut best = -1.0;
        let mut result = None;
        for &id in &self.players {
            let Some(player) = self.entity_manager.get_any(id) else { continue };
            if predicate(player) {
                let dist = player.base().distance_to_sqr_xyz(x, y, z);
                if (range < 0.0 || dist < range * range) && (best == -1.0 || dist < best) {
                    best = dist;
                    result = Some(id);
                }
            }
        }
        result
    }

    /// `getPlayerByUUID(uuid)`.
    pub fn get_player_by_uuid(&self, uuid: Uuid) -> Option<EntityId> {
        self.players.iter().copied().find(|&id| self.entity_manager.get_any(id).is_some_and(|p| p.uuid() == uuid))
    }
}


// ---- collision / movement integration ----------------------------------------------------------------------------

impl crate::world::level::collision::CollisionGetter for ServerLevel {
    fn chunk_available_for_collisions(&self, chunk_x: i32, chunk_z: i32) -> bool {
        self.chunk_now(chunk_x, chunk_z).is_some()
    }

    /// `borderCollision(source, box)`.
    fn border_collision(&self, source: EntityId, aabb: &AABB) -> Option<mc_core::phys::shapes::VoxelShape> {
        let entity = self.entity_manager.get_any(source)?;
        let border = self.world_border_ref()?;
        border.is_inside_close_to_border(entity.base().x(), entity.base().z(), aabb).then(|| border.get_collision_shape())
    }

    /// `Level.isUnobstructed(source, shape)`: no entity that `blocksBuilding` intersects `shape`.
    fn is_unobstructed(&self, source: Option<EntityId>, shape: &mc_core::phys::shapes::VoxelShape) -> bool {
        use mc_core::phys::shapes::{BooleanOp, Shapes};
        if shape.is_empty() {
            return true;
        }
        let source = source.and_then(|id| self.entity_manager.get_any(id));
        self.get_entities(None, &shape.bounds(), |_| true).into_iter().filter_map(|id| self.entity_manager.get_any(id)).all(|e| {
            e.is_removed()
                || !e.base().blocks_building
                || source.is_some_and(|s| self.is_passenger_of_same_vehicle(s, e))
                || !Shapes::join_is_not_empty(shape, &Shapes::create(e.get_bounding_box()), BooleanOp::AND)
        })
    }

    /// Id-based form (no live source): every entity that can be collided with.
    fn get_entity_collisions(&self, _source: Option<EntityId>, test_area: &AABB) -> Vec<mc_core::phys::shapes::VoxelShape> {
        crate::world::entity::movement::EntityCollisionQuery::entity_collisions(self, None, test_area)
    }
}

impl crate::world::entity::movement::EntityCollisionQuery for ServerLevel {
    /// `EntityGetter.getEntityCollisions(source, testArea)`.
    fn entity_collisions(&self, source: Option<&dyn Entity>, test_area: &AABB) -> Vec<mc_core::phys::shapes::VoxelShape> {
        if test_area.get_size() < 1.0E-7 {
            return Vec::new();
        }
        let area = test_area.inflate_scalar(1.0E-7);
        let ids = self.get_entities(source.map(|s| s.id()), &area, |e| {
            !e.is_spectator()
                && match source {
                    None => e.can_be_collided_with(None),
                    Some(s) => e.can_be_collided_with(Some(s)) && !self.is_passenger_of_same_vehicle(s, e),
                }
        });
        ids.into_iter().filter_map(|id| self.entity_manager.get_any(id)).map(|e| mc_core::phys::shapes::Shapes::create(e.get_bounding_box())).collect()
    }
}

impl crate::world::entity::movement::MovementLevel for ServerLevel {
    fn game_time(&self) -> i64 {
        ServerLevel::game_time(self)
    }

    fn world_border_collision(&self, source: &dyn Entity, aabb: &AABB) -> Option<mc_core::phys::shapes::VoxelShape> {
        let border = self.world_border_ref()?;
        border.is_inside_close_to_border(source.base().x(), source.base().z(), aabb).then(|| border.get_collision_shape())
    }

    fn has_chunks_at(&self, x0: i32, z0: i32, x1: i32, z1: i32) -> bool {
        let (cx0, cx1, cz0, cz1) = (x0 >> 4, x1 >> 4, z0 >> 4, z1 >> 4);
        (cx0..=cx1).all(|cx| (cz0..=cz1).all(|cz| crate::world::level::LevelReader::has_chunk(self, cx, cz)))
    }
}

impl ServerLevel {
    /// `Entity.isPassengerOfSameVehicle(other)`: same root vehicle.
    pub fn is_passenger_of_same_vehicle(&self, a: &dyn Entity, b: &dyn Entity) -> bool {
        let root = |e: &dyn Entity| {
            let mut id = e.id();
            let mut cur = e.base().vehicle;
            while let Some(v) = cur {
                id = v;
                cur = self.entity_manager.get_any(v).and_then(|x| x.base().vehicle);
            }
            id
        };
        root(a) == root(b)
    }
}

impl ServerLevel {
    /// `MinecraftServer.isUnderSpawnProtection(level, pos, player)`.
    pub fn is_under_spawn_protection(&self, player: &crate::world::entity::ServerPlayer, pos: mc_core::pos::BlockPos) -> bool {
        let _ = (player, pos); false
    }
}
