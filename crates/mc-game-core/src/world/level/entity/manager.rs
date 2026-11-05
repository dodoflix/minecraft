//! `net.minecraft.world.level.entity.PersistentEntitySectionManager` + `EntityLookup` + `LevelEntityGetterAdapter`.
use super::{EntitySectionStorage, EntityTickList, Visibility};
use crate::world::entity::{Entity, EntityId, RemovalReason, entity as entity_mod};
use indexmap::{IndexMap, IndexSet};
use mc_core::phys::AABB;
use mc_core::pos::chunk_pos::ChunkPos;
use mc_core::pos::{BlockPos, SectionPos};
use mc_nbt::CompoundTag;
use mc_world::region::entity_storage::{ChunkEntities, EntityStorage};
use std::collections::{HashMap, HashSet, VecDeque};
use std::ops::ControlFlow;
use uuid::Uuid;

/// `PersistentEntitySectionManager.ChunkLoadStatus`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ChunkLoadStatus {
    Fresh,
    Pending,
    Loaded,
}

/// `LevelCallback<Entity>` calls, drained by the level (`ServerLevel.EntityCallbacks`) after each operation.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EntityLevelEvent {
    Created(EntityId),
    Destroyed(EntityId),
    TickingStart(EntityId),
    TickingEnd(EntityId),
    TrackingStart(EntityId),
    TrackingEnd(EntityId),
    SectionChange(EntityId),
}

/// A change applied to an entity that was taken out of storage when it was requested.
pub type DeferredEntityOp = Box<dyn FnOnce(&mut dyn Entity)>;

struct EntitySlot {
    /// `None` while the entity is taken out (it ticks / a packet for it is handled).
    entity: Option<Box<dyn Entity>>,
    /// `Callback.currentSectionKey`.
    section_key: i64,
    /// `isAlwaysTicking()` (fixed per entity; cached so it is answerable while the entity is taken out).
    always_ticking: bool,
}

fn effective_status(always_ticking: bool, status: Visibility) -> Visibility {
    if always_ticking { Visibility::Ticking } else { status }
}

pub struct PersistentEntitySectionManager {
    known_uuids: HashSet<Uuid>,
    entities: HashMap<EntityId, EntitySlot>,
    /// `visibleEntityStorage.byId` (insertion-ordered, `Int2ObjectLinkedOpenHashMap`).
    visible_by_id: IndexMap<EntityId, ()>,
    visible_by_uuid: HashMap<Uuid, EntityId>,
    section_storage: EntitySectionStorage,
    chunk_visibility: HashMap<i64, Visibility>,
    chunk_load_statuses: HashMap<i64, ChunkLoadStatus>,
    chunks_to_unload: IndexSet<i64>,
    /// `loadingInbox`: raw entity NBT of loaded chunks, instantiated by the level (`process_pending_loads`).
    loading_inbox: VecDeque<ChunkEntities>,
    permanent_storage: Option<EntityStorage>,
    pub tick_list: EntityTickList,
    events: Vec<EntityLevelEvent>,
    /// Entities removed from storage whose events are still being processed.
    limbo: HashMap<EntityId, Box<dyn Entity>>,
    deferred: HashMap<EntityId, Vec<DeferredEntityOp>>,
}

impl PersistentEntitySectionManager {
    pub fn new(permanent_storage: Option<EntityStorage>) -> Self {
        Self {
            known_uuids: HashSet::new(),
            entities: HashMap::new(),
            visible_by_id: IndexMap::new(),
            visible_by_uuid: HashMap::new(),
            section_storage: EntitySectionStorage::new(),
            chunk_visibility: HashMap::new(),
            chunk_load_statuses: HashMap::new(),
            chunks_to_unload: IndexSet::new(),
            loading_inbox: VecDeque::new(),
            permanent_storage,
            tick_list: EntityTickList::default(),
            events: Vec::new(),
            limbo: HashMap::new(),
            deferred: HashMap::new(),
        }
    }

    // ---- events ---------------------------------------------------------------------------------------------

    /// Drain the queued `LevelCallback` calls.
    pub fn take_events(&mut self) -> Vec<EntityLevelEvent> {
        std::mem::take(&mut self.events)
    }

    pub fn has_events(&self) -> bool {
        !self.events.is_empty()
    }

