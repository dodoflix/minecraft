//! The entity half of `ServerLevel`: id allocation, adding, lookups, queries, the take-out loop and the
//! `ServerLevel.EntityCallbacks` (applied from the manager's event queue).

use super::PlayerRecord;
use super::ServerLevel;
use super::entity_tracker::Entities;
use crate::world::entity::{Entity, EntityId, RemovalReason};
use crate::world::level::entity::{EntityLevelEvent, PersistentEntitySectionManager};
use crate::world::level::gameevent::DynamicListenerAction;
use mc_core::phys::AABB;
use mc_data::EntityType;
use std::ops::ControlFlow;
use std::sync::atomic::{AtomicI32, Ordering};
use uuid::Uuid;

/// A removed entity is always readable for this many ticks (a sensor scans every 20 ticks, and goals or behaviours that
/// hold the object are not looked at); after that it stays only while a brain memory still names it (see
/// `ServerLevel::expire_recently_removed`).
const RECENTLY_REMOVED_RETENTION_TICKS: i64 = 20;

/// `ServerLevel.ENTITY_COUNTER` (static: shared by all levels).
static ENTITY_COUNTER: AtomicI32 = AtomicI32::new(0);

/// `ServerLevel.ENTITY_COUNTER.incrementAndGet()` for entities created off the game thread (worldgen): no level to
/// check for taken ids (they are saved as NBT and get a fresh id when loaded).
pub fn next_detached_entity_id() -> EntityId {
    ENTITY_COUNTER
        .fetch_add(1, Ordering::Relaxed)
        .wrapping_add(1)
}

impl ServerLevel {
    /// A private `ENTITY_COUNTER` starting after `start`, shared with chunk generation (the entities it creates take
    /// ids from the same sequence).
    pub fn use_private_entity_counter(&mut self, start: i32) {
        let counter = std::sync::Arc::new(AtomicI32::new(start));
        self.spawning.gen_live.share_entity_counter(counter.clone());
        self.entity_id_counter = Some(counter);
    }

    /// `getNextEntityId()`.
    pub fn next_entity_id(&self) -> EntityId {
        let counter = self.entity_id_counter.as_deref().unwrap_or(&ENTITY_COUNTER);
        let mut id = 0;
        while id == 0 || self.entity_manager.has_entity_with_id(id) {
            id = counter.fetch_add(1, Ordering::Relaxed).wrapping_add(1);
        }
        id
    }

    /// `EntityType.canSpawn(level)`: the type's required features are enabled and (unless allowed in peaceful) the
    /// difficulty is not peaceful.
    pub fn can_spawn_entity_type(&self, ty: EntityType) -> bool {
        let data = ty.data();
        let required: Vec<mc_core::ident::Identifier> = data
            .required_features
            .iter()
            .filter_map(|n| mc_core::ident::Identifier::try_parse(n))
            .collect();
        if !mc_world::level_storage::FeatureFlagSet::from_names(required.iter(), |_| {})
            .is_subset_of(self.tick_state.enabled_features)
        {
            return false;
        }
        data.allowed_in_peaceful
            || self.difficulty() != mc_world::level_storage::Difficulty::Peaceful
    }

    // ---- adding ---------------------------------------------------------------------------------------------

