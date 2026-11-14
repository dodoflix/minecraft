//! `LevelDebugSynchronizers` and `TrackingDebugSynchronizer`: poll the registered debug value sources of a level every
//! tick and send the changes to the subscribed operators that track the entity, chunk or block.

use super::subscriptions;
use crate::server::level::ServerLevel;
use crate::world::block_entity::BlockEntity;
use crate::world::entity::ai::village::poi::{Occupancy, PoiRecord};
use crate::world::entity::{Entity, EntityId, ServerPlayer};
use mc_core::java_collections::{JavaHashMap, block_pos_hash};
use mc_core::pos::{BlockPos, ChunkPos, SectionPos};
use mc_core::uuid_util::java_hash_code;
use mc_data::DebugSubscription;
use mc_protocol::packet::Packet;
use mc_protocol::packets::play::{
    ClientboundDebugBlockValuePacket, ClientboundDebugChunkValuePacket,
    ClientboundDebugEntityValuePacket, DebugPoiInfo, DebugSubscriptionEvent,
    DebugSubscriptionUpdate,
};
use uuid::Uuid;

/// A value a debug source produced. `identity` stands for the parts vanilla compares by reference rather than by value
/// (`Path.debugData`): two values are the same update only when both the event and the identity match.
#[derive(Clone, Debug, PartialEq)]
pub struct DebugValue {
    pub event: DebugSubscriptionEvent,
    pub identity: usize,
}

impl DebugValue {
    /// A value compared by its content.
    pub fn new(event: DebugSubscriptionEvent) -> Self {
        Self { event, identity: 0 }
    }

    pub fn with_identity(event: DebugSubscriptionEvent, identity: usize) -> Self {
        Self { event, identity }
    }
}

/// `DebugSubscription.packUpdate(value)` / `emptyUpdate()`.
fn pack_update(
    subscription: DebugSubscription,
    value: Option<&DebugValue>,
) -> DebugSubscriptionUpdate {
    match value {
        Some(v) => DebugSubscriptionUpdate::Value(v.event.clone()),
        None => DebugSubscriptionUpdate::Empty(subscription),
    }
}

/// An entity's `ValueGetter`s run against the level and the entity they were registered for.
pub type EntityDebugGetter = fn(&ServerLevel, &dyn Entity) -> Option<DebugValue>;

/// A block entity's getter: run against the level and the position the block entity was registered at.
pub type BlockEntityDebugGetter = fn(&ServerLevel, BlockPos) -> Option<DebugValue>;

/// A chunk's getter: run against the level and the chunk it was registered for.
pub type ChunkDebugGetter = fn(&ServerLevel, ChunkPos) -> Option<DebugValue>;

/// `DebugValueSource.Registration`: what `registerDebugValues` registers.
pub struct DebugRegistration<G> {
    entries: Vec<(DebugSubscription, G)>,
}

impl<G> IntoIterator for DebugRegistration<G> {
    type Item = (DebugSubscription, G);
    type IntoIter = std::vec::IntoIter<(DebugSubscription, G)>;

    fn into_iter(self) -> Self::IntoIter {
        self.entries.into_iter()
    }
}

impl<G> Default for DebugRegistration<G> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
        }
    }
}

impl<G> DebugRegistration<G> {
    /// `register(subscription, getter)`.
    pub fn register(&mut self, subscription: DebugSubscription, getter: G) {
        self.entries.push((subscription, getter));
    }
}

/// `TrackingDebugSynchronizer.ValueSource`.
struct ValueSource<G> {
    getter: G,
    last_synced: Option<DebugValue>,
}

impl<G> ValueSource<G> {
    fn new(getter: G) -> Self {
        Self {
            getter,
            last_synced: None,
        }
    }

    /// `pollUpdate(subscription)`: a changed value (`Objects.equals`), or `None`.
    fn poll_update(
        &mut self,
        subscription: DebugSubscription,
        value: Option<DebugValue>,
    ) -> Option<DebugSubscriptionUpdate> {
        if value == self.last_synced {
            return None;
        }
        self.last_synced = value;
        Some(pack_update(subscription, self.last_synced.as_ref()))
    }
}

