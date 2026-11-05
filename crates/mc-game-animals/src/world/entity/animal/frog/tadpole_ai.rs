//! `net.minecraft.world.entity.animal.frog.TadpoleAi`: the brain of the tadpole.

use mc_game_ai::world::entity::ai::behavior::behaviors::{CountDownCooldownTicks, boxed};
use mc_game_core::world::entity::ai::behavior::look_at_target_sink::LookAtTargetSink;
use mc_game_ai::world::entity::ai::behavior::{animal_panic::AnimalPanic, follow_temptation::FollowTemptation, random_stroll, set_entity_look_target_sometimes};
use mc_game_core::world::entity::ai::behavior::{BehaviorBuilder, BehaviorControl, GateBehavior, OrderPolicy, RunningPolicy, move_to_target_sink::MoveToTargetSink, set_walk_target_from_look_target};
use mc_game_core::world::entity::ai::brain::ActivityData;
use mc_game_core::world::entity::ai::memory::memory_module_type::{TEMPTATION_COOLDOWN_TICKS, WALK_TARGET};
use mc_game_core::world::entity::ai::memory::MemoryStatus;
use mc_game_core::world::entity::entity::Entity;
use mc_game_core::world::entity::schedule::Activity;
use mc_data::EntityType;

/// `TadpoleAi.getActivities()`.
pub fn get_activities(_body: &dyn Entity) -> Vec<ActivityData> {
    vec![init_core_activity(), init_idle_activity()]
}

/// `initCoreActivity()`.
fn init_core_activity() -> ActivityData {
    ActivityData::create(
        Activity::CORE,
        0,
        vec![
            boxed(AnimalPanic::new(2.0)),
            boxed(LookAtTargetSink::new(45, 90)),
            boxed(MoveToTargetSink::new()),
            boxed(CountDownCooldownTicks::new(TEMPTATION_COOLDOWN_TICKS)),
        ],
    )
}

/// `initIdleActivity()`.
fn init_idle_activity() -> ActivityData {
    let gate: Box<dyn BehaviorControl> = boxed(GateBehavior::new(
        vec![(WALK_TARGET.id(), MemoryStatus::ValueAbsent)],
        vec![],
        OrderPolicy::Ordered,
        RunningPolicy::TryAll,
        vec![
            (boxed(random_stroll::swim(0.5)), 2),
            (boxed(set_walk_target_from_look_target::create(0.5, 3)), 3),
            (boxed(BehaviorBuilder::trigger_if_alone(|e| e.is_in_water())), 5),
        ],
    ));
    ActivityData::create_pairs(
        Activity::IDLE,
        vec![
            (0, boxed(set_entity_look_target_sometimes::create_type(EntityType::PLAYER, 6.0, 30, 60))),
            (1, boxed(FollowTemptation::new(|_| 1.25))),
            (2, gate),
        ],
    )
}

/// `TadpoleAi.updateActivity(body)`.
pub fn update_activity(body: &mut dyn Entity) {
    body.brain_mut().set_active_activity_to_first_valid(&[Activity::IDLE]);
}
