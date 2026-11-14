//! `net.minecraft.world.entity.animal.nautilus.ZombieNautilusAi`: the brain of the zombie nautilus (the nautilus brain
//! without panic and breeding, a slower dash).

use super::nautilus_ai::{fight_activity, start_attacking};
use mc_data::SoundEvent;
use mc_game_ai::world::entity::ai::behavior::behaviors::{CountDownCooldownTicks, boxed};
use mc_game_ai::world::entity::ai::behavior::{follow_temptation::FollowTemptation, random_stroll};
use mc_game_core::world::entity::ai::behavior::look_at_target_sink::LookAtTargetSink;
use mc_game_core::world::entity::ai::behavior::{
    BehaviorControl, GateBehavior, OrderPolicy, RunningPolicy,
    move_to_target_sink::MoveToTargetSink, set_walk_target_from_look_target,
};
use mc_game_core::world::entity::ai::brain::ActivityData;
use mc_game_core::world::entity::ai::memory::MemoryStatus;
use mc_game_core::world::entity::ai::memory::memory_module_type::{
    ATTACK_TARGET_COOLDOWN, CHARGE_COOLDOWN_TICKS, TEMPTATION_COOLDOWN_TICKS, WALK_TARGET,
};
use mc_game_core::world::entity::entity::Entity;
use mc_game_core::world::entity::schedule::Activity;

const SPEED_MULTIPLIER_WHEN_IDLING_IN_WATER: f32 = 1.0;
const SPEED_MULTIPLIER_WHEN_TEMPTED: f32 = 0.9;
const SPEED_WHEN_ATTACKING: f32 = 0.5;

/// `ZombieNautilusAi.getActivities()`.
pub fn get_activities(_body: &dyn Entity) -> Vec<ActivityData> {
    vec![
        init_core_activity(),
        init_idle_activity(),
        fight_activity(
            SPEED_WHEN_ATTACKING,
            SoundEvent::ENTITY_ZOMBIE_NAUTILUS_DASH,
        ),
    ]
}

/// `initCoreActivity()`.
fn init_core_activity() -> ActivityData {
    ActivityData::create(
        Activity::CORE,
        0,
        vec![
            boxed(LookAtTargetSink::new(45, 90)),
            boxed(MoveToTargetSink::new()),
            boxed(CountDownCooldownTicks::new(TEMPTATION_COOLDOWN_TICKS)),
            boxed(CountDownCooldownTicks::new(CHARGE_COOLDOWN_TICKS)),
            boxed(CountDownCooldownTicks::new(ATTACK_TARGET_COOLDOWN)),
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
            (
                boxed(random_stroll::swim(SPEED_MULTIPLIER_WHEN_IDLING_IN_WATER)),
                2,
            ),
            (
                boxed(set_walk_target_from_look_target::create(
                    SPEED_MULTIPLIER_WHEN_IDLING_IN_WATER,
                    3,
                )),
                3,
            ),
        ],
    ));
    ActivityData::create_pairs(
        Activity::IDLE,
        vec![
            (
                1,
                boxed(FollowTemptation::with_distance(
                    |_| SPEED_MULTIPLIER_WHEN_TEMPTED,
                    |mob| if mob.is_baby() { 2.5 } else { 3.5 },
                )),
            ),
            (2, boxed(start_attacking())),
            (3, gate),
        ],
    )
}