/// What `ChunkMap.TrackedEntity` tells the debug synchronizers (applied by the level after every tracker call).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrackerEvent {
    /// The first player started tracking the entity: `registerEntity(entity)`.
    Register(EntityId),
    /// A player started tracking the entity: `startTrackingEntity(player, entity)`.
    StartTracking { player: EntityId, entity: EntityId },
    /// The last player stopped tracking the entity, or the entity stopped being tracked: `dropEntity(entity)`.
    Drop(Uuid),
}

fn uuid_hash(uuid: &Uuid) -> i32 {
    java_hash_code(uuid)
}

fn chunk_hash(pos: &ChunkPos) -> i32 {
    pos.hash_code()
}

/// `TrackingDebugSynchronizer`'s subscriber bookkeeping (`subscribedPlayers`).
struct Subscribers {
    subscription: DebugSubscription,
    subscribed: Vec<Uuid>,
}

impl Subscribers {
    fn new(subscription: DebugSubscription) -> Self {
        Self {
            subscription,
            subscribed: Vec::new(),
        }
    }

    /// The first half of `tick(level)`: players whose subscription changed. Returns the players that just subscribed.
    fn update(&mut self, level: &ServerLevel) -> Vec<EntityId> {
        let mut added = Vec::new();
        for &id in &level.players {
            let Some(player) = player_of(level, id) else {
                continue;
            };
            let uuid = player.uuid();
            let was = self.subscribed.contains(&uuid);
            let is = player
                .debug_subscriptions(level)
                .contains(&self.subscription);
            if is != was {
                if is {
                    self.subscribed.push(uuid);
                    added.push(id);
                } else {
                    self.subscribed.retain(|u| *u != uuid);
                }
            }
        }
        self.subscribed.retain(|u| {
            level
                .players
                .iter()
                .any(|&id| level.player_record(id).is_some_and(|p| p.uuid == *u))
        });
        added
    }

    /// The ids of the subscribed players that are in the level.
    fn player_ids(&self, level: &ServerLevel) -> Vec<EntityId> {
        level
            .players
            .iter()
            .copied()
            .filter(|&id| self.contains_player(level, id))
            .collect()
    }

    /// Also true for a player that is out of the level storage while its packet or tick runs.
    fn contains_player(&self, level: &ServerLevel, id: EntityId) -> bool {
        level
            .player_record(id)
            .is_some_and(|p| self.subscribed.contains(&p.uuid))
    }

    /// `sendToPlayersTrackingChunk(level, trackedChunk, packet)`.
    fn send_to_players_tracking_chunk<P: Packet>(
        &self,
        level: &ServerLevel,
        tracked_chunk: ChunkPos,
        packet: &P,
    ) {
        for id in self.player_ids(level) {
            if level
                .chunk_source
                .chunk_map
                .is_chunk_tracked(id, tracked_chunk.x, tracked_chunk.z)
                && let Some(connection) = level.player_record(id).and_then(|p| p.connection)
            {
                connection.send(packet);
            }
        }
    }
}

fn player_of(level: &ServerLevel, id: EntityId) -> Option<&ServerPlayer> {
    level
        .get_entity(id)
        .and_then(|e| e.as_any().downcast_ref::<ServerPlayer>())
}

fn send_to_player<P: Packet>(level: &ServerLevel, player: EntityId, packet: &P) {
    if let Some(connection) = level.player_record(player).and_then(|p| p.connection) {
        connection.send(packet);
    }
}

/// `TrackingDebugSynchronizer`.
trait TrackingSynchronizer {
    fn subscribers(&self) -> &Subscribers;
    fn subscribers_mut(&mut self) -> &mut Subscribers;
    fn clear(&mut self) {}
    fn poll_and_send_updates(&mut self, _level: &ServerLevel) {}
    fn send_initial_chunk(&self, _level: &ServerLevel, _player: EntityId, _chunk: ChunkPos) {}
    fn send_initial_entity(&self, _level: &ServerLevel, _player: EntityId, _entity: EntityId) {}
}