    /// `addFreshEntity(entity)` (= `addEntity`): false if the entity is already marked removed or its UUID exists.
    pub fn add_fresh_entity(&mut self, entity: Box<dyn Entity>) -> bool {
        if entity.is_removed() {
            log::warn!(
                "Tried to add entity {} but it was marked as removed already",
                entity.get_type().data().name
            );
            return false;
        }
        let mut entity = entity;
        entity.read_level_dependent_data(self);
        // `addFreshEntityWithPassengers`: passengers created by `finalizeSpawn` (spider jockey) follow their vehicle
        let passengers = entity
            .mob_mut()
            .map(|m| std::mem::take(&mut m.pending_passengers))
            .unwrap_or_default();
        let ok = self.entity_manager.add_new_entity(entity).is_ok();
        self.process_entity_events();
        if ok {
            for passenger in passengers {
                self.add_fresh_entity(passenger);
            }
        }
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
        if entities
            .iter()
            .any(|e| self.entity_manager.is_loaded(e.uuid()))
        {
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
        self.dead_players.remove(&id);
        self.with_entity(id, |_, e| e.remove(reason));
    }

    // ---- lookups ----------------------------------------------------------------------------------------------

    /// `getEntity(id)`: a tracked entity that is not currently taken out, or the entity that is loading.
    pub fn get_entity(&self, id: EntityId) -> Option<&dyn Entity> {
        self.entity_manager
            .get(id)
            .or_else(|| self.loading_entity.as_deref().filter(|e| e.id() == id))
    }

    /// [`ServerLevel::get_entity`], or the entity its running code lent although it removed itself meanwhile.
    pub fn get_entity_or_lent(&self, id: EntityId) -> Option<&dyn Entity> {
        self.get_entity(id)
            .or_else(|| self.entity_manager.lent_including_removed(id))
    }

    /// [`ServerLevel::get_entity`], or a removed non-player entity that a brain memory may still hold (removed at most
    /// [`RECENTLY_REMOVED_RETENTION_TICKS`] ticks ago, or still named by a memory): vanilla's memories keep the removed
    /// object, so a behavior that does not check `isRemoved` still reads its position until the memory is rewritten.
    pub fn get_entity_or_recently_removed(&self, id: EntityId) -> Option<&dyn Entity> {
        self.get_entity(id).or_else(|| {
            self.recently_removed
                .iter()
                .find(|(_, removed, _)| *removed == id)
                .map(|(_, _, e)| e.as_ref())
        })
    }

    /// [`ServerLevel::get_entity`], a killed player that left the level, or the entity that was removed when
    /// [`ServerLevel::removed_entities`] keeps them.
    pub fn get_entity_or_removed(&self, id: EntityId) -> Option<&dyn Entity> {
        self.get_entity_or_recently_removed(id)
            .or_else(|| self.get_dead_player(id))
            .or_else(|| {
                self.removed_entities
                    .as_ref()
                    .and_then(|kept| kept.get(&id))
                    .map(|e| e.as_ref())
            })
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

    /// `getRandomPlayer()`: one of the living players, drawn with the level random.
    pub fn get_random_player(&mut self) -> Option<EntityId> {
        let alive: Vec<EntityId> = self
            .players
            .iter()
            .copied()
            .filter(|id| self.get_entity(*id).is_some_and(|p| p.is_alive()))
            .collect();
        if alive.is_empty() {
            return None;
        }
        let index = mc_core::random::RandomSource::next_int_bounded(
            &mut self.tick_state.random,
            alive.len() as i32,
        );
        Some(alive[index as usize])
    }

    // ---- take-out / sync --------------------------------------------------------------------------------------

    /// Take an entity out of its slot (lookups return `None` while it is out). Pair with [`ServerLevel::put_back_entity`].
    pub fn take_entity(&mut self, id: EntityId) -> Option<Box<dyn Entity>> {
        let entity = self
            .entity_manager
            .take(id)
            .or_else(|| self.dead_players.remove(&id))?;
        if let Some(p) = entity
            .as_any()
            .downcast_ref::<crate::world::entity::ServerPlayer>()
        {
            self.player_records.insert(id, super::PlayerRecord::of(p));
        }
        Some(entity)
    }

    /// Put a taken-out entity back and apply removal / section moves (`EntityInLevelCallback`).
    pub fn put_back_entity(&mut self, id: EntityId, entity: Box<dyn Entity>) {
        self.player_records.remove(&id);
        self.entity_manager.put_back(id, entity);
        self.process_entity_events();
        self.process_waypoint_state(id);
    }

    /// The tracker part of `Entity.setRemoved` for an entity that is out of storage (it is being ticked or lent):
    /// vanilla runs `ChunkMap.removeEntity` at once, so watchers get `remove_entities` before anything the rest of
    /// the tick sends. [`ServerLevel::remove_held_entity`] calls it; the callbacks that need no packet order follow
    /// when the entity is put back.
    pub(super) fn untrack_discarded_entity(&mut self, entity: &mut dyn Entity) {
        debug_assert!(entity.is_removed() && !entity.is_player());
        let id = entity.id();
        self.lend(entity, |level| {
            level.entity_tracker.remove_entity(
                &mut Entities::new(&mut level.entity_manager),
                id,
                None,
            );
        });
    }

    /// Apply pending removal / section changes of an entity that was mutated in place.
    pub fn sync_entity(&mut self, id: EntityId) {
        self.entity_manager.sync_entity(id);
        self.process_entity_events();
        self.process_waypoint_state(id);
    }

    /// Take `id` out, run `f` with the level (the entity is absent from lookups), put it back. An entity that is
    /// out because its owner lent it (see [`ServerLevel::lend`]) is handed out as the lent `&mut` instead; its
    /// tracking / listener callbacks still wait for the owner's [`ServerLevel::put_back_entity`]. `None` if the
    /// entity is gone; an entity that is out of the level and not lent also yields `None`, which is a bug at the
    /// caller (it needs a `lend`), so that asserts in debug builds and logs a warning otherwise.
    pub fn with_entity<R>(
        &mut self,
        id: EntityId,
        f: impl FnOnce(&mut ServerLevel, &mut dyn Entity) -> R,
    ) -> Option<R> {
        let result = self.try_with_entity(id, f);
        if result.is_none() {
            self.note_unlent_target(id);
        }
        result
    }

    /// [`ServerLevel::with_entity`] for a caller that handles an entity it cannot reach (out and not lent) itself.
    pub fn try_with_entity<R>(
        &mut self,
        id: EntityId,
        f: impl FnOnce(&mut ServerLevel, &mut dyn Entity) -> R,
    ) -> Option<R> {
        let Some(mut entity) = self.take_entity(id) else {
            return PersistentEntitySectionManager::with_lent_mut(
                self,
                |level| &mut level.entity_manager,
                id,
                f,
            );
        };
        let result = f(self, entity.as_mut());
        self.put_back_entity(id, entity);
        Some(result)
    }

    /// Run `f` while `me`, the entity whose code is running (taken out of its slot), is visible to the level: by id,
    /// UUID, box, class and player queries, and `with_entity(me.id())` hands its `&mut` back, so effects on `me` run
    /// where vanilla runs them. Wrap exactly the call-out that looks `me` up (a hit, an explosion, a trigger, a block
    /// reading entities inside it); `me` cannot be used inside `f`.
    pub fn lend<R>(&mut self, me: &mut dyn Entity, f: impl FnOnce(&mut ServerLevel) -> R) -> R {
        PersistentEntitySectionManager::lend_in(self, |level| &mut level.entity_manager, me, f)
    }

    /// [`ServerLevel::lend`] for a read-only call-out (queries, predicates, game events): `me` is visible but
    /// `with_entity(me.id())` does not re-enter.
    pub fn expose<R>(&mut self, me: &dyn Entity, f: impl FnOnce(&mut ServerLevel) -> R) -> R {
        PersistentEntitySectionManager::expose_in(self, |level| &mut level.entity_manager, me, f)
    }

    /// Debug detector for call-outs that miss a [`ServerLevel::lend`]: a hit whose causing or direct entity exists in
    /// the level but is out of its slot and not lent cannot be found by anything `victim`'s code looks up. Builds
    /// without debug assertions skip it. When `MC_LEND_DETECTOR` names a file, each distinct attacker / victim / call
    /// path is appended once per process as `attacker type <- victim type | frames`.
    pub fn note_unlent_attacker(
        &self,
        source: &crate::world::damagesource::DamageSource,
        victim: &dyn Entity,
    ) {
        #[cfg(debug_assertions)]
        for (id, info) in [
            (source.get_entity(), source.causing_info()),
            (source.get_direct_entity(), source.direct_info()),
        ] {
            let Some(id) = id.filter(|&id| id != victim.id()) else {
                continue;
            };
            if self.entity_manager.has_entity_with_id(id)
                && self.entity_manager.get_any(id).is_none()
                && !self.entity_manager.is_lent(id)
            {
                let attacker = info.map_or("?", |i| i.entity_type.data().name);
                report_unlent_attacker(attacker, victim.get_type().data().name);
            }
        }
        #[cfg(not(debug_assertions))]
        let _ = (source, victim);
    }

    /// Debug detector like [`ServerLevel::note_unlent_attacker`] for a game event whose source entity exists but is
    /// out of its slot and not exposed: listeners cannot read it. Appended as `event <- game event | frames`.
    pub(super) fn note_unlent_event_source(
        &self,
        event: mc_data::GameEvent,
        context: &crate::world::level::gameevent::GameEventContext,
    ) {
        #[cfg(debug_assertions)]
        if let Some(id) = context
            .source_entity
            .filter(|_| context.source_info.is_none())
            && self.entity_manager.has_entity_with_id(id)
            && self.entity_manager.get_any(id).is_none()
            && !self.entity_manager.is_lent(id)
        {
            report_unlent_attacker(&event.identifier().to_string(), "game event");
        }
        #[cfg(not(debug_assertions))]
        let _ = (event, context);
    }

    /// A [`ServerLevel::with_entity`] that found nothing although `id` is still in the level: it is out of its slot
    /// and its owner did not [`ServerLevel::lend`] it, so the award / trigger / effect aimed at it is lost (vanilla
    /// finds it). A removed entity (slot gone) is fine: vanilla does nothing then either.
    fn note_unlent_target(&self, id: EntityId) {
        if self.entity_manager.has_entity_with_id(id) && !self.entity_manager.is_lent(id) {
            let msg = format!(
                "entity {id} is taken out of the level and not lent; the action aimed at it is lost"
            );
            log::warn!("{msg}");
            debug_assert!(false, "{msg}");
        }
    }

    /// `with_entity` for a player: `f` gets the `ServerPlayer` (taken out of the level while it runs).
    pub fn with_player<R>(
        &mut self,
        id: EntityId,
        f: impl FnOnce(&mut ServerLevel, &mut crate::world::entity::ServerPlayer) -> R,
    ) -> Option<R> {
        self.with_entity(id, |level, e| {
            e.as_any_mut()
                .downcast_mut::<crate::world::entity::ServerPlayer>()
                .map(|p| f(level, p))
        })
        .flatten()
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
        self.expire_recently_removed();
    }

    /// Every [`RECENTLY_REMOVED_RETENTION_TICKS`] ticks, forgets the removed entities that are older than that and that no
    /// brain memory holds any more. Vanilla's memories keep the removed object for as long as they live (`AVOID_TARGET`
    /// up to 400 ticks, `ATTACK_TARGET` of a piglin whose fight behaviours are paused by admiring), so the entity stays
    /// readable while a memory still names it.
    fn expire_recently_removed(&mut self) {
        let now = self.game_time();
        if now % RECENTLY_REMOVED_RETENTION_TICKS != 0
            || self
                .recently_removed
                .iter()
                .all(|(removed_at, ..)| now - removed_at <= RECENTLY_REMOVED_RETENTION_TICKS)
        {
            return;
        }
        let mut held = std::collections::HashSet::new();
        for entity in self.entity_manager.get_all() {
            if let Some(living) = entity.living() {
                living.brain.for_each(|_, value| {
                    if let Some((value, _)) = value {
                        value.collect_entities(&mut held);
                    }
                });
            }
        }
        let (live, expired): (Vec<_>, Vec<_>) = std::mem::take(&mut self.recently_removed)
            .into_iter()
            .partition(|(removed_at, id, _)| {
                now - removed_at <= RECENTLY_REMOVED_RETENTION_TICKS || held.contains(id)
            });
        self.recently_removed = live;
        if let Some(kept) = self.removed_entities.as_mut() {
            kept.extend(expired.into_iter().map(|(_, id, e)| (id, e)));
        }
    }

    /// `processPendingLoads()`: instantiate the entities of chunks whose entity data arrived and add them.
    pub fn process_pending_entity_loads(&mut self) {
        for chunk in self.entity_manager.take_loaded_chunks() {
            let mut entities = crate::world::entity::factory::load_entities_recursive(
                chunk.get_entities(),
                self,
                mc_data::enums::EntitySpawnReason::Load,
            );
            crate::world::entity::factory::read_level_dependent_data(&mut entities, self);
            let loaded_ids: Vec<EntityId> = entities
                .iter()
                .filter(|e| e.raider().is_some())
                .map(|e| e.id())
                .collect();
            self.entity_manager.add_legacy_chunk_entities(entities);
            // `Raider.readAdditionalSaveData` joins the saved raid
            for id in loaded_ids {
                self.with_entity(id, |level, e| e.after_loaded_to_level(level));
            }
            self.entity_manager
                .mark_chunk_entities_loaded(chunk.get_pos());
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
                let waypoint_event = match &event {
                    EntityLevelEvent::TrackingStart(id) => Some((*id, true)),
                    EntityLevelEvent::TrackingEnd(id) | EntityLevelEvent::Destroyed(id) => {
                        Some((*id, false))
                    }
                    _ => None,
                };
                self.handle_entity_event(event);
                match waypoint_event {
                    Some((id, true)) => self.waypoint_tracking_start(id),
                    Some((id, false)) => self.waypoint_tracking_end(id),
                    None => {}
                }
            }
        }
        // a killed player's entity leaves the level but the player lives on (player list, connection, respawn)
        let dead: Vec<EntityId> = self
            .entity_manager
            .removed_ids()
            .into_iter()
            .filter(|&id| {
                self.entity_manager.get_removed(id).is_some_and(|e| {
                    e.is_player()
                        && e.base().removal_reason() == Some(RemovalReason::Killed)
                        && e.as_any()
                            .downcast_ref::<crate::world::entity::ServerPlayer>()
                            .is_some_and(|p| p.connection.is_some())
                })
            })
            .collect();
        for id in dead {
            if let Some(player) = self.entity_manager.take_removed(id) {
                self.dead_players.insert(id, player);
            }
        }
        let now = self.game_time();
        let limbo = self.entity_manager.drain_limbo();
        let (players, removed): (Vec<_>, Vec<_>) =
            limbo.into_iter().partition(|(_, e)| e.is_player());
        if let Some(kept) = self.removed_entities.as_mut() {
            kept.extend(players);
        }
        self.recently_removed
            .extend(removed.into_iter().map(|(id, e)| (now, id, e)));
    }

    /// A killed player that is no longer in the level (see [`ServerLevel::dead_players`]).
    pub fn get_dead_player(&self, id: EntityId) -> Option<&dyn Entity> {
        self.dead_players.get(&id).map(|e| e.as_ref())
    }

    fn handle_entity_event(&mut self, event: EntityLevelEvent) {
        match event {
            EntityLevelEvent::TrackingStart(id) => {
                if self
                    .entity_manager
                    .get_any(id)
                    .is_some_and(|e| e.mob().is_some())
                {
                    self.navigating_mobs.insert(id);
                }
                if self
                    .entity_manager
                    .get_any(id)
                    .is_some_and(|e| e.is_player())
                    && !self.players.contains(&id)
                {
                    self.players.push(id);
                    // `ServerChunkCache.addEntity(player)` -> `ChunkMap.updatePlayerStatus(player, true)`
                    let info = self
                        .entity_manager
                        .get_any(id)
                        .and_then(|e| {
                            e.as_any()
                                .downcast_ref::<crate::world::entity::ServerPlayer>()
                        })
                        .and_then(|p| {
                            Some((
                                p,
                                p.connection.clone()?,
                                p.listener.as_ref()?.chunk_sender.clone(),
                            ))
                        })
                        .map(
                            |(p, connection, sink)| super::chunk_map::PlayerTrackingInfo {
                                entity_id: id,
                                uuid: p.uuid(),
                                connection,
                                sink: Box::new(sink),
                                position: p.position(),
                                requested_view_distance: p.requested_view_distance,
                                spectator: p.is_spectator(),
                                alive: p.is_alive(),
                            },
                        );
                    if let Some(info) = info {
                        self.chunk_source.chunk_map.add_player(info);
                    }
                    self.update_sleeping_player_list();
                }
                let players = self.player_records();
                self.entity_tracker.add_entity(
                    &mut Entities::new(&mut self.entity_manager),
                    &self.chunk_source.chunk_map,
                    &players,
                    id,
                );
                self.update_dynamic_game_event_listener(id, DynamicListenerAction::Add);
            }
            EntityLevelEvent::TrackingEnd(id) => {
                self.navigating_mobs.remove(&id);
                if self.players.contains(&id) {
                    self.chunk_source.chunk_map.remove_player(id);
                }
                let was_player = self.players.contains(&id);
                self.players.retain(|&p| p != id);
                if was_player {
                    self.update_sleeping_player_list();
                }
                let player = self
                    .entity_manager
                    .get_removed(id)
                    .or_else(|| self.entity_manager.get_any(id))
                    .and_then(|e| {
                        e.as_any()
                            .downcast_ref::<crate::world::entity::ServerPlayer>()
                    })
                    .map(PlayerRecord::of);
                self.entity_tracker.remove_entity(
                    &mut Entities::new(&mut self.entity_manager),
                    id,
                    player.as_ref(),
                );
                self.update_dynamic_game_event_listener(id, DynamicListenerAction::Remove);
            }
            EntityLevelEvent::SectionChange(id) => {
                self.update_dynamic_game_event_listener(id, DynamicListenerAction::Move)
            }
            // tick list is maintained by the manager; the other callbacks have no state of their own yet
            EntityLevelEvent::Created(_)
            | EntityLevelEvent::Destroyed(_)
            | EntityLevelEvent::TickingStart(_)
            | EntityLevelEvent::TickingEnd(_) => {}
        }
    }

    /// `entity.updateDynamicGameEventListener(action)` (the entity is taken out of its slot, or out of the removal limbo,
    /// while it gets the level).
    fn update_dynamic_game_event_listener(&mut self, id: EntityId, action: DynamicListenerAction) {
        let Some(mut entity) = self
            .entity_manager
            .take(id)
            .or_else(|| self.entity_manager.take_removed(id))
        else {
            return;
        };
        entity.update_dynamic_game_event_listener(self, action);
        // a removed entity's slot is gone: `put_back` parks it in the limbo again
        self.entity_manager.put_back(id, entity);
    }
    /// `getEntity(uuid).getId()` that also finds an entity that is currently taken out of its slot (it is ticking).
    pub fn entity_id_by_uuid(&self, uuid: Uuid) -> Option<EntityId> {
        self.entity_manager.id_by_uuid(uuid)
    }

    /// `entity.getUUID()` of a tracked entity that is in its slot.
    pub fn entity_uuid_by_id(&self, id: EntityId) -> Option<Uuid> {
        self.entity_manager.get_any(id).map(|e| e.uuid())
    }

    /// The record of player `id`: read live while the player is in storage or lent, else the last record taken when its
    /// code started running.
    pub fn player_record(&self, id: EntityId) -> Option<PlayerRecord> {
        match self.entity_manager.get_any(id).and_then(|e| {
            e.as_any()
                .downcast_ref::<crate::world::entity::ServerPlayer>()
        }) {
            Some(p) => Some(PlayerRecord::of(p)),
            None => self.player_records.get(&id).cloned(),
        }
    }

    /// `level.players()` as records, in player-list order (what the tracker and the packet broadcasts walk).
    pub fn player_records(&self) -> Vec<PlayerRecord> {
        self.players
            .iter()
            .filter_map(|&id| self.player_record(id))
            .collect()
    }

    /// `ChunkMap.tick()`'s entity half (`updateTrackedEntities` + `sendChanges`).
    pub fn tick_entity_tracker(&mut self) {
        let players = self.player_records();
        let shared = self.shared.clone();
        let maps = super::FrameMapEnv {
            map_storage: &shared.map_storage,
            dimension: &self.dimension,
            game_time: self.game_time(),
            tick_count: shared.tick_count(),
        };
        self.entity_tracker.tick(
            &mut Entities::new(&mut self.entity_manager),
            &self.chunk_source.chunk_map,
            &players,
            &maps,
        );
    }

    /// `ChunkMap.move(player)` entity half: runs before the chunk map's own `move_player` (vanilla runs the tracker loop
    /// first, with the previous chunk tracking view). `player` is out of the level storage while its code runs, so it is
    /// lent: its own tracked entity pairs with the other players through the lent `&mut`.
    pub fn entity_tracker_move_player(&mut self, player: &mut crate::world::entity::ServerPlayer) {
        let record = PlayerRecord::of(player);
        self.player_records.insert(record.id, record.clone());
        let id = record.id;
        self.lend(player, |level| {
            let mut players = level.player_records();
            match players.iter_mut().find(|p| p.id == id) {
                Some(p) => *p = record.clone(),
                None => players.push(record.clone()),
            }
            level.entity_tracker.move_player(
                &mut Entities::new(&mut level.entity_manager),
                &level.chunk_source.chunk_map,
                &players,
                &record,
            );
        });
    }

    // ---- queries ----------------------------------------------------------------------------------------------

    /// `getEntities(except, bb, selector)`: ids in vanilla iteration order (sections by key, insertion order inside).
    pub fn get_entities(
        &self,
        except: Option<EntityId>,
        bb: &AABB,
        mut selector: impl FnMut(&dyn Entity) -> bool,
    ) -> Vec<EntityId> {
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
    pub fn get_entities_of_class(
        &self,
        bb: &AABB,
        test: impl Fn(&dyn Entity) -> bool,
        mut selector: impl FnMut(&dyn Entity) -> bool,
    ) -> Vec<EntityId> {
        let mut out = Vec::new();
        self.entity_manager.for_each_in_aabb(bb, |id, e| {
            if test(e) && selector(e) {
                out.push(id);
            }
            ControlFlow::Continue(())
        });
        out
    }
    /// `getEntitiesOfClass(ServerPlayer.class, bb)` (spectators excluded) in vanilla's iteration order. A player whose code
    /// is running (out of the storage) is included through its record: vanilla's entity storage still holds it.
    pub fn get_players_in_aabb(&self, bb: &AABB) -> Vec<EntityId> {
        let mut out = Vec::new();
        self.entity_manager.for_each_id_in_aabb(bb, |id, entity| {
            let included = match entity {
                Some(e) => {
                    e.as_any().is::<crate::world::entity::ServerPlayer>() && !e.is_spectator()
                }
                None => self
                    .player_records
                    .get(&id)
                    .is_some_and(|p| !p.spectator && bb.intersects_aabb(&p.bounding_box)),
            };
            if included {
                out.push(id);
            }
            ControlFlow::Continue(())
        });
        out
    }

    /// `getNearestPlayer(x, y, z, range, predicate)`.
    pub fn get_nearest_player(
        &self,
        x: f64,
        y: f64,
        z: f64,
        range: f64,
        predicate: impl Fn(&dyn Entity) -> bool,
    ) -> Option<EntityId> {
        let mut best = -1.0;
        let mut result = None;
        for &id in &self.players {
            let Some(player) = self.entity_manager.get_any(id) else {
                continue;
            };
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
        self.players
            .iter()
            .copied()
            .find(|&id| self.player_record(id).is_some_and(|p| p.uuid == uuid))
    }
}

// ---- collision / movement integration ----------------------------------------------------------------------------

impl crate::world::level::collision::CollisionGetter for ServerLevel {
    fn chunk_available_for_collisions(&self, chunk_x: i32, chunk_z: i32) -> bool {
        self.chunk_now(chunk_x, chunk_z).is_some()
    }

    /// `borderCollision(source, box)`.
    fn border_collision(
        &self,
        source: EntityId,
        aabb: &AABB,
    ) -> Option<mc_core::phys::shapes::VoxelShape> {
        let entity = self.entity_manager.get_any(source)?;
        let border = self.world_border_ref()?;
        border
            .is_inside_close_to_border(entity.base().x(), entity.base().z(), aabb)
            .then(|| border.get_collision_shape())
    }

    /// `Level.isUnobstructed(source, shape)`: no entity that `blocksBuilding` intersects `shape`.
    fn is_unobstructed(
        &self,
        source: Option<EntityId>,
        shape: &mc_core::phys::shapes::VoxelShape,
    ) -> bool {
        use mc_core::phys::shapes::{BooleanOp, Shapes};
        if shape.is_empty() {
            return true;
        }
        let bounds = shape.bounds();
        let source_root = source.map(|id| self.root_vehicle(id));
        let blocks = |id: EntityId, removed: bool, blocks_building: bool, aabb: &AABB| {
            !removed
                && blocks_building
                && source_root.is_none_or(|root| self.root_vehicle(id) != root)
                && Shapes::join_is_not_empty(shape, &Shapes::create(*aabb), BooleanOp::AND)
        };
        // a player whose code is running is not in the storage: its record blocks building like the entity would
        self.get_entities(None, &bounds, |e| !e.is_spectator())
            .into_iter()
            .filter_map(|id| self.entity_manager.get_any(id))
            .all(|e| {
                !blocks(
                    e.id(),
                    e.is_removed(),
                    e.base().blocks_building,
                    &e.get_bounding_box(),
                )
            })
            && self.player_records.values().all(|p| {
                p.spectator || !blocks(p.id, p.removed, p.blocks_building, &p.bounding_box)
            })
    }

    /// Id-based form (no live source): every entity that can be collided with.
    fn get_entity_collisions(
        &self,
        _source: Option<EntityId>,
        test_area: &AABB,
    ) -> Vec<mc_core::phys::shapes::VoxelShape> {
        crate::world::entity::movement::EntityCollisionQuery::entity_collisions(
            self, None, test_area,
        )
    }
}

impl crate::world::entity::movement::EntityCollisionQuery for ServerLevel {
    /// `EntityGetter.getEntityCollisions(source, testArea)`.
    fn entity_collisions(
        &self,
        source: Option<&dyn Entity>,
        test_area: &AABB,
    ) -> Vec<mc_core::phys::shapes::VoxelShape> {
        if test_area.get_size() < 1.0E-7 {
            return Vec::new();
        }
        let area = test_area.inflate_scalar(1.0E-7);
        let ids = self.get_entities(source.map(|s| s.id()), &area, |e| {
            !e.is_spectator()
                && match source {
                    None => e.can_be_collided_with(None),
                    Some(s) => s.can_collide_with(self, e),
                }
        });
        ids.into_iter()
            .filter_map(|id| self.entity_manager.get_any(id))
            .map(|e| mc_core::phys::shapes::Shapes::create(e.get_bounding_box()))
            .collect()
    }
}

impl crate::world::entity::movement::MovementLevel for ServerLevel {
    fn game_time(&self) -> i64 {
        ServerLevel::game_time(self)
    }

    fn world_border_collision(
        &self,
        source: &dyn Entity,
        aabb: &AABB,
    ) -> Option<mc_core::phys::shapes::VoxelShape> {
        let border = self.world_border_ref()?;
        border
            .is_inside_close_to_border(source.base().x(), source.base().z(), aabb)
            .then(|| border.get_collision_shape())
    }

    fn has_chunks_at(&self, x0: i32, z0: i32, x1: i32, z1: i32) -> bool {
        let (cx0, cx1, cz0, cz1) = (x0 >> 4, x1 >> 4, z0 >> 4, z1 >> 4);
        (cx0..=cx1).all(|cx| {
            (cz0..=cz1).all(|cz| crate::world::level::LevelReader::has_chunk(self, cx, cz))
        })
    }
}

impl ServerLevel {
    /// `getRootVehicle()` by id; a player whose code is running is read from its record.
    fn root_vehicle(&self, id: EntityId) -> EntityId {
        let mut root = id;
        let mut cur = match self.entity_manager.get_any(id) {
            Some(e) => e.base().vehicle,
            None => self.player_records.get(&id).and_then(|p| p.vehicle),
        };
        while let Some(v) = cur {
            root = v;
            cur = self
                .entity_manager
                .get_any(v)
                .and_then(|x| x.base().vehicle);
        }
        root
    }

    /// `Entity.isPassengerOfSameVehicle(other)`: same root vehicle.
    pub fn is_passenger_of_same_vehicle(&self, a: &dyn Entity, b: &dyn Entity) -> bool {
        let root = |e: &dyn Entity| {
            let mut id = e.id();
            let mut cur = e.base().vehicle;
            while let Some(v) = cur {
                id = v;
                cur = self
                    .entity_manager
                    .get_any(v)
                    .and_then(|x| x.base().vehicle);
            }
            id
        };
        root(a) == root(b)
    }
}

/// Appends the call path of a hit that missed a lend to the file named by `MC_LEND_DETECTOR` (once per distinct line).
#[cfg(debug_assertions)]
fn report_unlent_attacker(attacker: &str, victim: &str) {
    use std::io::Write as _;
    use std::sync::{Mutex, OnceLock};
    static SEEN: OnceLock<Mutex<std::collections::HashSet<String>>> = OnceLock::new();
    let Some(path) = std::env::var_os("MC_LEND_DETECTOR") else {
        return;
    };
    // frames of this crate between the hit and the driver; the first ones are the detector and the hurt itself
    let trace = std::backtrace::Backtrace::force_capture().to_string();
    let frames: Vec<&str> = trace
        .lines()
        .filter_map(|l| l.trim().split_once(": ").map(|(_, name)| name))
        .filter(|n| {
            n.contains("mc_game") && !n.contains("note_unlent_") && !n.contains("report_unlent")
        })
        .take(6)
        .collect();
    let thread = std::thread::current();
    let line = format!(
        "{attacker} <- {victim} | {} | {}",
        frames.join(" < "),
        thread.name().unwrap_or("?")
    );
    if !SEEN
        .get_or_init(Default::default)
        .lock()
        .is_ok_and(|mut seen| seen.insert(line.clone()))
    {
        return;
    }
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let thread = std::thread::current();
        let _ = writeln!(file, "{line} [{}]", thread.name().unwrap_or("?"));
    }
}
