//! `GoAndGiveItemsToTarget`: walk up to a target position and throw items at it.

use mc_core::phys::Vec3;
use mc_game_core::entry_condition;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::Entity;
use mc_game_core::world::entity::ai::behavior::behavior_utils::set_walk_and_look_target_memories;
use mc_game_core::world::entity::ai::behavior::position_tracker::PositionTracker;
use mc_game_core::world::entity::ai::behavior::{Behavior, BehaviorBase};
use mc_game_core::world::entity::ai::memory::MemoryModuleType;
use mc_game_core::world::entity::ai::memory::memory_module_type::{LOOK_TARGET, WALK_TARGET};
use std::sync::Arc;

const CLOSE_ENOUGH_DISTANCE_TO_TARGET: f64 = 3.0;

/// `Function<LivingEntity, Optional<PositionTracker>>`.
pub type TargetPositionGetter =
    Arc<dyn Fn(&ServerLevel, &dyn Entity) -> Option<PositionTracker> + Send + Sync>;
/// `ItemThrower.throwItem(level, thrower, targetPos)`.
pub type ItemThrower = Arc<dyn Fn(&mut ServerLevel, &mut dyn Entity, Vec3) + Send + Sync>;

pub struct GoAndGiveItemsToTarget {
    target_position_getter: TargetPositionGetter,
    speed_modifier: f32,
    item_thrower: ItemThrower,
    cooldown_memory: MemoryModuleType<i32>,
    cooldown_duration: i32,
    has_item_predicate: Arc<dyn Fn(&dyn Entity) -> bool + Send + Sync>,
}

impl GoAndGiveItemsToTarget {
    pub fn new(
        target_position_getter: TargetPositionGetter,
        speed_modifier: f32,
        timeout_duration: i32,
        item_thrower: ItemThrower,
        cooldown_memory: MemoryModuleType<i32>,
        cooldown_duration: i32,
        has_item_predicate: Arc<dyn Fn(&dyn Entity) -> bool + Send + Sync>,
    ) -> BehaviorBase<GoAndGiveItemsToTarget> {
        BehaviorBase::with_timeout(
            entry_condition![LOOK_TARGET => Registered, WALK_TARGET => Registered, cooldown_memory => Registered],
            timeout_duration,
            GoAndGiveItemsToTarget {
                target_position_getter,
                speed_modifier,
                item_thrower,
                cooldown_memory,
                cooldown_duration,
                has_item_predicate,
            },
        )
    }

    fn can_throw_item_to_target(&self, level: &ServerLevel, body: &dyn Entity) -> bool {
        (self.has_item_predicate)(body) && (self.target_position_getter)(level, body).is_some()
    }
}

impl Behavior for GoAndGiveItemsToTarget {
    fn check_extra_start_conditions(
        &mut self,
        level: &mut ServerLevel,
        body: &mut dyn Entity,
    ) -> bool {
        self.can_throw_item_to_target(level, body)
    }

    fn can_still_use(
        &mut self,
        level: &mut ServerLevel,
        body: &mut dyn Entity,
        _timestamp: i64,
    ) -> bool {
        self.can_throw_item_to_target(level, body)
    }

    fn start(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        if let Some(position_tracker) = (self.target_position_getter)(level, body) {
            set_walk_and_look_target_memories(body, position_tracker, self.speed_modifier, 3);
        }
    }

    fn tick(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        if let Some(deposit_target) = (self.target_position_getter)(level, body) {
            let deposit_position = deposit_target.current_position(level);
            let distance_to_target = deposit_position.distance_to(body.base().get_eye_position());
            if distance_to_target < CLOSE_ENOUGH_DISTANCE_TO_TARGET {
                (self.item_thrower)(level, body, deposit_target.current_position(level));
                body.brain_mut()
                    .set_memory(&self.cooldown_memory, self.cooldown_duration);
            }
        }
    }
}