    /// Entities removed while events were pending (valid until [`Self::clear_limbo`]).
    pub fn get_removed(&self, id: EntityId) -> Option<&dyn Entity> {
        self.limbo.get(&id).map(|e| e.as_ref())
    }

    /// Ids of the entities waiting in limbo.
    pub fn removed_ids(&self) -> Vec<EntityId> {
        self.limbo.keys().copied().collect()
    }

    /// Hand a removed entity to its owner (players keep existing after removal).
    pub fn take_removed(&mut self, id: EntityId) -> Option<Box<dyn Entity>> {
        self.limbo.remove(&id)
    }

    pub fn clear_limbo(&mut self) {
        self.limbo.clear();
    }

    // ---- adding ---------------------------------------------------------------------------------------------

    fn add_entity_uuid(&mut self, uuid: Uuid, describe: impl FnOnce() -> String) -> bool {
        if !self.known_uuids.insert(uuid) {
            log::warn!("UUID of added entity already exists: {}", describe());
            false
        } else {
            true
        }
    }

    /// `addNewEntity(entity)`: `Err` gives the entity back when its UUID already exists.
    pub fn add_new_entity(&mut self, entity: Box<dyn Entity>) -> Result<(), Box<dyn Entity>> {
        self.add_entity(entity, false)
    }

    fn add_entity(&mut self, entity: Box<dyn Entity>, loaded: bool) -> Result<(), Box<dyn Entity>> {
        let id = entity.id();
        if !self.add_entity_uuid(entity.uuid(), || describe(entity.as_ref())) {
            return Err(entity);
        }
        let section_key = SectionPos::as_long_block_pos(&entity.block_position());
        let chunk_visibility = &self.chunk_visibility;
        let section = self.section_storage.get_or_create_section(section_key, |chunk_key| chunk_visibility.get(&chunk_key).copied().unwrap_or_default());
        section.add(id);
        let section_status = section.status();
        let always_ticking = entity.is_always_ticking();
        let mut entity = entity;
        entity.base_mut().moved_since_sync = false;
        self.entities.insert(id, EntitySlot { entity: Some(entity), section_key, always_ticking });
        if !loaded {
            self.events.push(EntityLevelEvent::Created(id));
        }
        let status = effective_status(always_ticking, section_status);
        if status.is_accessible() {
            self.start_tracking(id);
        }
        if status.is_ticking() {
            self.start_ticking(id);
        }
        Ok(())
    }

    pub fn add_legacy_chunk_entities(&mut self, entities: Vec<Box<dyn Entity>>) {
        for e in entities {
            let _ = self.add_entity(e, true);
        }
    }

    pub fn add_world_gen_chunk_entities(&mut self, entities: Vec<Box<dyn Entity>>) {
        for e in entities {
            let _ = self.add_entity(e, false);
        }
    }

    // ---- ticking / tracking transitions -----------------------------------------------------------------------

    fn start_ticking(&mut self, id: EntityId) {
        self.tick_list.add(id);
        self.events.push(EntityLevelEvent::TickingStart(id));
    }

    fn stop_ticking(&mut self, id: EntityId) {
        self.tick_list.remove(id);
        self.events.push(EntityLevelEvent::TickingEnd(id));
    }

    fn start_tracking(&mut self, id: EntityId) {
        let Some(uuid) = self.entities.get(&id).and_then(|s| s.entity.as_ref().map(|e| e.uuid())).or_else(|| self.limbo.get(&id).map(|e| e.uuid())) else {
            return;
        };
        if self.visible_by_uuid.contains_key(&uuid) {
            log::warn!("Duplicate entity UUID {uuid}: id {id}");
        } else {
            self.visible_by_uuid.insert(uuid, id);
            self.visible_by_id.insert(id, ());
        }
        self.events.push(EntityLevelEvent::TrackingStart(id));
    }

    fn stop_tracking(&mut self, id: EntityId) {
        self.events.push(EntityLevelEvent::TrackingEnd(id));
        self.visible_by_id.shift_remove(&id);
        self.visible_by_uuid.retain(|_, v| *v != id);
    }

    // ---- chunk status ------------------------------------------------------------------------------------------

    pub fn is_ticking(&self, pos: ChunkPos) -> bool {
        self.chunk_visibility.get(&pos.pack()).copied().unwrap_or_default().is_ticking()
    }