/// `TrackingDebugSynchronizer.tick(level)`.
fn tick_synchronizer(sync: &mut dyn TrackingSynchronizer, level: &ServerLevel) {
    for player in sync.subscribers_mut().update(level) {
        add_subscriber(sync, level, player);
    }
    if !sync.subscribers().subscribed.is_empty() {
        sync.poll_and_send_updates(level);
    }
}

/// `addSubscriber(player)`: what the player tracks right now.
fn add_subscriber(sync: &dyn TrackingSynchronizer, level: &ServerLevel, player: EntityId) {
    for chunk in level
        .chunk_source
        .chunk_map
        .tracked_chunks_not_pending(player)
    {
        sync.send_initial_chunk(level, player, chunk);
    }
    for entity in level.entity_tracker.entities_tracked_by(player) {
        sync.send_initial_entity(level, player, entity);
    }
}

fn start_tracking_chunk(
    sync: &dyn TrackingSynchronizer,
    level: &ServerLevel,
    player: EntityId,
    chunk: ChunkPos,
) {
    if sync.subscribers().contains_player(level, player) {
        sync.send_initial_chunk(level, player, chunk);
    }
}

fn start_tracking_entity(
    sync: &dyn TrackingSynchronizer,
    level: &ServerLevel,
    player: EntityId,
    entity: EntityId,
) {
    if sync.subscribers().contains_player(level, player) {
        sync.send_initial_entity(level, player, entity);
    }
}

/// `TrackingDebugSynchronizer.SourceSynchronizer`.
struct SourceSynchronizer {
    subscribers: Subscribers,
    chunk_sources: JavaHashMap<ChunkPos, ValueSource<ChunkDebugGetter>>,
    block_entity_sources: JavaHashMap<BlockPos, ValueSource<BlockEntityDebugGetter>>,
    entity_sources: JavaHashMap<Uuid, ValueSource<EntityDebugGetter>>,
}

impl SourceSynchronizer {
    fn new(subscription: DebugSubscription) -> Self {
        Self {
            subscribers: Subscribers::new(subscription),
            chunk_sources: JavaHashMap::new(chunk_hash),
            block_entity_sources: JavaHashMap::new(block_pos_hash),
            entity_sources: JavaHashMap::new(uuid_hash),
        }
    }

    fn subscription(&self) -> DebugSubscription {
        self.subscribers.subscription
    }

    fn drop_chunk(&mut self, chunk: ChunkPos) {
        self.chunk_sources.remove(&chunk);
        let inside: Vec<BlockPos> = self
            .block_entity_sources
            .keys()
            .filter(|p| ChunkPos::containing(p) == chunk)
            .copied()
            .collect();
        for pos in inside {
            self.block_entity_sources.remove(&pos);
        }
    }

    /// `dropBlockEntity(level, pos)`: the clients forget the value.
    fn drop_block_entity(&mut self, level: &ServerLevel, pos: BlockPos) {
        if self.block_entity_sources.remove(&pos).is_some() {
            self.subscribers.send_to_players_tracking_chunk(
                level,
                ChunkPos::containing(&pos),
                &ClientboundDebugBlockValuePacket {
                    block_pos: pos,
                    update: pack_update(self.subscription(), None),
                },
            );
        }
    }
}

impl TrackingSynchronizer for SourceSynchronizer {
    fn subscribers(&self) -> &Subscribers {
        &self.subscribers
    }

    fn subscribers_mut(&mut self) -> &mut Subscribers {
        &mut self.subscribers
    }

    fn clear(&mut self) {
        self.chunk_sources.clear();
        self.block_entity_sources.clear();
        self.entity_sources.clear();
    }

