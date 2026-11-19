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
/// hold the object are not looked at); after that it stays only while a brain memory or a player's last explosion cause still names it (see
/// `ServerLevel::expire_recently_removed`).
const RECENTLY_REMOVED_RETENTION_TICKS: i64 = 20;

/// Entity chunks the storage writes to disk per tick: vanilla's IO thread works through the queue between the saves.
const ENTITY_CHUNKS_STORED_PER_TICK: usize = 16;

/// `ServerLevel.ENTITY_COUNTER` (static: shared by all levels).
static ENTITY_COUNTER: AtomicI32 = AtomicI32::new(0);

/// `ServerLevel.ENTITY_COUNTER.incrementAndGet()` for entities created off the game thread (worldgen): no level to
/// check for taken ids (they are saved as NBT and get a fresh id when loaded).
pub fn next_detached_entity_id() -> EntityId {
    ENTITY_COUNTER
        .fetch_add(1, Ordering::Relaxed)
        .wrapping_add(1)
}

/// What `getEntityInAnyDimension` found: the entity itself in this level, or the copy another level published.
pub enum AnyDimensionEntity<'a> {
    Here(&'a dyn Entity),
    Elsewhere(Box<super::RemoteEntity>),
}

impl AnyDimensionEntity<'_> {
    /// `getType()`.
    pub fn get_type(&self) -> EntityType {
        match self {
            Self::Here(e) => e.get_type(),
            Self::Elsewhere(r) => r.entity_type,
        }
    }

    pub fn id(&self) -> EntityId {
        match self {
            Self::Here(e) => e.id(),
            Self::Elsewhere(r) => r.id,
        }
    }

    pub fn uuid(&self) -> Uuid {
        match self {
            Self::Here(e) => e.uuid(),
            Self::Elsewhere(r) => r.uuid,
        }
    }

    pub fn position(&self) -> mc_core::phys::Vec3 {
        match self {
            Self::Here(e) => e.position(),
            Self::Elsewhere(r) => r.position,
        }
    }

    pub fn block_position(&self) -> mc_core::pos::BlockPos {
        match self {
            Self::Here(e) => e.block_position(),
            Self::Elsewhere(r) => r.block_position(),
        }
    }

    /// `getBbHeight()`.
    pub fn bb_height(&self) -> f32 {
        match self {
            Self::Here(e) => e.base().get_bb_height(),
            Self::Elsewhere(r) => r.bb_height,
        }
    }

    /// `instanceof LivingEntity`.
    pub fn is_living(&self) -> bool {
        match self {
            Self::Here(e) => e.living().is_some(),
            Self::Elsewhere(r) => r.is_living,
        }
    }

    /// `instanceof Mob`.
    pub fn is_mob(&self) -> bool {
        match self {
            Self::Here(e) => e.is_mob(),
            Self::Elsewhere(r) => r.is_mob,
        }
    }

    /// `isAlive()`.
    pub fn is_alive(&self) -> bool {
        match self {
            Self::Here(e) => e.is_alive(),
            Self::Elsewhere(r) => r.alive,
        }
    }

    pub fn is_spectator(&self) -> bool {
        match self {
            Self::Here(e) => e.is_spectator(),
            Self::Elsewhere(r) => r.spectator,
        }
    }

    /// `canInteractWithLevel()` (an entity a level tracks is not removed).
    pub fn can_interact_with_level(&self) -> bool {
        match self {
            Self::Here(e) => e.can_interact_with_level(),
            Self::Elsewhere(r) => r.alive && !r.spectator,
        }
    }

    /// `instanceof Player`.
    pub fn is_player(&self) -> bool {
        match self {
            Self::Here(e) => e.is_player(),
            Self::Elsewhere(r) => r.is_player,
        }
    }

    /// `getScoreboardName()`.
    pub fn scoreboard_name(&self) -> String {
        match self {
            Self::Here(e) => e.get_scoreboard_name(),
            Self::Elsewhere(r) => r.scoreboard_name.clone(),
        }
    }

    /// `instanceof OwnableEntity`.
    pub fn is_ownable(&self) -> bool {
        match self {
            Self::Here(e) => e.is_ownable(),
            Self::Elsewhere(r) => r.is_ownable,
        }
    }

    /// `OwnableEntity.getOwnerReference()` (`None` for entities that are not ownable).
    pub fn ownable_owner_uuid(&self) -> Option<Uuid> {
        match self {
            Self::Here(e) => e.ownable_owner_uuid(),
            Self::Elsewhere(r) => r.ownable_owner,
        }
    }
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
    /// The level-dependent part of loading and of `finalizeSpawn`'s `setAge` ran in vanilla before this call: it runs first,
    /// so a tadpole that converted while loading is already removed.
    pub fn add_fresh_entity(&mut self, entity: Box<dyn Entity>) -> bool {
        let mut entity = entity;
        entity.read_level_dependent_data(self);
        crate::world::entity::mob::ageable::run_pending_age_boundary(entity.as_mut(), self);
        if entity.is_removed() {
            log::warn!(
                "Tried to add entity {} but it was marked as removed already",
                entity.get_type().data().name
            );
            return false;
        }
        // `addFreshEntityWithPassengers`: passengers created by `finalizeSpawn` (spider jockey) follow their vehicle
        let passengers = entity
            .mob_mut()
            .map(|m| std::mem::take(&mut m.pending_passengers))
            .unwrap_or_default();
        // the vanilla caller keeps the object it could not add (a replay's references dump it): kept where removed entities are
        let ok = match self.entity_manager.add_new_entity(entity) {
            Ok(()) => true,
            Err(rejected) => {
                if let Some(kept) = self.removed_entities.as_mut() {
                    kept.insert(rejected.id(), rejected);
                }
                false
            }
        };
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
    pub fn try_add_fresh_entity_with_passengers(
        &mut self,
        mut entities: Vec<Box<dyn Entity>>,
    ) -> bool {
        if entities
            .iter()
            .any(|e| self.entity_manager.is_loaded(e.uuid()))
        {
            // the loading of the entities (`readAdditionalSaveData`, `addPassenger` of the loaded passengers) happened already
            for entity in &mut entities {
                entity.read_level_dependent_data(self);
            }
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

    /// What `hurtServer(level, ..)` does with its `level` argument (the damage event, the block sound, `onMobHurt` effects)
    /// when that is not the level of the entity `entity` it hurts: queued on `tick_state.hurt_level_work` for the caller that
    /// hurt it from the other level to run there; here and now otherwise.
    pub fn in_hurt_level(
        &mut self,
        entity: EntityId,
        f: impl FnOnce(&mut ServerLevel) + Send + 'static,
    ) {
        if self.tick_state.hurt_through_other_level == Some(entity) {
            self.tick_state.hurt_level_work.push(Box::new(f));
        } else {
            f(self);
        }
    }

    /// `getEntity(id)`: a tracked entity that is not currently taken out, or the entity that is loading. A player of another
    /// level is no entity of this level, so not found here: see [`ServerLevel::get_entity_or_remote_target`].
    pub fn get_entity(&self, id: EntityId) -> Option<&dyn Entity> {
        self.entity_manager
            .get(id)
            .or_else(|| self.loading_entity.as_deref().filter(|e| e.id() == id))
    }

    /// [`ServerLevel::get_entity`], or the stand-in of a player of another level that code of this level holds as an object
    /// (a mob's target, the entity of a damage source, the killer of a loot table; see
    /// [`RemoteTargets`](super::RemoteTargets)). Only for code that vanilla runs on the `Player` it holds, whichever level
    /// it is in: the stand-in has no connection, is no member of the level and is never lent.
    pub fn get_entity_or_remote_target(&self, id: EntityId) -> Option<&dyn Entity> {
        self.get_entity(id).or_else(|| {
            // a player that has come to this level is the level's own entity
            let stand_in = (!self.entity_manager.has_entity_with_id(id))
                .then(|| self.remote_targets.get(id))
                .flatten()?;
            // a reader keeps the player wanted: its full state stays current for the stand-in
            self.server_state.entities.want(stand_in.uuid());
            Some(stand_in)
        })
    }

    /// `EntityReference.getLivingEntity(reference, level)` for a mob's target or anger target: the entity of this level, or
    /// the stand-in of a player of another level (made when first asked for).
    pub fn get_entity_or_remote_player(&self, uuid: Uuid) -> Option<&dyn Entity> {
        if let Some(entity) = self.get_entity_by_uuid(uuid) {
            return Some(entity);
        }
        let id = self
            .server_state
            .entities
            .with(uuid, |remote| self.make_stand_in(remote))??;
        self.server_state.entities.want(uuid);
        self.remote_targets.get(id)
    }

    /// The entity that a goal or behavior holds by id between ticks (`lookAt`, a begging player, a ram target, a mob's target):
    /// vanilla keeps the object, which stays valid when it is a living entity that went to another level (a player that
    /// changed level). [`ServerLevel::get_entity`], or the stand-in of that entity (made when first asked for).
    pub fn get_held_entity(&self, id: EntityId) -> Option<&dyn Entity> {
        self.get_entity_or_remote_target(id).or_else(|| {
            self.hold_entity_of_other_level(id);
            self.get_entity_or_remote_target(id)
        })
    }

    /// Makes the stand-in of `remote` findable by [`ServerLevel::get_entity_or_remote_target`] when it is a player of another level: a
    /// damage source built from a snapshot of its player names the player by id, and the predicates that judge the
    /// source look that entity up in this level. Returns the player's id. The player's saved state reaches the stand-in
    /// with the player's next refresh: only a player somebody asked for is copied that way.
    pub fn ensure_remote_player(&self, remote: &super::RemoteEntity) -> Option<EntityId> {
        let id = self.make_stand_in(remote)?;
        self.server_state.entities.want(remote.uuid);
        Some(id)
    }

    /// Makes the living entity `id` of another level findable by [`ServerLevel::get_entity_or_remote_target`] (the cause of a
    /// damage source that was made in that level, which this level's predicates look up by id). Nothing for an entity of this level.
    pub fn hold_entity_of_other_level(&self, id: EntityId) {
        if self.get_entity_or_remote_target(id).is_some() {
            return;
        }
        if let Some(remote) = self.remote_entity_by_id(id)
            && remote.is_living
        {
            self.remote_targets.ensure(&remote, &self.registries);
            self.server_state.entities.want(remote.uuid);
        }
    }

    fn make_stand_in(&self, remote: &super::RemoteEntity) -> Option<EntityId> {
        (remote.is_player && remote.dimension != self.dimension).then(|| {
            self.remote_targets.ensure(remote, &self.registries);
            remote.id
        })
    }

    /// `EntityReference.getLivingEntity(reference, level)`: the living entity `uuid` of this level, or the stand-in of
    /// one that another level tracks (made when first asked for; a player's by [`ServerLevel::get_entity_or_remote_player`]
    /// too). Its saved state reaches the stand-in with the entity's next put-back once the entity is wanted, so the
    /// reference should be one that names it (see `owner_reference`) for the state to be there.
    pub fn get_entity_or_remote_living(&self, uuid: Uuid) -> Option<&dyn Entity> {
        if let Some(entity) = self.get_entity_by_uuid(uuid) {
            return Some(entity).filter(|e| e.living().is_some());
        }
        let id = self.server_state.entities.with(uuid, |remote| {
            (remote.is_living && remote.dimension != self.dimension).then(|| {
                self.remote_targets.ensure(remote, &self.registries);
                remote.id
            })
        })??;
        self.server_state.entities.want(uuid);
        self.remote_targets.get(id)
    }

    /// Whether `id` names the stand-in of a player of another level (`entity.level() != this.level()`).
    pub fn is_remote_target(&self, id: EntityId) -> bool {
        self.remote_targets.contains(id) && !self.entity_manager.has_entity_with_id(id)
    }

    /// The copy of the entity `id` of another level of the server (an indirect explosion source that is not here).
    pub fn remote_entity_by_id(&self, id: EntityId) -> Option<super::RemoteEntity> {
        self.server_state
            .entities
            .get_by_id(id)
            .filter(|remote| remote.dimension != self.dimension)
    }

    /// [`ServerLevel::get_entity`], or the entity its running code lent although it removed itself meanwhile.
    pub fn get_entity_or_lent(&self, id: EntityId) -> Option<&dyn Entity> {
        self.get_entity(id)
            .or_else(|| self.entity_manager.lent_including_removed(id))
    }

    /// [`ServerLevel::get_entity`], or a removed non-player entity that a brain memory may still hold (removed at most
    /// [`RECENTLY_REMOVED_RETENTION_TICKS`] ticks ago, or still named by a memory or by a player's last explosion cause):
    /// vanilla's memories keep the removed object, so a behavior that does not check `isRemoved` still reads its position
    /// until the memory is rewritten.
    pub fn get_entity_or_recently_removed(&self, id: EntityId) -> Option<&dyn Entity> {
        self.get_entity(id).or_else(|| {
            self.recently_removed
                .iter()
                .find(|(_, removed, _)| *removed == id)
                .map(|(_, _, e)| e.as_ref())
        })
    }

    /// An entity of a damage source (`getEntity()` / `getDirectEntity()`): [`ServerLevel::get_entity_or_recently_removed`], or
    /// the stand-in of a player of another level.
    pub fn get_source_entity(&self, id: EntityId) -> Option<&dyn Entity> {
        self.get_entity_or_remote_target(id)
            .or_else(|| self.get_entity_or_recently_removed(id))
    }

    /// [`ServerLevel::get_entity`], a killed player that left the level, or the entity that was removed when
    /// [`ServerLevel::removed_entities`] keeps them (it also keeps the entities the level refused to add).
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

    /// `getEntityOrPart(id)`: [`ServerLevel::get_entity`], or a hitbox of a tracked multipart entity.
    pub fn get_entity_or_part(&self, id: EntityId) -> Option<&dyn Entity> {
        self.get_entity(id).or_else(|| self.dragon_part(id))
    }

    /// A hitbox of a tracked multipart entity (`dragonParts.get(id)`), unless it is taken out.
    pub fn dragon_part(&self, id: EntityId) -> Option<&dyn Entity> {
        self.dragon_parts
            .get(i64::from(id))
            .and_then(Option::as_deref)
    }

    pub fn dragon_part_mut(&mut self, id: EntityId) -> Option<&mut dyn Entity> {
        self.dragon_parts
            .get_mut(i64::from(id))
            .and_then(Option::as_deref_mut)
    }

    /// `dragonParts()`: the hitboxes in the iteration order of vanilla's `Int2ObjectOpenHashMap`.
    pub fn dragon_parts(&self) -> impl Iterator<Item = &dyn Entity> {
        self.dragon_parts.values().filter_map(|p| p.as_deref())
    }

    /// The multipart entity a hitbox belongs to (`EnderDragonPart.parentMob`).
    pub fn dragon_part_parent(&self, id: EntityId) -> Option<EntityId> {
        self.dragon_part(id).and_then(part_parent)
    }

    pub fn get_entity_by_uuid(&self, uuid: Uuid) -> Option<&dyn Entity> {
        self.entity_manager.get_by_uuid(uuid)
    }

    /// `getEntityInAnyDimension(uuid)`: the entity of this level, or a copy of the tracked entity of another level of the
    /// server (see [`EntityDirectory`](super::EntityDirectory)). An entity of this level that is out of its slot (its own
    /// code runs) is not found, like [`ServerLevel::get_entity_by_uuid`].
    pub fn get_entity_in_any_dimension(&self, uuid: Uuid) -> Option<AnyDimensionEntity<'_>> {
        if let Some(entity) = self.get_entity_by_uuid(uuid) {
            return Some(AnyDimensionEntity::Here(entity));
        }
        self.server_state
            .entities
            .get(uuid)
            .filter(|remote| remote.dimension != self.dimension)
            .map(|remote| AnyDimensionEntity::Elsewhere(Box::new(remote)))
    }

    /// Whether another level of the server tracks the entity `uuid` (it is no entity of this level).
    pub fn is_in_other_level(&self, uuid: Uuid) -> bool {
        matches!(
            self.get_entity_in_any_dimension(uuid),
            Some(AnyDimensionEntity::Elsewhere(_))
        )
    }

    /// `getPlayerInAnyDimension(uuid)` = `PlayerList.getPlayer(uuid)`: the connection of the player, wherever it is (also
    /// while it is dead and out of every level).
    pub fn get_player_in_any_dimension(
        &self,
        uuid: Uuid,
    ) -> Option<crate::server::players::PlayerHandle> {
        self.server_state.roster.get(uuid)
    }

    /// `EntityReference.get(reference, level, ServerPlayer.class)` as a yes/no: a player of any level (a dead one that is
    /// removed from its level is none).
    pub fn is_player_in_any_dimension(&self, uuid: Uuid) -> bool {
        self.server_state
            .entities
            .with(uuid, |remote| remote.is_player)
            .unwrap_or(false)
    }

    /// `isAlive()` of the entity `id` that code holds as an object (`lastHurtMob`): an entity of this level, or the copy
    /// of one that another level tracks (one of this level that its own code has taken out is read from its copy too).
    pub fn is_alive_in_any_dimension(&self, id: EntityId) -> bool {
        self.get_entity(id).map_or_else(
            || {
                self.server_state
                    .entities
                    .get_by_id(id)
                    .is_some_and(|r| r.alive)
            },
            |e| e.is_alive(),
        )
    }

    /// The copy of the living entity `uuid` that another level tracks (`getLivingEntity` of a reference whose entity is not
    /// in this level).
    pub fn living_in_other_level(&self, uuid: Uuid) -> Option<super::RemoteEntity> {
        match self.get_entity_in_any_dimension(uuid)? {
            AnyDimensionEntity::Elsewhere(remote) if remote.is_living => Some(*remote),
            _ => None,
        }
    }

    /// `source` with `remote`, an entity of another level, as its causing entity (a living one: what the copy kept of it, at
    /// the position of the copy); a player of another level also becomes findable in this one.
    pub fn attribute_to_remote_entity(
        &self,
        source: crate::world::damagesource::DamageSource,
        remote: &super::RemoteEntity,
    ) -> crate::world::damagesource::DamageSource {
        self.ensure_remote_player(remote);
        match &remote.damage_entity {
            Some(owner) => {
                let mut owner = (**owner).clone();
                owner.position = remote.position;
                source.with_causing_snapshot(owner)
            }
            None => source,
        }
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
        let Some(entity) = self
            .entity_manager
            .take(id)
            .or_else(|| self.dead_players.remove(&id))
        else {
            return self
                .dragon_parts
                .get_mut(i64::from(id))
                .and_then(Option::take);
        };
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
        if entity.is_ender_dragon_part() {
            if let Some(slot) = self.dragon_parts.get_mut(i64::from(id)) {
                *slot = Some(entity);
            }
            return;
        }
        self.player_records.remove(&id);
        self.server_state
            .entities
            .refresh(entity.as_ref(), &self.dimension);
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
                &mut Entities::new(&mut level.entity_manager, &level.server_state.entities),
                &mut level.debug_synchronizers,
                id,
                None,
            );
        });
    }

    /// Apply pending removal / section changes of an entity that was mutated in place.
    pub fn sync_entity(&mut self, id: EntityId) {
        if let Some(entity) = self.entity_manager.get_any(id) {
            self.server_state.entities.refresh(entity, &self.dimension);
        }
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

    /// Run `f` on an entity that was removed a moment ago (see `recently_removed`): vanilla's caller still holds the object and
    /// calls into it, an explosion of a crystal that an earlier blast of the same tick removed. `None` when it is not kept.
    pub fn with_recently_removed<R>(
        &mut self,
        id: EntityId,
        f: impl FnOnce(&mut ServerLevel, &mut dyn Entity) -> R,
    ) -> Option<R> {
        let index = self
            .recently_removed
            .iter()
            .position(|(_, removed, _)| *removed == id)?;
        let (at, id, mut entity) = self.recently_removed.remove(index);
        let result = f(self, entity.as_mut());
        let index = index.min(self.recently_removed.len());
        self.recently_removed.insert(index, (at, id, entity));
        Some(result)
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
        self.entity_manager
            .store_pending_chunks(ENTITY_CHUNKS_STORED_PER_TICK);
        self.process_entity_events();
        self.expire_recently_removed();
    }

    /// Every [`RECENTLY_REMOVED_RETENTION_TICKS`] ticks, forgets the removed entities that are older than that and that no
    /// brain memory holds any more. Vanilla's memories keep the removed object for as long as they live (`AVOID_TARGET`
    /// up to 400 ticks, `ATTACK_TARGET` of a piglin whose fight behaviours are paused by admiring), so the entity stays
    /// readable while a memory still names it; a player's `currentExplosionCause` holds its entity the same way until the
    /// player starts to fall.
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
            // a player that was hit by an explosion keeps naming its cause until it starts to fall
            if let Some(player) = entity
                .as_any()
                .downcast_ref::<crate::world::entity::ServerPlayer>()
            {
                held.extend(player.current_explosion_cause);
            }
        }
        for dead in self.dead_players.values() {
            if let Some(player) = dead
                .as_any()
                .downcast_ref::<crate::world::entity::ServerPlayer>()
            {
                held.extend(player.current_explosion_cause);
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
                self.entity_manager
                    .get_removed(id)
                    .is_some_and(is_parked_dead_player)
            })
            .collect();
        for id in dead {
            if let Some(player) = self.entity_manager.take_removed(id) {
                self.dead_players.insert(id, player);
            }
        }
        let now = self.game_time();
        let limbo = self.entity_manager.drain_limbo();
        for (id, removed) in &limbo {
            if let Some(owner) = removed.ender_pearl_owner_to_deregister() {
                crate::server::players::ender_pearl_ticket::deregister_ender_pearl(
                    self, owner, *id,
                );
            }
        }
        let (players, removed): (Vec<_>, Vec<_>) =
            limbo.into_iter().partition(|(_, e)| e.is_player());
        if let Some(kept) = self.removed_entities.as_mut() {
            kept.extend(players);
        }
        self.recently_removed
            .extend(removed.into_iter().map(|(id, e)| (now, id, e)));
    }

    /// `removePlayerImmediately(player, CHANGED_DIMENSION)`: the chunk map stops tracking the player now, ahead of the queued
    /// tracking-end event. The player takes its chunk sender to the new level, which marks its chunks pending in that sender
    /// first thing: dropping this level's chunks afterwards would clear the marks of the chunks both levels share by position.
    pub fn stop_tracking_player_chunks(&mut self, id: EntityId) {
        self.chunk_source.chunk_map.remove_removed_player(id);
    }

    /// `ChunkMap.removeEntity(player)` for a player that leaves this level for another: its connection hears
    /// `remove_entities` for everything it was shown here. The stand-in that stays in the level has no connection, so
    /// this runs with the record of the real player before the later removal of the stand-in.
    pub fn stop_tracking_entities_for(&mut self, player: &PlayerRecord) {
        self.entity_tracker.remove_player_pairings(
            &mut Entities::new(&mut self.entity_manager, &self.server_state.entities),
            &mut self.debug_synchronizers,
            player,
        );
    }

    /// A killed player that is no longer in the level (see [`ServerLevel::dead_players`]).
    pub fn get_dead_player(&self, id: EntityId) -> Option<&dyn Entity> {
        self.dead_players.get(&id).map(|e| e.as_ref())
    }

    fn handle_entity_event(&mut self, event: EntityLevelEvent) {
        match event {
            EntityLevelEvent::TrackingStart(id) => {
                if let Some(entity) = self.entity_manager.get_any(id) {
                    self.server_state.entities.publish(entity, &self.dimension);
                }
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
                    &mut Entities::new(&mut self.entity_manager, &self.server_state.entities),
                    &mut self.debug_synchronizers,
                    &self.chunk_source.chunk_map,
                    &players,
                    id,
                );
                if let Some(parts) = self
                    .entity_manager
                    .get_mut(id)
                    .map(|e| e.take_sub_entities())
                {
                    for part in parts {
                        self.dragon_parts.insert(i64::from(part.id()), Some(part));
                    }
                }
                self.update_dynamic_game_event_listener(id, DynamicListenerAction::Add);
            }
            EntityLevelEvent::TrackingEnd(id) => {
                self.server_state.entities.withdraw(id, &self.dimension);
                self.navigating_mobs.remove(&id);
                if self.players.contains(&id) {
                    // the entity is removed when its tracking ends: not alive, so no forget packets
                    self.chunk_source.chunk_map.remove_removed_player(id);
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
                    .map(|p| {
                        let mut record = PlayerRecord::of(p);
                        // the stand-in of a respawning player has no connection: the entities it watched still drop out
                        if let Some((player, connection)) = &self.removing_player_connection
                            && *player == id
                        {
                            record.connection = Some(connection.clone());
                        }
                        record
                    });
                self.entity_tracker.remove_entity(
                    &mut Entities::new(&mut self.entity_manager, &self.server_state.entities),
                    &mut self.debug_synchronizers,
                    id,
                    player.as_ref(),
                );
                let part_ids: Vec<EntityId> = self
                    .entity_manager
                    .get_removed(id)
                    .or_else(|| self.entity_manager.get_any(id))
                    .map_or_else(Vec::new, |e| e.sub_entity_ids().to_vec());
                let parts: Vec<Box<dyn Entity>> = part_ids
                    .iter()
                    .filter_map(|part| self.dragon_parts.remove(i64::from(*part)).flatten())
                    .collect();
                if !parts.is_empty()
                    && let Some(entity) = self.entity_manager.get_any_mut(id)
                {
                    entity.restore_sub_entities(parts);
                }
                self.update_dynamic_game_event_listener(id, DynamicListenerAction::Remove);
            }
            EntityLevelEvent::SectionChange(id) => {
                self.update_dynamic_game_event_listener(id, DynamicListenerAction::Move)
            }
            // `onDestroyed` -> `Scoreboard.entityRemoved` (the waypoint half runs in `process_entity_events`)
            EntityLevelEvent::Destroyed(id) => {
                if let Some((name, is_player, is_alive)) = self
                    .entity_manager
                    .get_removed(id)
                    .or_else(|| self.entity_manager.get_any(id))
                    .map(|e| (e.get_scoreboard_name(), e.is_player(), e.is_alive()))
                {
                    self.scoreboard_mut().entity_removed(
                        &crate::world::scores::score::NameOnlyHolder(name),
                        is_player,
                        is_alive,
                    );
                }
            }
            // tick list is maintained by the manager; the other callbacks have no state of their own yet
            EntityLevelEvent::Created(_)
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
            directory: &self.server_state.entities,
            game_time: self.game_time(),
        };
        self.entity_tracker.tick(
            &mut Entities::new(&mut self.entity_manager, &self.server_state.entities),
            &mut self.debug_synchronizers,
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
                &mut Entities::new(&mut level.entity_manager, &level.server_state.entities),
                &mut level.debug_synchronizers,
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
        for part in self.dragon_parts() {
            if Some(part.id()) != except
                && part_parent(part) != except
                && selector(part)
                && bb.intersects_aabb(&part.get_bounding_box())
            {
                out.push(part.id());
            }
        }
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
            if test(e) {
                if selector(e) {
                    out.push(id);
                }
                // the hitboxes of a multipart entity found in the box are tested on their own, wherever they are
                for part in e
                    .sub_entity_ids()
                    .iter()
                    .filter_map(|p| self.dragon_part(*p))
                {
                    if test(part) && selector(part) {
                        out.push(part.id());
                    }
                }
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
        self.chunk_source
            .get_chunk_without_loading(chunk_x, chunk_z)
            .is_some()
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

/// `EnderDragonPart.parentMob` of a hitbox.
fn part_parent(part: &dyn Entity) -> Option<EntityId> {
    part.as_any()
        .downcast_ref::<crate::world::entity::boss::ender_dragon::EnderDragonPart>()
        .map(|part| part.parent)
}

/// A killed player that keeps its connection: it leaves the level but the player lives on (see
/// [`ServerLevel::dead_players`]). A player that left the player list is removed for good.
fn is_parked_dead_player(e: &dyn Entity) -> bool {
    e.is_player()
        && e.base().removal_reason() == Some(RemovalReason::Killed)
        && e.as_any()
            .downcast_ref::<crate::world::entity::ServerPlayer>()
            .is_some_and(|p| p.connection.is_some() && !p.removed_from_player_list)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::entity::factory::{self, EntitySpawnRequest};
    use mc_data::enums::EntitySpawnReason;

    #[test]
    fn a_dragon_that_stops_being_tracked_keeps_its_parts_and_their_places() {
        let mut level = ServerLevel::detached(
            mc_core::ident::Identifier::parse("minecraft:overworld").unwrap(),
        );
        level.use_private_entity_counter(100);
        level.entity_manager.update_chunk_status(
            mc_core::pos::chunk_pos::ChunkPos::new(0, 0),
            crate::world::level::entity::Visibility::Ticking,
        );
        let request = EntitySpawnRequest {
            reason: EntitySpawnReason::Command,
            ignore_checks: true,
        };
        let dragon = factory::create(EntityType::ENDER_DRAGON, &level, request).unwrap();
        let id = dragon.id();
        assert!(level.add_fresh_entity(dragon));
        level.dragon_part_mut(103).unwrap().set_pos(4.0, 5.0, 6.0);

        level.handle_entity_event(EntityLevelEvent::TrackingEnd(id));
        assert!((102..=109).all(|part| level.dragon_part(part).is_none()));
        level.handle_entity_event(EntityLevelEvent::TrackingStart(id));
        assert!((102..=109).all(|part| level.dragon_part(part).is_some()));
        assert_eq!(
            level.dragon_part(103).unwrap().position(),
            mc_core::phys::Vec3::new(4.0, 5.0, 6.0)
        );
    }

    /// A killed player with a connection is parked; once it left the player list it is removed for good.
    #[test]
    fn a_dead_player_that_left_the_player_list_is_not_parked_again() {
        let mut level = ServerLevel::detached(
            mc_core::ident::Identifier::parse("minecraft:overworld").unwrap(),
        );
        level.entity_manager.update_chunk_status(
            mc_core::pos::chunk_pos::ChunkPos::new(0, 0),
            crate::world::level::entity::Visibility::Ticking,
        );
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let mut player = crate::world::entity::ServerPlayer::new(
            1,
            mc_core::ident::Identifier::parse("minecraft:overworld").unwrap(),
            mc_protocol::codec::GameProfile {
                id: uuid::Uuid::from_u128(1),
                name: "A".into(),
                properties: vec![],
            },
            Default::default(),
        );
        player.connection = Some(crate::server::network::ConnectionSender::new(tx));
        level.add_player(Box::new(player));
        level.remove_player_immediately(1, RemovalReason::Killed);
        level.process_entity_events();
        assert!(level.dead_players.contains_key(&1));

        let mut dead = level.take_entity(1).expect("parked player");
        dead.as_any_mut()
            .downcast_mut::<crate::world::entity::ServerPlayer>()
            .unwrap()
            .removed_from_player_list = true;
        level.put_back_entity(1, dead);
        assert!(level.dead_players.is_empty());
    }
}