    pub fn can_position_tick(&self, pos: BlockPos) -> bool {
        self.chunk_visibility.get(&ChunkPos::pack_block_pos(&pos)).copied().unwrap_or_default().is_ticking()
    }

    pub fn are_entities_loaded(&self, chunk_key: i64) -> bool {
        self.chunk_load_status(chunk_key) == ChunkLoadStatus::Loaded
    }

    fn chunk_load_status(&self, chunk_key: i64) -> ChunkLoadStatus {
        self.chunk_load_statuses.get(&chunk_key).copied().unwrap_or(ChunkLoadStatus::Fresh)
    }

    /// `updateChunkStatus(pos, Visibility)` (callers map `FullChunkStatus` with `Visibility::from_full_chunk_status`).
    pub fn update_chunk_status(&mut self, pos: ChunkPos, chunk_status: Visibility) {
        let chunk_key = pos.pack();
        if chunk_status == Visibility::Hidden {
            self.chunk_visibility.remove(&chunk_key);
            self.chunks_to_unload.insert(chunk_key);
        } else {
            self.chunk_visibility.insert(chunk_key, chunk_status);
            self.chunks_to_unload.shift_remove(&chunk_key);
            self.ensure_chunk_queued_for_load(chunk_key);
        }
        for section_key in self.section_storage.existing_section_positions_in_chunk(chunk_key) {
            let Some(section) = self.section_storage.get_section_mut(section_key) else { continue };
            let previous = section.update_chunk_status(chunk_status);
            let ids: Vec<EntityId> = section.entities().to_vec();
            let ids: Vec<EntityId> = ids.into_iter().filter(|id| !self.is_always_ticking(*id)).collect();
            let (was_accessible, is_accessible) = (previous.is_accessible(), chunk_status.is_accessible());
            let (was_ticking, is_ticking) = (previous.is_ticking(), chunk_status.is_ticking());
            if was_ticking && !is_ticking {
                ids.iter().for_each(|&id| self.stop_ticking(id));
            }
            if was_accessible && !is_accessible {
                ids.iter().for_each(|&id| self.stop_tracking(id));
            } else if !was_accessible && is_accessible {
                ids.iter().for_each(|&id| self.start_tracking(id));
            }
            if !was_ticking && is_ticking {
                ids.iter().for_each(|&id| self.start_ticking(id));
            }
        }
    }

    fn is_always_ticking(&self, id: EntityId) -> bool {
        self.entities.get(&id).is_some_and(|s| s.always_ticking)
    }

    // ---- loading / unloading -----------------------------------------------------------------------------------

    fn ensure_chunk_queued_for_load(&mut self, chunk_key: i64) {
        if self.chunk_load_status(chunk_key) == ChunkLoadStatus::Fresh {
            self.request_chunk_load(chunk_key);
        }
    }

    fn request_chunk_load(&mut self, chunk_key: i64) {
        self.chunk_load_statuses.insert(chunk_key, ChunkLoadStatus::Pending);
        let pos = ChunkPos::unpack(chunk_key);
        let Some(storage) = self.permanent_storage.as_mut() else {
            self.loading_inbox.push_back(ChunkEntities::new(pos, Vec::new()));
            return;
        };
        match storage.load_entities(&pos) {
            Ok(loaded) => self.loading_inbox.push_back(loaded.chunk),
            Err(e) => log::error!("Failed to read chunk {pos}: {e}"),
        }
    }

    /// Raw entity NBT of chunks whose storage read finished (`processPendingLoads` first half). The level
    /// instantiates the entities, adds them with [`Self::add_legacy_chunk_entities`] and then calls
    /// [`Self::mark_chunk_entities_loaded`].
    pub fn take_loaded_chunks(&mut self) -> Vec<ChunkEntities> {
        self.loading_inbox.drain(..).collect()
    }

    pub fn mark_chunk_entities_loaded(&mut self, pos: ChunkPos) {
        self.chunk_load_statuses.insert(pos.pack(), ChunkLoadStatus::Loaded);
    }

    /// Root entities of a chunk that `shouldBeSaved`, in section order.
    fn root_entities_to_save(&self, chunk_key: i64) -> Vec<EntityId> {
        let mut result = Vec::new();
        for section_key in self.section_storage.existing_section_positions_in_chunk(chunk_key) {
            if let Some(section) = self.section_storage.get_section(section_key) {
                for &id in section.entities() {
                    if self.should_be_saved(id) {
                        result.push(id);
                    }
                }
            }
        }
        result
    }