    /// `pollAndSendUpdates(level)`: chunks, then block entities, then entities.
    fn poll_and_send_updates(&mut self, level: &ServerLevel) {
        let subscription = self.subscription();
        let chunks: Vec<ChunkPos> = self.chunk_sources.keys().copied().collect();
        for pos in chunks {
            let Some(source) = self.chunk_sources.get_mut(&pos) else {
                continue;
            };
            let value = (source.getter)(level, pos);
            if let Some(update) = source.poll_update(subscription, value) {
                self.subscribers.send_to_players_tracking_chunk(
                    level,
                    pos,
                    &ClientboundDebugChunkValuePacket {
                        chunk_pos: pos,
                        update,
                    },
                );
            }
        }
        let block_entities: Vec<BlockPos> = self.block_entity_sources.keys().copied().collect();
        for pos in block_entities {
            let Some(source) = self.block_entity_sources.get_mut(&pos) else {
                continue;
            };
            let value = (source.getter)(level, pos);
            if let Some(update) = source.poll_update(subscription, value) {
                self.subscribers.send_to_players_tracking_chunk(
                    level,
                    ChunkPos::containing(&pos),
                    &ClientboundDebugBlockValuePacket {
                        block_pos: pos,
                        update,
                    },
                );
            }
        }
        let players = self.subscribers.player_ids(level);
        let uuids: Vec<Uuid> = self.entity_sources.keys().copied().collect();
        for uuid in uuids {
            let Some(entity) = level.get_entity_by_uuid(uuid) else {
                continue;
            };
            let Some(source) = self.entity_sources.get_mut(&uuid) else {
                continue;
            };
            let value = (source.getter)(level, entity);
            if let Some(update) = source.poll_update(subscription, value) {
                let payload = crate::server::level::server_entity::encode_payload(
                    &ClientboundDebugEntityValuePacket {
                        entity_id: entity.id(),
                        update,
                    },
                );
                level.entity_tracker.send_to_tracking_players_filtered(
                    entity.id(),
                    &payload,
                    |id| players.contains(&id),
                );
            }
        }
    }

    fn send_initial_chunk(&self, level: &ServerLevel, player: EntityId, chunk: ChunkPos) {
        let subscription = self.subscription();
        if let Some(last) = self
            .chunk_sources
            .get(&chunk)
            .and_then(|s| s.last_synced.as_ref())
        {
            send_to_player(
                level,
                player,
                &ClientboundDebugChunkValuePacket {
                    chunk_pos: chunk,
                    update: pack_update(subscription, Some(last)),
                },
            );
        }
        for (pos, source) in self.block_entity_sources.iter() {
            if let Some(last) = &source.last_synced
                && ChunkPos::containing(pos) == chunk
            {
                send_to_player(
                    level,
                    player,
                    &ClientboundDebugBlockValuePacket {
                        block_pos: *pos,
                        update: pack_update(subscription, Some(last)),
                    },
                );
            }
        }
    }

    fn send_initial_entity(&self, level: &ServerLevel, player: EntityId, entity: EntityId) {
        let Some(entity_ref) = level.entity_manager.get_any(entity) else {
            return;
        };
        if let Some(last) = self
            .entity_sources
            .get(&entity_ref.uuid())
            .and_then(|s| s.last_synced.as_ref())
        {
            send_to_player(
                level,
                player,
                &ClientboundDebugEntityValuePacket {
                    entity_id: entity,
                    update: pack_update(self.subscription(), Some(last)),
                },
            );
        }
    }
}

/// `DebugPoiInfo(record)`.
fn poi_info(record: &PoiRecord) -> DebugPoiInfo {
    DebugPoiInfo {
        pos: record.get_pos(),
        poi_type: mc_data::PointOfInterestType::by_name(record.get_poi_type().registered_name())
            .expect("registered poi type"),
        free_ticket_count: record.get_free_tickets(),
    }
}

fn poi_update(record: &PoiRecord) -> DebugSubscriptionUpdate {
    DebugSubscriptionUpdate::Value(DebugSubscriptionEvent::Pois(poi_info(record)))
}

/// `TrackingDebugSynchronizer.PoiSynchronizer`.
struct PoiSynchronizer {
    subscribers: Subscribers,
}

impl PoiSynchronizer {
    fn new() -> Self {
        Self {
            subscribers: Subscribers::new(subscriptions::pois()),
        }
    }

