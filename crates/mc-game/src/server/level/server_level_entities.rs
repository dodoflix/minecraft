//! The entity half of `ServerLevel`: id allocation, adding, lookups, queries, the take-out loop and the
//! `ServerLevel.EntityCallbacks` (applied from the manager's event queue).

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

impl ServerLevel {
    /// `getNextEntityId()`.
    pub fn next_entity_id(&self) -> EntityId {
        let mut id = 0;
        while id == 0 || self.entity_manager.has_entity_with_id(id) {
            id = ENTITY_COUNTER.fetch_add(1, Ordering::Relaxed).wrapping_add(1);
        }
        id
    }

    /// `EntityType.canSpawn(level)`: enabled features and peaceful difficulty. The level data these read
    /// (`enabledFeatures`, difficulty) does not exist on `ServerLevel` yet, so every type may spawn for now.
    pub fn can_spawn_entity_type(&self, _ty: EntityType) -> bool {
        true
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
        self.entity_manager.take(id)
    }

    /// Put a taken-out entity back and apply removal / section moves (`EntityInLevelCallback`).
    pub fn put_back_entity(&mut self, id: EntityId, entity: Box<dyn Entity>) {
        self.entity_manager.put_back(id, entity);
        self.process_entity_events();
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
                }
            }
            EntityLevelEvent::TrackingEnd(id) => {
                self.players.retain(|&p| p != id);
            }
            // tick list is maintained by the manager; the other callbacks have no state of their own yet
            EntityLevelEvent::Created(_)
            | EntityLevelEvent::Destroyed(_)
            | EntityLevelEvent::TickingStart(_)
            | EntityLevelEvent::TickingEnd(_)
            | EntityLevelEvent::SectionChange(_) => {}
        }
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

