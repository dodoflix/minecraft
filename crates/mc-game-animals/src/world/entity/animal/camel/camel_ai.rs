//! `net.minecraft.world.entity.animal.camel.CamelAi`: the brain of the camel and the camel husk.

use super::camel::Camel;
use mc_core::random::RandomSource;
use mc_data::EntityType;
use mc_game_ai::world::entity::ai::behavior::animal_panic::{
    AnimalPanic, PANIC_MAX_DURATION, PANIC_MIN_DURATION,
};
use mc_game_ai::world::entity::ai::behavior::behaviors::{CountDownCooldownTicks, boxed};
use mc_game_ai::world::entity::ai::behavior::random_look_around::RandomLookAround;
use mc_game_ai::world::entity::ai::behavior::{
    animal_make_love::AnimalMakeLove, baby_follow_adult, follow_temptation::FollowTemptation,
    random_stroll, set_entity_look_target_sometimes,
};
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::ai::behavior::look_at_target_sink::LookAtTargetSink;
use mc_game_core::world::entity::ai::behavior::move_to_target_sink::MoveToTargetSink;
use mc_game_core::world::entity::ai::behavior::swim::Swim;
use mc_game_core::world::entity::ai::behavior::{
    Behavior, BehaviorBase, BehaviorBuilder, BehaviorControl, DoNothing, OneShot, RunOne,
    set_walk_target_from_look_target,
};
use mc_game_core::world::entity::ai::brain::ActivityData;
use mc_game_core::world::entity::ai::memory::MemoryStatus;
use mc_game_core::world::entity::ai::memory::memory_module_type::{
    GAZE_COOLDOWN_TICKS, TEMPTATION_COOLDOWN_TICKS, WALK_TARGET,
};
use mc_game_core::world::entity::entity::Entity;
use mc_game_core::world::entity::schedule::Activity;

/// `SPEED_MULTIPLIER_WHEN_PANICKING`.
const SPEED_MULTIPLIER_WHEN_PANICKING: f32 = 4.0;
/// `SPEED_MULTIPLIER_WHEN_IDLING`.
const SPEED_MULTIPLIER_WHEN_IDLING: f32 = 2.0;
/// `SPEED_MULTIPLIER_WHEN_TEMPTED`.
const SPEED_MULTIPLIER_WHEN_TEMPTED: f32 = 2.5;
/// `SPEED_MULTIPLIER_WHEN_FOLLOWING_ADULT`.
const SPEED_MULTIPLIER_WHEN_FOLLOWING_ADULT: f32 = 2.5;
/// `ADULT_FOLLOW_RANGE` (`UniformInt.of(5, 16)`).
const ADULT_FOLLOW_RANGE: (i32, i32) = (5, 16);

/// `body` as the camel the brain belongs to.
fn camel(body: &dyn Entity) -> &Camel {
    body.as_any()
        .downcast_ref::<Camel>()
        .expect("camel brain on a camel")
}

/// `CamelAi.initMemories(body, random)`: nothing to remember at spawn.
pub fn init_memories(_body: &mut dyn Entity, _random: &mut dyn RandomSource) {}

/// `CamelAi.getActivities()`.
pub fn get_activities(_body: &dyn Entity) -> Vec<ActivityData> {
    vec![init_core_activity(), init_idle_activity()]
}

/// `initCoreActivity()`.
fn init_core_activity() -> ActivityData {
    ActivityData::create(
        Activity::CORE,
        0,
        vec![
            boxed(Swim::new(0.8)),
            boxed(camel_panic(SPEED_MULTIPLIER_WHEN_PANICKING)),
            boxed(LookAtTargetSink::new(45, 90)),
            boxed(MoveToTargetSink::new()),
            boxed(CountDownCooldownTicks::new(TEMPTATION_COOLDOWN_TICKS)),
            boxed(CountDownCooldownTicks::new(GAZE_COOLDOWN_TICKS)),
        ],
    )
}