    fn send_block_update(
        &self,
        level: &ServerLevel,
        pos: BlockPos,
        update: DebugSubscriptionUpdate,
    ) {
        self.subscribers.send_to_players_tracking_chunk(
            level,
            ChunkPos::containing(&pos),
            &ClientboundDebugBlockValuePacket {
                block_pos: pos,
                update,
            },
        );
    }

    fn on_poi_added(&self, level: &ServerLevel, record: &PoiRecord) {
        self.send_block_update(level, record.get_pos(), poi_update(record));
    }

    fn on_poi_removed(&self, level: &ServerLevel, pos: BlockPos) {
        self.send_block_update(level, pos, pack_update(self.subscribers.subscription, None));
    }

    fn on_poi_ticket_count_changed(&self, level: &ServerLevel, pos: BlockPos) {
        let update = match level.poi_manager().get_debug_poi_info(pos) {
            Some(record) => poi_update(&record),
            None => pack_update(self.subscribers.subscription, None),
        };
        self.send_block_update(level, pos, update);
    }
}

impl TrackingSynchronizer for PoiSynchronizer {
    fn subscribers(&self) -> &Subscribers {
        &self.subscribers
    }

    fn subscribers_mut(&mut self) -> &mut Subscribers {
        &mut self.subscribers
    }

    fn send_initial_chunk(&self, level: &ServerLevel, player: EntityId, chunk: ChunkPos) {
        for record in level
            .poi_manager()
            .get_in_chunk(&|_| true, chunk, Occupancy::Any)
        {
            send_to_player(
                level,
                player,
                &ClientboundDebugBlockValuePacket {
                    block_pos: record.get_pos(),
                    update: poi_update(&record),
                },
            );
        }
    }
}

/// `TrackingDebugSynchronizer.VillageSectionSynchronizer`.
struct VillageSectionSynchronizer {
    subscribers: Subscribers,
}

/// `forEachVillageSectionUpdate(level, centerSection, consumer)`.
fn for_each_village_section_update(
    level: &ServerLevel,
    center_section: SectionPos,
    mut consumer: impl FnMut(SectionPos, bool),
) {
    for offset_z in -1..=1 {
        for offset_x in -1..=1 {
            for offset_y in -1..=1 {
                let section = center_section.offset(offset_x, offset_y, offset_z);
                consumer(section, level.is_village(section.center()));
            }
        }
    }
}

impl VillageSectionSynchronizer {
    fn new() -> Self {
        Self {
            subscribers: Subscribers::new(subscriptions::village_sections()),
        }
    }

    fn update(&self, is_village: bool) -> DebugSubscriptionUpdate {
        if is_village {
            DebugSubscriptionUpdate::Value(DebugSubscriptionEvent::VillageSections(()))
        } else {
            pack_update(self.subscribers.subscription, None)
        }
    }

    fn send_village_sections_packet(&self, level: &ServerLevel, poi_pos: BlockPos) {
        for_each_village_section_update(
            level,
            SectionPos::of_block_pos(&poi_pos),
            |section, is_village| {
                let pos = section.center();
                self.subscribers.send_to_players_tracking_chunk(
                    level,
                    ChunkPos::containing(&pos),
                    &ClientboundDebugBlockValuePacket {
                        block_pos: pos,
                        update: self.update(is_village),
                    },
                );
            },
        );
    }
}

impl TrackingSynchronizer for VillageSectionSynchronizer {
    fn subscribers(&self) -> &Subscribers {
        &self.subscribers
    }

    fn subscribers_mut(&mut self) -> &mut Subscribers {
        &mut self.subscribers
    }

    fn send_initial_chunk(&self, level: &ServerLevel, player: EntityId, chunk: ChunkPos) {
        for record in level
            .poi_manager()
            .get_in_chunk(&|_| true, chunk, Occupancy::Any)
        {
            for_each_village_section_update(
                level,
                SectionPos::of_block_pos(&record.get_pos()),
                |section, is_village| {
                    send_to_player(
                        level,
                        player,
                        &ClientboundDebugBlockValuePacket {
                            block_pos: section.center(),
                            update: self.update(is_village),
                        },
                    );
                },
            );
        }
    }
}

