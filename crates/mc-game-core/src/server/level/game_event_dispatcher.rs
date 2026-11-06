//! `GameEventDispatcher`: delivers a game event to every listener registered in the chunk sections around it whose own
//! radius covers the event position.

use super::ServerLevel;
use crate::world::entity::{Entity, EntityId};
use crate::world::level::gameevent::vibrations::capture_source_entity;
use crate::world::level::gameevent::{
    DeliveryMode, GameEventContext, ListenerOwner, RegisteredListener,
};
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_data::GameEvent;
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

    /// `gameEvent(sourceEntity, event, pos)`.
    pub fn game_event_from_entity(
        &mut self,
        source: Option<EntityId>,
        event: GameEvent,
        position: Vec3,
    ) {
        self.post_game_event(event, position, &GameEventContext::of_entity(source));
    }

    /// `gameEvent(sourceEntity, event, pos)` with the source entity at hand: its facts for the vibration system are
    /// captured (the entity may be out of the level's slot, e.g. while it ticks).
    pub fn game_event_from_entity_ref(
        &mut self,
        entity: &dyn Entity,
        event: GameEvent,
        position: Vec3,
    ) {
        let context = GameEventContext::of_captured(capture_source_entity(self, entity));
        self.post_game_event(event, position, &context);
    }

    /// Like [`game_event_from_entity_ref`](Self::game_event_from_entity_ref) for listeners that act on the source entity
    /// itself (`ENTITY_DIE` listened to by sculk catalysts): the listeners get the entity.
    pub fn game_event_from_live_entity(
        &mut self,
        entity: &mut dyn Entity,
        event: GameEvent,
        position: Vec3,
    ) {
        let context = GameEventContext::of_captured(capture_source_entity(self, entity));
        self.post_game_event_with(event, position, &context, Some(entity));
    }

    /// `GameEventDispatcher.post(event, position, context)`.
    pub fn post_game_event(
        &mut self,
        event: GameEvent,
        position: Vec3,
        context: &GameEventContext,
    ) {
        self.post_game_event_with(event, position, context, None);
    }

    /// `post` with the live source entity (see [`game_event_from_live_entity`](Self::game_event_from_live_entity)).
    pub fn post_game_event_with(
        &mut self,
        event: GameEvent,
        position: Vec3,
        context: &GameEventContext,
        mut live: Option<&mut dyn Entity>,
    ) {
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
                            self.deliver_game_event(
                                listener.owner,
                                event,
                                context,
                                position,
                                live.as_deref_mut(),
                            );
                        }
                    }
                }
            }
        }
        // `Collections.sort` is stable: equal distances keep visiting order.
        to_handle_by_distance.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (_, listener) in to_handle_by_distance {
            self.deliver_game_event(
                listener.owner,
                event,
                context,
                position,
                live.as_deref_mut(),
            );
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
        live: Option<&mut dyn Entity>,
    ) {
        match owner {
            ListenerOwner::BlockEntity(pos) => {
                let handler = self
                    .get_block_entity_immediate(pos)
                    .and_then(|block_entity| block_entity.game_event_listener())
                    .map(|listener| listener.handle);
                if let Some(handle) = handler {
                    handle(self, pos, event, context, position, live);
                }
            }
            ListenerOwner::Entity(id) => {
                self.with_entity(id, |level, entity| {
                    if let Some(listener) = entity.game_event_listener() {
                        listener.handle_game_event(level, event, context, position);
                    }
                });
            }
        }
    }
}
