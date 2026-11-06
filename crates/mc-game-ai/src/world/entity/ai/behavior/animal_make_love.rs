//! `AnimalMakeLove`.

use mc_core::random::RandomSource;
use mc_data::EntityType;
use mc_game_core::entry_condition;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::EntityId;
use mc_game_core::world::entity::ai::behavior::behavior_utils;
use mc_game_core::world::entity::ai::behavior::{Behavior, BehaviorBase};
use mc_game_core::world::entity::ai::memory::EntityRef;
use mc_game_core::world::entity::ai::memory::memory_module_type::{
    BREED_TARGET, IS_PANICKING, LOOK_TARGET, NEAREST_VISIBLE_LIVING_ENTITIES, WALK_TARGET,
};
use mc_game_core::world::entity::ai::sensing::closer_than;
use mc_game_core::world::entity::entity::Entity;
use mc_game_core::world::entity::mob::animal;

/// `BREED_RANGE`.
const BREED_RANGE: f64 = 3.0;
const DEFAULT_CLOSE_ENOUGH_DISTANCE: i32 = 2;

pub struct AnimalMakeLove {
    partner_type: EntityType,
    speed_modifier: f32,
    close_enough_distance: i32,
    spawn_child_at_time: i64,
}

impl AnimalMakeLove {
    /// `new AnimalMakeLove(partnerType)`.
    pub fn new(partner_type: EntityType) -> BehaviorBase<AnimalMakeLove> {
        Self::with(partner_type, 1.0, DEFAULT_CLOSE_ENOUGH_DISTANCE)
    }

    /// `new AnimalMakeLove(partnerType, speedModifier, closeEnoughDistance)`.
    pub fn with(
        partner_type: EntityType,
        speed_modifier: f32,
        close_enough_distance: i32,
    ) -> BehaviorBase<AnimalMakeLove> {
        BehaviorBase::with_timeout(
            entry_condition![
                NEAREST_VISIBLE_LIVING_ENTITIES => ValuePresent,
                BREED_TARGET => ValueAbsent,
                WALK_TARGET => Registered,
                LOOK_TARGET => Registered,
                IS_PANICKING => ValueAbsent,
            ],
            110,
            AnimalMakeLove {
                partner_type,
                speed_modifier,
                close_enough_distance,
                spawn_child_at_time: 0,
            },
        )
    }

    fn find_valid_breed_partner(&self, level: &ServerLevel, body: &dyn Entity) -> Option<EntityId> {
        let visible = body.brain().get_memory(&NEAREST_VISIBLE_LIVING_ENTITIES)?;
        visible
            .find_closest(level, body, |entity| {
                entity.get_type() == self.partner_type
                    && entity.mob().is_some_and(|m| m.animal.is_some())
                    && animal::can_mate(body, entity)
                    && !entity.is_panicking()
            })
            .map(|e| e.0)
    }

    fn breed_target(&self, level: &ServerLevel, body: &dyn Entity) -> Option<EntityId> {
        let EntityRef(id) = *body.brain().get_memory(&BREED_TARGET)?;
        level
            .get_entity(id)
            .filter(|t| t.get_type() == self.partner_type)
            .map(|_| id)
    }
}

impl Behavior for AnimalMakeLove {
    fn check_extra_start_conditions(
        &mut self,
        level: &mut ServerLevel,
        body: &mut dyn Entity,
    ) -> bool {
        animal::is_in_love(body) && self.find_valid_breed_partner(level, body).is_some()
    }

    fn start(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, timestamp: i64) {
        let partner = self
            .find_valid_breed_partner(level, body)
            .expect("partner found by the start condition");
        body.brain_mut()
            .set_memory(&BREED_TARGET, EntityRef(partner));
        let (speed, close) = (self.speed_modifier, self.close_enough_distance);
        level.with_entity(partner, |_, partner| {
            partner
                .brain_mut()
                .set_memory(&BREED_TARGET, EntityRef(body.id()));
            behavior_utils::lock_gaze_and_walk_to_each_other(body, partner, speed, close);
        });
        let duration = 60 + body.base_mut().random.next_int_bounded(50);
        self.spawn_child_at_time = timestamp + i64::from(duration);
    }

    fn can_still_use(
        &mut self,
        level: &mut ServerLevel,
        body: &mut dyn Entity,
        timestamp: i64,
    ) -> bool {
        let Some(partner_id) = self.breed_target(level, body) else {
            return false;
        };
        let Some(partner) = level.get_entity(partner_id) else {
            return false;
        };
        partner.is_alive()
            && animal::can_mate(body, partner)
            && behavior_utils::entity_is_visible(level, body, partner_id)
            && timestamp <= self.spawn_child_at_time
            && !body.is_panicking()
            && !partner.is_panicking()
    }

    fn tick(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, timestamp: i64) {
        let Some(&EntityRef(partner_id)) = body.brain().get_memory(&BREED_TARGET) else {
            return;
        };
        let (speed, close) = (self.speed_modifier, self.close_enough_distance);
        let closer = level
            .with_entity(partner_id, |_, partner| {
                behavior_utils::lock_gaze_and_walk_to_each_other(body, partner, speed, close);
                closer_than(body, partner, BREED_RANGE)
            })
            .unwrap_or(false);
        if closer && timestamp >= self.spawn_child_at_time {
            animal::spawn_child_from_breeding(body, level, partner_id);
            body.brain_mut().erase_memory(&BREED_TARGET);
            level.with_entity(partner_id, |_, partner| {
                partner.brain_mut().erase_memory(&BREED_TARGET)
            });
        }
    }

    fn stop(&mut self, _level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        body.brain_mut().erase_memory(&BREED_TARGET);
        body.brain_mut().erase_memory(&WALK_TARGET);
        body.brain_mut().erase_memory(&LOOK_TARGET);
        self.spawn_child_at_time = 0;
    }
}