/// `LevelDebugSynchronizers`.
pub struct LevelDebugSynchronizers {
    sleeping: bool,
    enabled: Vec<DebugSubscription>,
    /// One per subscription that has a value codec, in registry order (vanilla keeps them in a `HashMap` keyed by identity,
    /// so no order is reproducible).
    sources: Vec<SourceSynchronizer>,
    poi: PoiSynchronizer,
    village_sections: VillageSectionSynchronizer,
}

impl Default for LevelDebugSynchronizers {
    fn default() -> Self {
        Self::new()
    }
}

impl LevelDebugSynchronizers {
    pub fn new() -> Self {
        Self {
            sleeping: true,
            enabled: Vec::new(),
            sources: DebugSubscription::iter()
                .filter(|s| s.id() != 0)
                .map(SourceSynchronizer::new)
                .collect(),
            poi: PoiSynchronizer::new(),
            village_sections: VillageSectionSynchronizer::new(),
        }
    }

    fn source_mut(&mut self, subscription: DebugSubscription) -> Option<&mut SourceSynchronizer> {
        self.sources
            .iter_mut()
            .find(|s| s.subscription() == subscription)
    }

    fn all(&mut self) -> impl Iterator<Item = &mut dyn TrackingSynchronizer> {
        self.sources
            .iter_mut()
            .map(|s| s as &mut dyn TrackingSynchronizer)
            .chain([
                &mut self.poi as &mut dyn TrackingSynchronizer,
                &mut self.village_sections as &mut dyn TrackingSynchronizer,
            ])
    }

    fn all_ref(&self) -> impl Iterator<Item = &dyn TrackingSynchronizer> {
        self.sources
            .iter()
            .map(|s| s as &dyn TrackingSynchronizer)
            .chain([
                &self.poi as &dyn TrackingSynchronizer,
                &self.village_sections as &dyn TrackingSynchronizer,
            ])
    }

    /// `tick(serverSubscribers)`.
    pub fn tick(&mut self, level: &ServerLevel) {
        self.enabled = level.server_state.debug_subscribers.enabled();
        let should_sleep = self.enabled.is_empty();
        if self.sleeping != should_sleep {
            self.sleeping = should_sleep;
            if should_sleep {
                for synchronizer in self.all() {
                    synchronizer.clear();
                }
            } else {
                self.wake_up(level);
            }
        }
        if !self.sleeping {
            for synchronizer in self.all() {
                tick_synchronizer(synchronizer, level);
            }
        }
    }

    /// `wakeUp()`: registers what is already loaded and tracked.
    fn wake_up(&mut self, level: &ServerLevel) {
        for key in level.chunk_source.chunk_map.ready_to_send_chunks() {
            self.register_chunk(level, ChunkPos::unpack(key));
        }
        for entity in level.entity_manager.get_all() {
            if level.entity_tracker.is_tracked_by_any_player(entity.id()) {
                self.register_entity(entity);
            }
        }
    }

    /// `registerChunk(chunk)`: the chunk's values and its block entities'.
    pub fn register_chunk(&mut self, level: &ServerLevel, pos: ChunkPos) {
        if self.sleeping {
            return;
        }
        let Some(chunk) = level.chunk_source.get_chunk_now(pos.x, pos.z) else {
            return;
        };
        let mut registration = DebugRegistration::default();
        super::chunk_values::register_chunk_debug_values(chunk, &mut registration);
        for (subscription, getter) in registration {
            if let Some(source) = self.source_mut(subscription) {
                source.chunk_sources.insert(pos, ValueSource::new(getter));
            }
        }
        let block_entities: Vec<BlockPos> = chunk.block_entities.keys().copied().collect();
        for block_pos in block_entities {
            self.register_block_entity(level, block_pos);
        }
    }