    /// `Entity.shouldBeSaved()`.
    fn should_be_saved(&self, id: EntityId) -> bool {
        let Some(e) = self.get_any(id) else { return false };
        if !e.should_be_saved_ignoring_riding() {
            return false;
        }
        let b = e.base();
        if b.is_passenger() {
            return false;
        }
        !b.is_vehicle() || self.count_player_passengers(id) != 1
    }

    fn count_player_passengers(&self, id: EntityId) -> usize {
        let Some(e) = self.get_any(id) else { return 0 };
        self.count_players_among(&e.base().passengers)
    }

    /// `countPlayerPassengers()` of an entity whose slot is taken out: its indirect passengers that are players.
    pub(crate) fn count_players_among(&self, passengers: &[EntityId]) -> usize {
        passengers.iter().map(|&p| usize::from(self.get_any(p).is_some_and(|p| p.is_player())) + self.count_player_passengers(p)).sum()
    }

    /// `getPassengersAndSelf()` ids (post-order).
    fn passengers_and_self(&self, id: EntityId, out: &mut Vec<EntityId>) {
        if let Some(e) = self.get_any(id) {
            for &p in &e.base().passengers {
                self.passengers_and_self(p, out);
            }
        }
        out.push(id);
    }

    fn lookup<'a>(&'a self) -> impl Fn(EntityId) -> Option<&'a dyn Entity> + 'a {
        |id| self.entities.get(&id).and_then(|s| s.entity.as_deref())
    }

    /// `storeChunkSections`: false if the chunk can't be stored yet (load pending / fresh with entities).
    fn store_chunk_sections(&mut self, chunk_key: i64, unload_saved: bool) -> bool {
        let status = self.chunk_load_status(chunk_key);
        if status == ChunkLoadStatus::Pending {
            return false;
        }
        let roots = self.root_entities_to_save(chunk_key);
        let pos = ChunkPos::unpack(chunk_key);
        if roots.is_empty() {
            if status == ChunkLoadStatus::Loaded {
                if let Some(storage) = self.permanent_storage.as_mut() {
                    storage.store_entities(&ChunkEntities::new(pos, Vec::new()));
                }
            }
            return true;
        }
        if status == ChunkLoadStatus::Fresh {
            self.request_chunk_load(chunk_key);
            return false;
        }
        let tags: Vec<CompoundTag> = {
            let lookup = self.lookup();
            roots.iter().filter_map(|&id| self.get_any(id).and_then(|e| entity_mod::save(e, &lookup))).collect()
        };
        if let Some(storage) = self.permanent_storage.as_mut() {
            storage.store_entities(&ChunkEntities::new(pos, tags));
        }
        if unload_saved {
            for root in roots {
                let mut ids = Vec::new();
                self.passengers_and_self(root, &mut ids);
                for id in ids {
                    self.unload_entity(id);
                }
            }
        }
        true
    }

    /// `unloadEntity`: `setRemoved(UNLOADED_TO_CHUNK)` + callback removal.
    fn unload_entity(&mut self, id: EntityId) {
        if let Some(e) = self.entities.get_mut(&id).and_then(|s| s.entity.as_mut()) {
            e.base_mut().set_removal_reason(RemovalReason::UnloadedToChunk);
        }
        self.finish_removal(id, RemovalReason::UnloadedToChunk);
    }

    fn process_chunk_unload(&mut self, chunk_key: i64) -> bool {
        if !self.store_chunk_sections(chunk_key, true) {
            return false;
        }
        self.chunk_load_statuses.remove(&chunk_key);
        true
    }

    /// `processUnloads()`.
    pub fn process_unloads(&mut self) {
        let keys: Vec<i64> = self.chunks_to_unload.iter().copied().collect();
        for key in keys {
            let remove = if self.chunk_visibility.get(&key).copied().unwrap_or_default() != Visibility::Hidden { true } else { self.process_chunk_unload(key) };
            if remove {
                self.chunks_to_unload.shift_remove(&key);
            }
        }
    }

    fn all_chunks_to_save(&self) -> IndexSet<i64> {
        let mut result: IndexSet<i64> = self.section_storage.get_all_chunks_with_existing_sections().into_iter().collect();
        for (&k, &s) in &self.chunk_load_statuses {
            if s == ChunkLoadStatus::Loaded {
                result.insert(k);
            }
        }
        result
    }

    /// `autoSave()`.
    pub fn auto_save(&mut self) {
        for chunk_key in self.all_chunks_to_save() {
            let should_unload = self.chunk_visibility.get(&chunk_key).copied().unwrap_or_default() == Visibility::Hidden;
            if should_unload {
                self.process_chunk_unload(chunk_key);
            } else {
                self.store_chunk_sections(chunk_key, false);
            }
        }
    }

    /// `saveAll()` minus the pending-load instantiation: `load_pending` is called between rounds (the level
    /// instantiates `take_loaded_chunks()` there) so fresh chunks can finish loading before they are stored.
    pub fn save_all(&mut self, mut load_pending: impl FnMut(&mut Self)) {
        let mut chunks_to_save = self.all_chunks_to_save();
        while !chunks_to_save.is_empty() {
            if let Some(storage) = self.permanent_storage.as_mut() {
                if let Err(e) = storage.flush(false) {
                    log::error!("Failed to flush entity storage: {e}");
                }
            }
            load_pending(self);
            let keys: Vec<i64> = chunks_to_save.iter().copied().collect();
            for chunk_key in keys {
                let should_unload = self.chunk_visibility.get(&chunk_key).copied().unwrap_or_default() == Visibility::Hidden;
                let done = if should_unload { self.process_chunk_unload(chunk_key) } else { self.store_chunk_sections(chunk_key, false) };
                if done {
                    chunks_to_save.shift_remove(&chunk_key);
                }
            }
        }
        if let Some(storage) = self.permanent_storage.as_mut() {
            if let Err(e) = storage.flush(true) {
                log::error!("Failed to flush entity storage: {e}");
            }
        }
    }

    pub fn close(&mut self) {
        if let Some(storage) = self.permanent_storage.as_mut() {
            if let Err(e) = storage.close() {
                log::error!("Failed to close entity storage: {e}");
            }
        }
    }

    // ---- removal / moving (EntityInLevelCallback) --------------------------------------------------------------

    /// Apply pending removal / section changes of `id` (what `Entity.setRemoved`/`setPosRaw` do through the level
    /// callback in vanilla). Call after any mutation of an entity that lives in storage, and on `put_back`.
    pub fn sync_entity(&mut self, id: EntityId) {
        let Some(slot) = self.entities.get_mut(&id) else { return };
        let Some(entity) = slot.entity.as_mut() else { return };
        if let Some(reason) = entity.base().removal_reason() {
            self.finish_removal(id, reason);
            return;
        }
        if entity.base().moved_since_sync {
            entity.base_mut().moved_since_sync = false;
            let new_key = SectionPos::as_long_block_pos(&entity.block_position());
            if new_key != slot.section_key {
                self.on_move(id, new_key);
            }
        }
    }

    /// `Callback.onMove`.
    fn on_move(&mut self, id: EntityId, new_section_pos: i64) {
        let old_key = self.entities[&id].section_key;
        let previous_status = self.section_storage.get_section(old_key).map(|s| s.status()).unwrap_or_default();
        let found = self.section_storage.get_section_mut(old_key).is_some_and(|s| s.remove(id));
        if !found {
            log::warn!("Entity {id} wasn't found in section {:?} (moving to {new_section_pos})", SectionPos::of_long(old_key));
        }
        self.remove_section_if_empty(old_key);
        let chunk_visibility = &self.chunk_visibility;
        let new_section = self.section_storage.get_or_create_section(new_section_pos, |k| chunk_visibility.get(&k).copied().unwrap_or_default());
        new_section.add(id);
        let new_status = new_section.status();
        if let Some(slot) = self.entities.get_mut(&id) {
            slot.section_key = new_section_pos;
        }
        self.update_status(id, previous_status, new_status);
    }

    fn update_status(&mut self, id: EntityId, previous_status: Visibility, new_status: Visibility) {
        let always = self.is_always_ticking(id);
        let eff_prev = effective_status(always, previous_status);
        let eff_new = effective_status(always, new_status);
        if eff_prev == eff_new {
            if eff_new.is_accessible() {
                self.events.push(EntityLevelEvent::SectionChange(id));
            }
        } else {
            let (was_accessible, is_accessible) = (eff_prev.is_accessible(), eff_new.is_accessible());
            if was_accessible && !is_accessible {
                self.stop_tracking(id);
            } else if !was_accessible && is_accessible {
                self.start_tracking(id);
            }
            let (was_ticking, is_ticking) = (eff_prev.is_ticking(), eff_new.is_ticking());
            if was_ticking && !is_ticking {
                self.stop_ticking(id);
            } else if !was_ticking && is_ticking {
                self.start_ticking(id);
            }
            if is_accessible {
                self.events.push(EntityLevelEvent::SectionChange(id));
            }
        }
    }

    fn remove_section_if_empty(&mut self, section_key: i64) {
        if self.section_storage.get_section(section_key).is_some_and(|s| s.is_empty()) {
            self.section_storage.remove(section_key);
        }
    }

    /// `Callback.onRemove(reason)`: leaves the section, stops ticking/tracking, forgets the UUID; the entity
    /// moves to `limbo` until the events are processed.
    fn finish_removal(&mut self, id: EntityId, reason: RemovalReason) {
        let Some(mut slot) = self.entities.remove(&id) else { return };
        let Some(mut entity) = slot.entity.take() else {
            self.entities.insert(id, slot);
            return;
        };
        let section_status = match self.section_storage.get_section_mut(slot.section_key) {
            Some(s) => {
                if !s.remove(id) {
                    log::warn!("Entity {id} wasn't found in section {:?} (destroying due to {reason:?})", SectionPos::of_long(slot.section_key));
                }
                s.status()
            }
            None => {
                log::warn!("Entity {id} wasn't found in section {:?} (destroying due to {reason:?})", SectionPos::of_long(slot.section_key));
                Visibility::Hidden
            }
        };
        let status = effective_status(slot.always_ticking, section_status);
        // keep the entity reachable for the handlers of the events queued below
        entity.base_mut().moved_since_sync = false;
        self.limbo.insert(id, entity);
        if status.is_ticking() {
            self.stop_ticking(id);
        }
        if status.is_accessible() {
            self.stop_tracking(id);
        }
        if reason.should_destroy() {
            self.events.push(EntityLevelEvent::Destroyed(id));
        }
        if let Some(uuid) = self.limbo.get(&id).map(|e| e.uuid()) {
            self.known_uuids.remove(&uuid);
        }
        self.remove_section_if_empty(slot.section_key);
        self.deferred.remove(&id);
    }

    // ---- access -----------------------------------------------------------------------------------------------

    /// The entity if it exists (even in a hidden section) and is not taken out.
    pub fn get_any(&self, id: EntityId) -> Option<&dyn Entity> {
        self.entities.get(&id).and_then(|s| s.entity.as_deref())
    }

    /// `LevelEntityGetter.get(int id)`: tracked (visible) entities only; `None` while taken out.
    pub fn get(&self, id: EntityId) -> Option<&dyn Entity> {
        if self.visible_by_id.contains_key(&id) { self.get_any(id) } else { None }
    }

    /// Mutable access; callers must `sync_entity(id)` afterwards (or use `Level::with_entity`).
    pub fn get_mut(&mut self, id: EntityId) -> Option<&mut dyn Entity> {
        if !self.visible_by_id.contains_key(&id) {
            return None;
        }
        self.entities.get_mut(&id).and_then(|s| s.entity.as_deref_mut())
    }

    /// Mutable access ignoring visibility (hidden entities included); `None` while taken out.
    pub fn get_any_mut(&mut self, id: EntityId) -> Option<&mut dyn Entity> {
        self.entities.get_mut(&id).and_then(|s| s.entity.as_deref_mut())
    }

    /// Mutable access to a live entity, or to one that was just removed and waits in limbo for its callbacks.
    pub fn get_any_or_removed_mut(&mut self, id: EntityId) -> Option<&mut dyn Entity> {
        if self.entities.get(&id).is_some_and(|s| s.entity.is_some()) {
            return self.get_any_mut(id);
        }
        self.limbo.get_mut(&id).map(|e| e.as_mut())
    }

    pub fn get_by_uuid(&self, uuid: Uuid) -> Option<&dyn Entity> {
        self.visible_by_uuid.get(&uuid).and_then(|&id| self.get_any(id))
    }

    /// Entity id registered under `uuid` (also while the entity is taken out).
    pub fn id_by_uuid(&self, uuid: Uuid) -> Option<EntityId> {
        self.visible_by_uuid.get(&uuid).copied()
    }

    /// Does an entity with this id exist (taken out or not, visible or not)? (`chunkSource.hasEntityWithId`)
    pub fn has_entity_with_id(&self, id: EntityId) -> bool {
        self.entities.contains_key(&id)
    }

    /// Take the entity out of its slot (it stays registered; lookups return `None` until `put_back`).
    pub fn take(&mut self, id: EntityId) -> Option<Box<dyn Entity>> {
        self.entities.get_mut(&id).and_then(|s| s.entity.take())
    }

    /// Put a taken-out entity back: runs deferred operations, then syncs removal / section changes.
    pub fn put_back(&mut self, id: EntityId, mut entity: Box<dyn Entity>) {
        if let Some(ops) = self.deferred.remove(&id) {
            for op in ops {
                op(entity.as_mut());
            }
        }
        match self.entities.get_mut(&id) {
            Some(slot) => slot.entity = Some(entity),
            None => {
                // the entity was destroyed while out (its slot is gone): keep it for the caller's events
                self.limbo.insert(id, entity);
                return;
            }
        }
        self.sync_entity(id);
    }

    /// Run `op` on the entity now if it is in storage, else when it is put back.
    pub fn defer_or_apply(&mut self, id: EntityId, op: DeferredEntityOp) {
        match self.entities.get_mut(&id) {
            Some(slot) => match slot.entity.as_deref_mut() {
                Some(e) => {
                    op(e);
                    self.sync_entity(id);
                }
                None => self.deferred.entry(id).or_default().push(op),
            },
            None => {}
        }
    }

    pub fn is_loaded(&self, uuid: Uuid) -> bool {
        self.known_uuids.contains(&uuid)
    }

    pub fn count(&self) -> usize {
        self.visible_by_id.len()
    }

    /// `getAll()`: visible entity ids in insertion order.
    pub fn all_visible_ids(&self) -> Vec<EntityId> {
        self.visible_by_id.keys().copied().collect()
    }

    /// `getAll()` entities (those not taken out).
    pub fn get_all(&self) -> impl Iterator<Item = &dyn Entity> {
        self.visible_by_id.keys().filter_map(|&id| self.get_any(id))
    }

    /// `getEntities(AABB, consumer)`: visits `(id, entity)` of accessible sections' entities whose box intersects.
    pub fn for_each_in_aabb(&self, bb: &AABB, mut consumer: impl FnMut(EntityId, &dyn Entity) -> ControlFlow<()>) {
        self.section_storage.for_each_accessible_non_empty_section(bb, |_, section| {
            for &id in section.entities() {
                if let Some(e) = self.get_any(id) {
                    if e.get_bounding_box().intersects_aabb(bb) && consumer(id, e).is_break() {
                        return ControlFlow::Break(());
                    }
                }
            }
            ControlFlow::Continue(())
        });
    }
    /// `for_each_in_aabb` for a class query issued BY the ticking entity (taken out of its slot): vanilla's
    /// `getEntitiesOfClass` also returns the querying entity itself, so `stand_in` is visited at its slot position.
    pub fn for_each_in_aabb_with_stand_in(&self, bb: &AABB, stand_in: &dyn Entity, mut consumer: impl FnMut(EntityId, &dyn Entity) -> ControlFlow<()>) {
        let stand_in_id = stand_in.id();
        self.section_storage.for_each_accessible_non_empty_section(bb, |_, section| {
            for &id in section.entities() {
                let e = if id == stand_in_id { Some(stand_in) } else { self.get_any(id) };
                if let Some(e) = e {
                    if e.get_bounding_box().intersects_aabb(bb) && consumer(id, e).is_break() {
                        return ControlFlow::Break(());
                    }
                }
            }
            ControlFlow::Continue(())
        });
    }

    pub fn gather_stats(&self) -> String {
        format!(
            "{},{},{},{},{},{},{}",
            self.known_uuids.len(),
            self.visible_by_id.len(),
            self.section_storage.count(),
            self.chunk_load_statuses.len(),
            self.chunk_visibility.len(),
            self.loading_inbox.len(),
            self.chunks_to_unload.len()
        )
    }
}

