//! `GameEventDispatcher`: delivers a game event to every listener registered in the chunk sections around it whose own
//! radius covers the event position.

use super::ServerLevel;
use crate::world::entity::{Entity, EntityId};
use crate::world::level::gameevent::vibrations::source_entity_info;
use crate::world::level::gameevent::{
    DeliveryMode, GameEventContext, ListenerOwner, RegisteredListener,
};
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_data::GameEvent;
use mc_data::block::BlockState;
use uuid::Uuid;

impl ServerLevel {
    /// `gameEvent(event, pos, context)` at the centre of a block.
    pub fn game_event_at_block(
        &mut self,
        event: GameEvent,
        pos: BlockPos,
        context: &GameEventContext,
    ) {
        self.post_game_event(event, Vec3::at_center_of(&pos), context);
    }

    /// `gameEvent(sourceEntity, event, pos)` for a source that is visible to the level (stored). The entity whose code
    /// is running goes through [`game_event_from_entity_ref`](Self::game_event_from_entity_ref).
    pub fn game_event_from_entity(
        &mut self,
        source: Option<EntityId>,
        event: GameEvent,
        position: Vec3,
    ) {
        self.post_game_event(event, position, &GameEventContext::of_entity(source));
    }

    /// `gameEvent(sourceEntity, event, pos)` with the source entity at hand (the entity whose code is running is out
    /// of the level's slot): it is exposed to the listeners for the call.
    pub fn game_event_from_entity_ref(
        &mut self,
        entity: &dyn Entity,
        event: GameEvent,
        position: Vec3,
    ) {
        self.game_event_from_entity_ref_in_state(entity, None, event, position);
    }

    /// `gameEvent(event, pos, Context.of(entity, state))` with the source entity at hand.
    pub fn game_event_from_entity_ref_in_state(
        &mut self,
        entity: &dyn Entity,
        state: Option<BlockState>,
        event: GameEvent,
        position: Vec3,
    ) {
        let mut context = GameEventContext::of(Some(entity.id()), state);
        self.expose(entity, |level| {
            // a removed or not yet added entity is invisible to the level, vanilla's context still holds the object
            if level.get_entity(entity.id()).is_none() {
                context.source_info = Some(source_entity_info(level, entity));
            }
            level.post_game_event(event, position, &context);
        });
    }

    /// `gameEvent(source, event, pos)` for a source its running code lent to the level (`ServerLevel::lend`), also when it
    /// removed itself meanwhile (vanilla's context still holds the object).
    pub fn game_event_from_lent_entity(
        &mut self,
        source: Option<EntityId>,
        event: GameEvent,
        position: Vec3,
    ) {
        let mut context = GameEventContext::of_entity(source);
        if let Some(id) = source
            && self.get_entity(id).is_none()
            && let Some(entity) = self.get_entity_or_lent(id)
        {
            context.source_info = Some(source_entity_info(self, entity));
        }
        self.post_game_event(event, position, &context);
    }

    /// [`game_event_from_entity_ref`](Self::game_event_from_entity_ref) for the listeners that act on the source entity
    /// itself (`ENTITY_DIE` listened to by sculk catalysts): it is lent, so they re-enter it with `with_entity`.
    pub fn game_event_from_live_entity(
        &mut self,
        entity: &mut dyn Entity,
        event: GameEvent,
        position: Vec3,
    ) {
        self.game_event_from_live_entity_in_state(entity, None, event, position);
    }

    /// [`game_event_from_live_entity`](Self::game_event_from_live_entity) with `Context.of(entity, state)`: the movement
    /// events of the running entity (step, swim, flap, hit ground), so a listener that acts on it (the avoid vibration
    /// trigger of a sneaking player) re-enters it.
    pub fn game_event_from_live_entity_in_state(
        &mut self,
        entity: &mut dyn Entity,
        state: Option<BlockState>,
        event: GameEvent,
        position: Vec3,
    ) {
        let mut context = GameEventContext::of(Some(entity.id()), state);
        // a removed or not yet added entity stays invisible to the level, vanilla's context still holds the object
        if entity.is_removed() || !self.entity_manager.has_entity_with_id(entity.id()) {
            context.source_info = Some(source_entity_info(self, &*entity));
        }
        self.lend(entity, |level| {
            level.post_game_event(event, position, &context);
        });
    }

