//! `net.minecraft.world.entity.animal.happyghast.HappyGhastAi`: the brain of a ghastling (an adult runs on goals).

use mc_game_ai::world::entity::ai::behavior::animal_panic::AnimalPanic;
use mc_game_ai::world::entity::ai::behavior::behaviors::{CountDownCooldownTicks, boxed};
use mc_game_ai::world::entity::ai::behavior::{
    baby_follow_adult, follow_temptation::FollowTemptation, random_stroll,
};
use mc_game_core::world::entity::ai::behavior::look_at_target_sink::LookAtTargetSink;
use mc_game_core::world::entity::ai::behavior::move_to_target_sink::MoveToTargetSink;
use mc_game_core::world::entity::ai::behavior::swim::Swim;
use mc_game_core::world::entity::ai::behavior::{
    BehaviorControl, RunOne, set_walk_target_from_look_target,
};
use mc_game_core::world::entity::ai::brain::ActivityData;
use mc_game_core::world::entity::ai::memory::MemoryStatus;
use mc_game_core::world::entity::ai::memory::memory_module_type::{
    IS_PANICKING, NEAREST_VISIBLE_ADULT, NEAREST_VISIBLE_PLAYER, TEMPTATION_COOLDOWN_TICKS,
};
use mc_game_core::world::entity::entity::Entity;
use mc_game_core::world::entity::schedule::Activity;

const SPEED_MULTIPLIER_WHEN_IDLING: f32 = 1.0;
const SPEED_MULTIPLIER_WHEN_TEMPTED: f32 = 1.25;
const SPEED_MULTIPLIER_WHEN_FOLLOWING_ADULT: f32 = 1.1;
const BABY_GHAST_CLOSE_ENOUGH_DIST: f64 = 3.0;
/// `ADULT_FOLLOW_RANGE` (`UniformInt.of(3, 16)`).
const ADULT_FOLLOW_RANGE: (i32, i32) = (3, 16);

/// `HappyGhastAi.getActivities()`.
pub fn get_activities(_body: &dyn Entity) -> Vec<ActivityData> {
    vec![
        init_core_activity(),
        init_idle_activity(),
        init_panic_activity(),
    ]
}

/// `initCoreActivity()`.
fn init_core_activity() -> ActivityData {
    ActivityData::create(
        Activity::CORE,
        0,
        vec![
            boxed(Swim::new(0.8)),
            boxed(AnimalPanic::flying(2.0, 0)),
            boxed(LookAtTargetSink::new(45, 90)),
            boxed(MoveToTargetSink::new()),
            boxed(CountDownCooldownTicks::new(TEMPTATION_COOLDOWN_TICKS)),
        ],
    )
}

/// `initIdleActivity()`.
fn init_idle_activity() -> ActivityData {
    let run_one: Box<dyn BehaviorControl> = boxed(RunOne::new(vec![
        (boxed(random_stroll::fly(SPEED_MULTIPLIER_WHEN_IDLING)), 1),
        (
            boxed(set_walk_target_from_look_target::create(
                SPEED_MULTIPLIER_WHEN_IDLING,
                3,
            )),
            1,
        ),
    ]));
    ActivityData::create_pairs(
        Activity::IDLE,
        vec![
            (
                1,
                boxed(FollowTemptation::with_look(
                    |_| SPEED_MULTIPLIER_WHEN_TEMPTED,
                    |_| BABY_GHAST_CLOSE_ENOUGH_DIST,
                    true,
                )),
            ),
            (
                2,
                boxed(baby_follow_adult::create_with(
                    ADULT_FOLLOW_RANGE.0,
                    ADULT_FOLLOW_RANGE.1,
                    |_| SPEED_MULTIPLIER_WHEN_FOLLOWING_ADULT,
                    NEAREST_VISIBLE_PLAYER,
                    true,
                )),
            ),
            (
                3,
                boxed(baby_follow_adult::create_with(
                    ADULT_FOLLOW_RANGE.0,
                    ADULT_FOLLOW_RANGE.1,
                    |_| SPEED_MULTIPLIER_WHEN_FOLLOWING_ADULT,
                    NEAREST_VISIBLE_ADULT,
                    true,
                )),
            ),
            (4, run_one),
        ],
    )
}

/// `initPanicActivity()`.
fn init_panic_activity() -> ActivityData {
    ActivityData::create_pairs_with_conditions(
        Activity::PANIC,
        Vec::new(),
        vec![(IS_PANICKING.id(), MemoryStatus::ValuePresent)],
    )
}

/// `HappyGhastAi.updateActivity(body)`.
pub fn update_activity(body: &mut dyn Entity) {
    body.brain_mut()
        .set_active_activity_to_first_valid(&[Activity::PANIC, Activity::IDLE]);
}