/// `initIdleActivity()`.
fn init_idle_activity() -> ActivityData {
    // `BehaviorBuilder.triggerIf(Predicate.not(Camel::refuseToMove), behavior)`
    fn unless_refusing(behavior: OneShot) -> OneShot {
        BehaviorBuilder::sequence(
            |level: &mut ServerLevel, body: &mut dyn Entity, _: i64| {
                !camel(&*body).refuse_to_move(level.game_time())
            },
            behavior,
        )
    }

    let follow: Box<dyn BehaviorControl> = boxed(RunOne::new(vec![
        (
            boxed(FollowTemptation::with_distance(
                |_| SPEED_MULTIPLIER_WHEN_TEMPTED,
                |camel| if camel.is_baby() { 2.5 } else { 3.5 },
            )),
            1,
        ),
        (
            boxed(unless_refusing(baby_follow_adult::create(
                ADULT_FOLLOW_RANGE.0,
                ADULT_FOLLOW_RANGE.1,
                SPEED_MULTIPLIER_WHEN_FOLLOWING_ADULT,
            ))),
            1,
        ),
    ]));
    let wander: Box<dyn BehaviorControl> = boxed(RunOne::with_entry_condition(
        vec![(WALK_TARGET.id(), MemoryStatus::ValueAbsent)],
        vec![
            (
                boxed(unless_refusing(random_stroll::stroll(
                    SPEED_MULTIPLIER_WHEN_IDLING,
                ))),
                1,
            ),
            (
                boxed(unless_refusing(set_walk_target_from_look_target::create(
                    SPEED_MULTIPLIER_WHEN_IDLING,
                    3,
                ))),
                1,
            ),
            (boxed(random_sitting(20)), 1),
            (boxed(DoNothing::new(30, 60)), 1),
        ],
    ));
    ActivityData::create_pairs(
        Activity::IDLE,
        vec![
            (
                0,
                boxed(set_entity_look_target_sometimes::create_type(
                    EntityType::PLAYER,
                    6.0,
                    30,
                    60,
                )),
            ),
            (1, boxed(AnimalMakeLove::new(EntityType::CAMEL))),
            (2, follow),
            (
                3,
                boxed(RandomLookAround::new_uniform(150, 250, 30.0, 0.0, 0.0)),
            ),
            (4, wander),
        ],
    )
}

/// `CamelAi.updateActivity(body)`.
pub fn update_activity(body: &mut dyn Entity) {
    body.brain_mut()
        .set_active_activity_to_first_valid(&[Activity::IDLE]);
}

/// `CamelAi.CamelPanic extends AnimalPanic`: no panic while a mob drives the camel, and it stands up at once before running.
struct CamelPanic(AnimalPanic);

fn camel_panic(speed_multiplier: f32) -> BehaviorBase<CamelPanic> {
    let base = AnimalPanic::new(speed_multiplier);
    let entry_condition = base.entry_condition.clone();
    BehaviorBase::with_duration(
        entry_condition,
        PANIC_MIN_DURATION,
        PANIC_MAX_DURATION,
        CamelPanic(base.inner),
    )
}

impl Behavior for CamelPanic {
    fn check_extra_start_conditions(
        &mut self,
        level: &mut ServerLevel,
        body: &mut dyn Entity,
    ) -> bool {
        self.0.check_extra_start_conditions(level, body) && !camel(body).is_mob_controlled()
    }
    fn can_still_use(
        &mut self,
        level: &mut ServerLevel,
        body: &mut dyn Entity,
        timestamp: i64,
    ) -> bool {
        self.0.can_still_use(level, body, timestamp)
    }
    fn tick(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, timestamp: i64) {
        self.0.tick(level, body, timestamp);
    }
    fn timed_out(&self, timestamp: i64, end_timestamp: i64) -> bool {
        self.0.timed_out(timestamp, end_timestamp)
    }
    fn start(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, timestamp: i64) {
        body.as_any_mut()
            .downcast_mut::<Camel>()
            .expect("camel brain on a camel")
            .stand_up_instantly(level);
        self.0.start(level, body, timestamp);
    }
    fn stop(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, timestamp: i64) {
        self.0.stop(level, body, timestamp);
    }
}

/// `CamelAi.RandomSitting`: sits down (or stands up) after a pose has lasted `minimalPoseTimeSec` seconds.
struct RandomSitting {
    minimal_pose_ticks: i64,
}

fn random_sitting(minimal_pose_time_sec: i32) -> BehaviorBase<RandomSitting> {
    BehaviorBase::new(
        vec![],
        RandomSitting {
            minimal_pose_ticks: i64::from(minimal_pose_time_sec * 20),
        },
    )
}

impl Behavior for RandomSitting {
    fn check_extra_start_conditions(
        &mut self,
        level: &mut ServerLevel,
        body: &mut dyn Entity,
    ) -> bool {
        let camel = camel(body);
        !camel.is_in_water()
            && camel.get_pose_time(level.game_time()) >= self.minimal_pose_ticks
            && !mc_game_core::world::entity::mob::leashable::is_leashed(camel)
            && camel.base().on_ground()
            && !camel.has_controlling_passenger()
            && camel.can_camel_change_pose(level)
    }

    fn start(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        let is_panicking = body.is_panicking();
        let camel = body
            .as_any_mut()
            .downcast_mut::<Camel>()
            .expect("camel brain on a camel");
        if camel.is_camel_sitting() {
            camel.stand_up(level);
        } else if !is_panicking {
            camel.sit_down(level);
        }
    }
}