    /// `dropChunk(chunkPos)`.
    pub fn drop_chunk(&mut self, pos: ChunkPos) {
        if self.sleeping {
            return;
        }
        for source in &mut self.sources {
            source.drop_chunk(pos);
        }
    }

    /// `registerBlockEntity(blockEntity)`.
    pub fn register_block_entity(&mut self, level: &ServerLevel, pos: BlockPos) {
        if self.sleeping {
            return;
        }
        let Some(block_entity) = level.block_entity_ref(pos) else {
            return;
        };
        let mut registration = DebugRegistration::default();
        BlockEntity::register_debug_values(block_entity, &mut registration);
        for (subscription, getter) in registration {
            if let Some(source) = self.source_mut(subscription) {
                source
                    .block_entity_sources
                    .insert(pos, ValueSource::new(getter));
            }
        }
    }

    /// `dropBlockEntity(pos)`.
    pub fn drop_block_entity(&mut self, level: &ServerLevel, pos: BlockPos) {
        if self.sleeping {
            return;
        }
        for source in &mut self.sources {
            source.drop_block_entity(level, pos);
        }
    }

    /// `registerEntity(entity)`.
    pub fn register_entity(&mut self, entity: &dyn Entity) {
        if self.sleeping {
            return;
        }
        let mut registration = DebugRegistration::default();
        entity.register_debug_values(&mut registration);
        for (subscription, getter) in registration {
            if let Some(source) = self.source_mut(subscription) {
                source
                    .entity_sources
                    .insert(entity.uuid(), ValueSource::new(getter));
            }
        }
    }

    /// `dropEntity(entity)`.
    pub fn drop_entity(&mut self, uuid: Uuid) {
        if self.sleeping {
            return;
        }
        for source in &mut self.sources {
            source.entity_sources.remove(&uuid);
        }
    }

    /// `startTrackingChunk(player, chunkPos)`.
    pub fn start_tracking_chunk(&self, level: &ServerLevel, player: EntityId, chunk: ChunkPos) {
        if !self.sleeping {
            for synchronizer in self.all_ref() {
                start_tracking_chunk(synchronizer, level, player, chunk);
            }
        }
    }

    /// `startTrackingEntity(player, entity)`.
    pub fn start_tracking_entity(&self, level: &ServerLevel, player: EntityId, entity: EntityId) {
        if !self.sleeping {
            for synchronizer in self.all_ref() {
                start_tracking_entity(synchronizer, level, player, entity);
            }
        }
    }

    /// Applies what the entity tracker recorded since the last call.
    pub fn apply_tracker_events(&mut self, level: &ServerLevel, events: Vec<TrackerEvent>) {
        for event in events {
            match event {
                TrackerEvent::Register(id) => {
                    if let Some(entity) = level.entity_manager.get_any(id) {
                        self.register_entity(entity);
                    }
                }
                TrackerEvent::StartTracking { player, entity } => {
                    self.start_tracking_entity(level, player, entity)
                }
                TrackerEvent::Drop(uuid) => self.drop_entity(uuid),
            }
        }
    }

    /// `registerPoi(record)`.
    pub fn register_poi(&self, level: &ServerLevel, record: &PoiRecord) {
        if !self.sleeping {
            self.poi.on_poi_added(level, record);
            self.village_sections
                .send_village_sections_packet(level, record.get_pos());
        }
    }

    /// `updatePoi(pos)`.
    pub fn update_poi(&self, level: &ServerLevel, pos: BlockPos) {
        if !self.sleeping {
            self.poi.on_poi_ticket_count_changed(level, pos);
        }
    }

    /// `dropPoi(pos)`.
    pub fn drop_poi(&self, level: &ServerLevel, pos: BlockPos) {
        if !self.sleeping {
            self.poi.on_poi_removed(level, pos);
            self.village_sections
                .send_village_sections_packet(level, pos);
        }
    }

    /// `hasAnySubscriberFor(subscription)`.
    pub fn has_any_subscriber_for(&self, subscription: DebugSubscription) -> bool {
        self.enabled.contains(&subscription)
    }
}