    /// `GameEventDispatcher.post(event, position, context)`.
    pub fn post_game_event(
        &mut self,
        event: GameEvent,
        position: Vec3,
        context: &GameEventContext,
    ) {
        self.note_unlent_event_source(event, context);
        let radius = event.data().notification_radius;
        let center = BlockPos::containing_position(&position);
        let section_min_x = (center.x() - radius) >> 4;
        let section_min_y = (center.y() - radius) >> 4;
        let section_min_z = (center.z() - radius) >> 4;
        let section_max_x = (center.x() + radius) >> 4;
        let section_max_y = (center.y() + radius) >> 4;
        let section_max_z = (center.z() + radius) >> 4;
        // `ListenerInfo`s of BY_DISTANCE listeners: (squared distance, listener).
        let mut to_handle_by_distance: Vec<(f64, RegisteredListener)> = Vec::new();
        for chunk_x in section_min_x..=section_max_x {
            for chunk_z in section_min_z..=section_max_z {
                for section in section_min_y..=section_max_y {
                    let candidates = {
                        let Some(chunk) = self.chunk_now(chunk_x, chunk_z) else {
                            break;
                        };
                        let Some(registry) = chunk.game_event_registries.get(&section) else {
                            continue;
                        };
                        let entity_position =
                            |uuid: Uuid| self.get_entity_by_uuid(uuid).map(|e| e.position());
                        registry.in_range_listeners(position, &entity_position)
                    };
                    for (listener, listener_position) in candidates {
                        if listener.delivery == DeliveryMode::ByDistance {
                            to_handle_by_distance
                                .push((position.distance_to_sqr(listener_position), listener));
                        } else if self.listener_still_registered(
                            chunk_x,
                            chunk_z,
                            section,
                            listener.owner,
                        ) {
                            self.deliver_game_event(listener.owner, event, context, position);
                        }
                    }
                }
            }
        }
        // `Collections.sort` is stable: equal distances keep visiting order.
        to_handle_by_distance.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (_, listener) in to_handle_by_distance {
            self.deliver_game_event(listener.owner, event, context, position);
        }
    }

    fn listener_still_registered(
        &self,
        chunk_x: i32,
        chunk_z: i32,
        section: i32,
        owner: ListenerOwner,
    ) -> bool {
        self.chunk_now(chunk_x, chunk_z)
            .and_then(|c| c.game_event_registries.get(&section))
            .is_some_and(|r| r.contains(owner))
    }

    /// Resolve the owner of a listener and call `handleGameEvent` (entities are taken out of their slot meanwhile, block
    /// entities stay in their chunk).
    fn deliver_game_event(
        &mut self,
        owner: ListenerOwner,
        event: GameEvent,
        context: &GameEventContext,
        position: Vec3,
    ) {
        match owner {
            ListenerOwner::BlockEntity(pos) => {
                let handler = self
                    .get_block_entity_immediate(pos)
                    .and_then(|block_entity| block_entity.game_event_listener())
                    .map(|listener| listener.handle);
                if let Some(handle) = handler {
                    handle(self, pos, event, context, position);
                }
            }
            ListenerOwner::Entity(id) => {
                self.with_entity(id, |level, entity| {
                    if let Some(listener) = entity.game_event_listener() {
                        listener.handle_game_event(level, event, context, position);
                    }
                });
            }
            ListenerOwner::EntitySecondary(id) => {
                self.with_entity(id, |level, entity| {
                    entity.handle_secondary_game_event(level, event, context, position);
                });
            }
        }
    }
}