fn describe(e: &dyn Entity) -> String {
    format!("{}['{}'/{}, x={:.2}, y={:.2}, z={:.2}]", e.get_type().data().name, e.get_type().data().name, e.id(), e.position().x, e.position().y, e.position().z)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::entity::plain::PlainEntity;
    use mc_data::EntityType;

    fn pig(id: EntityId, x: f64, y: f64, z: f64) -> Box<dyn Entity> {
        let mut e = PlainEntity::new(EntityType::PIG, id);
        e.set_pos(x, y, z);
        e.base_mut().moved_since_sync = false;
        Box::new(e)
    }

    fn collect(m: &PersistentEntitySectionManager, bb: &AABB) -> Vec<EntityId> {
        let mut out = vec![];
        m.for_each_in_aabb(bb, |id, _| {
            out.push(id);
            ControlFlow::Continue(())
        });
        out
    }

    #[test]
    fn visibility_follows_chunk_status_and_queries_see_only_accessible_sections() {
        let mut m = PersistentEntitySectionManager::new(None);
        m.add_new_entity(pig(1, 1.0, 64.0, 1.0)).ok().unwrap();
        m.add_new_entity(pig(2, 2.0, 64.0, 2.0)).ok().unwrap();
        let bb = AABB::new(0.0, 60.0, 0.0, 10.0, 70.0, 10.0);
        assert!(collect(&m, &bb).is_empty(), "hidden chunk");
        assert!(m.get(1).is_none());
        m.update_chunk_status(ChunkPos::new(0, 0), Visibility::Ticking);
        let _ = m.take_loaded_chunks();
        m.mark_chunk_entities_loaded(ChunkPos::new(0, 0));
        assert_eq!(collect(&m, &bb), vec![1, 2]);
        assert!(m.get(2).is_some());
        assert!(m.tick_list.contains(1));
        let events = m.take_events();
        assert!(events.contains(&EntityLevelEvent::TrackingStart(1)) && events.contains(&EntityLevelEvent::TickingStart(2)));
        m.update_chunk_status(ChunkPos::new(0, 0), Visibility::Tracked);
        assert!(!m.tick_list.contains(1) && m.get(1).is_some());
    }

    #[test]
    fn take_out_hides_entity_and_put_back_moves_it_between_sections() {
        let mut m = PersistentEntitySectionManager::new(None);
        m.update_chunk_status(ChunkPos::new(0, 0), Visibility::Ticking);
        m.update_chunk_status(ChunkPos::new(1, 0), Visibility::Tracked);
        m.add_new_entity(pig(1, 1.0, 64.0, 1.0)).ok().unwrap();
        m.take_events();
        let mut e = m.take(1).unwrap();
        assert!(m.get(1).is_none(), "own lookup excludes the taken-out entity");
        e.set_pos(17.5, 64.0, 1.0);
        m.put_back(1, e);
        assert!(!m.tick_list.contains(1), "moved into a tracked-only chunk");
        let ev = m.take_events();
        assert!(ev.contains(&EntityLevelEvent::TickingEnd(1)) && ev.contains(&EntityLevelEvent::SectionChange(1)));
        assert_eq!(collect(&m, &AABB::new(16.0, 60.0, 0.0, 20.0, 70.0, 4.0)), vec![1]);
    }

    #[test]
    fn removal_is_applied_on_put_back_and_uuid_is_forgotten() {
        let mut m = PersistentEntitySectionManager::new(None);
        m.update_chunk_status(ChunkPos::new(0, 0), Visibility::Ticking);
        m.add_new_entity(pig(1, 1.0, 64.0, 1.0)).ok().unwrap();
        let uuid = m.get(1).unwrap().uuid();
        let mut e = m.take(1).unwrap();
        e.discard();
        m.put_back(1, e);
        assert!(!m.is_loaded(uuid) && m.get_any(1).is_none() && m.count() == 0);
        let ev = m.take_events();
        assert!(ev.contains(&EntityLevelEvent::Destroyed(1)) && ev.contains(&EntityLevelEvent::TrackingEnd(1)));
        assert!(m.get_removed(1).is_some());
    }

    #[test]
    fn duplicate_uuid_is_rejected() {
        let mut m = PersistentEntitySectionManager::new(None);
        let a = pig(1, 0.0, 0.0, 0.0);
        let mut b = PlainEntity::new(EntityType::PIG, 2);
        b.base_mut().uuid = a.uuid();
        m.add_new_entity(a).ok().unwrap();
        assert!(m.add_new_entity(Box::new(b)).is_err());
    }
}
