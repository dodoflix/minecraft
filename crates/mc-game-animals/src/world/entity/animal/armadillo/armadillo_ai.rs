//! `net.minecraft.world.entity.animal.armadillo.ArmadilloAi`: the brain of the armadillo.

use super::armadillo::{Armadillo, animation_duration, is_threatened};
use mc_core::random::RandomSource;
use mc_data::{EntityType, SoundEvent};
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
use mc_game_core::world::damagesource::DamageTypeTags;
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
    DANGER_DETECTED_RECENTLY, GAZE_COOLDOWN_TICKS, IS_PANICKING, TEMPTATION_COOLDOWN_TICKS,
    WALK_TARGET,
};
use mc_game_core::world::entity::entity::Entity;
use mc_game_core::world::entity::schedule::Activity;
use mc_protocol::packets::play::ArmadilloState;

/// `SPEED_MULTIPLIER_WHEN_PANICKING`.
const SPEED_MULTIPLIER_WHEN_PANICKING: f32 = 2.0;
/// `SPEED_MULTIPLIER_WHEN_IDLING`.
const SPEED_MULTIPLIER_WHEN_IDLING: f32 = 1.0;
/// `SPEED_MULTIPLIER_WHEN_TEMPTED`.
const SPEED_MULTIPLIER_WHEN_TEMPTED: f32 = 1.25;
/// `SPEED_MULTIPLIER_WHEN_FOLLOWING_ADULT`.
const SPEED_MULTIPLIER_WHEN_FOLLOWING_ADULT: f32 = 1.25;
/// `SPEED_MULTIPLIER_WHEN_MAKING_LOVE`.
const SPEED_MULTIPLIER_WHEN_MAKING_LOVE: f32 = 1.0;
/// `ADULT_FOLLOW_RANGE` (`UniformInt.of(5, 16)`).
const ADULT_FOLLOW_RANGE: (i32, i32) = (5, 16);
/// `BALL_UP_STAY_IN_STATE`.
const BALL_UP_STAY_IN_STATE: i32 = 5 * 60 * 20;
/// `DANGER_DETECTED_RECENTLY_DANGER_THRESHOLD`.
const DANGER_DETECTED_RECENTLY_DANGER_THRESHOLD: i64 = 75;
/// `MoveToTargetSink()` default timeouts.
const MOVE_TO_TARGET_SINK_TIMEOUT: (i32, i32) = (150, 250);

/// `body` as the armadillo the brain belongs to.
fn armadillo(body: &mut dyn Entity) -> &mut Armadillo {
    body.as_any_mut()
        .downcast_mut::<Armadillo>()
        .expect("armadillo brain on an armadillo")
}

/// `ARMADILLO_ROLLING_OUT`: rolls out as soon as no danger is remembered.
fn armadillo_rolling_out() -> OneShot {
    let mut i = BehaviorBuilder::instance();
    let _danger = i.absent(&DANGER_DETECTED_RECENTLY);
    i.create(move |c| {
        let a = armadillo(&mut *c.body);
        if a.is_scared() {
            a.roll_out(&mut *c.level);
            true
        } else {
            false
        }
    })
}

/// `ArmadilloAi.getActivities()`.
pub fn get_activities(_body: &dyn Entity) -> Vec<ActivityData> {
    vec![
        init_core_activity(),
        init_idle_activity(),
        init_scared_activity(),
    ]
}

/// `initCoreActivity()`.
fn init_core_activity() -> ActivityData {
    ActivityData::create(
        Activity::CORE,
        0,
        vec![
            boxed(Swim::new(0.8)),
            boxed(armadillo_panic(SPEED_MULTIPLIER_WHEN_PANICKING)),
            boxed(LookAtTargetSink::new(45, 90)),
            boxed(armadillo_move_to_target_sink()),
            boxed(CountDownCooldownTicks::new(TEMPTATION_COOLDOWN_TICKS)),
            boxed(CountDownCooldownTicks::new(GAZE_COOLDOWN_TICKS)),
            boxed(armadillo_rolling_out()),
        ],
    )
}

/// `initIdleActivity()`.
fn init_idle_activity() -> ActivityData {
    let follow: Box<dyn BehaviorControl> = boxed(RunOne::new(vec![
        (
            boxed(FollowTemptation::with_distance(
                |_| SPEED_MULTIPLIER_WHEN_TEMPTED,
                |armadillo| if armadillo.is_baby() { 1.0 } else { 2.0 },
            )),
            1,
        ),
        (
            boxed(baby_follow_adult::create(
                ADULT_FOLLOW_RANGE.0,
                ADULT_FOLLOW_RANGE.1,
                SPEED_MULTIPLIER_WHEN_FOLLOWING_ADULT,
            )),
            1,
        ),
    ]));
    let wander: Box<dyn BehaviorControl> = boxed(RunOne::with_entry_condition(
        vec![(WALK_TARGET.id(), MemoryStatus::ValueAbsent)],
        vec![
            (
                boxed(random_stroll::stroll(SPEED_MULTIPLIER_WHEN_IDLING)),
                1,
            ),
            (
                boxed(set_walk_target_from_look_target::create(
                    SPEED_MULTIPLIER_WHEN_IDLING,
                    3,
                )),
                1,
            ),
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
            (
                1,
                boxed(AnimalMakeLove::with(
                    EntityType::ARMADILLO,
                    SPEED_MULTIPLIER_WHEN_MAKING_LOVE,
                    1,
                )),
            ),
            (2, follow),
            (
                3,
                boxed(RandomLookAround::new_uniform(150, 250, 30.0, 0.0, 0.0)),
            ),
            (4, wander),
        ],
    )
}

/// `initScaredActivity()`.
fn init_scared_activity() -> ActivityData {
    ActivityData::create_pairs_with_conditions(
        Activity::PANIC,
        vec![(
            0,
            boxed(BehaviorBase::with_timeout(
                vec![],
                BALL_UP_STAY_IN_STATE,
                ArmadilloBallUp {
                    next_peek_timer: 0,
                    danger_was_around: false,
                },
            )),
        )],
        vec![
            (DANGER_DETECTED_RECENTLY.id(), MemoryStatus::ValuePresent),
            (IS_PANICKING.id(), MemoryStatus::ValueAbsent),
        ],
    )
}

/// `ArmadilloAi.updateActivity(body)`.
pub fn update_activity(body: &mut dyn Entity) {
    body.brain_mut()
        .set_active_activity_to_first_valid(&[Activity::PANIC, Activity::IDLE]);
}

/// `ArmadilloAi.ArmadilloBallUp`: stays rolled up (peeking now and then) while the danger memory lasts.
struct ArmadilloBallUp {
    next_peek_timer: i32,
    danger_was_around: bool,
}

impl ArmadilloBallUp {
    /// `pickNextPeekTimer(body)`.
    fn pick_next_peek_timer(body: &mut dyn Entity) -> i32 {
        animation_duration(ArmadilloState::Scared) as i32
            + body.base_mut().random.next_int_between_inclusive(100, 400)
    }
}

impl Behavior for ArmadilloBallUp {
    fn tick(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        if self.next_peek_timer > 0 {
            self.next_peek_timer -= 1;
        }
        if armadillo(body).should_switch_to_scared_state() {
            armadillo(body).switch_to_state(ArmadilloState::Scared);
            if body.base().on_ground() {
                body.play_sound_default(level, SoundEvent::ENTITY_ARMADILLO_LAND);
            }
        } else {
            let state = armadillo(body).get_state();
            let danger_tick_counter = body
                .brain()
                .get_time_until_expiry(&DANGER_DETECTED_RECENTLY);
            let danger_is_around = danger_tick_counter > DANGER_DETECTED_RECENTLY_DANGER_THRESHOLD;
            if danger_is_around != self.danger_was_around {
                self.next_peek_timer = Self::pick_next_peek_timer(body);
            }
            self.danger_was_around = danger_is_around;
            if state == ArmadilloState::Scared {
                if self.next_peek_timer == 0 && body.base().on_ground() && danger_is_around {
                    level.broadcast_entity_event(body.id(), 64);
                    self.next_peek_timer = Self::pick_next_peek_timer(body);
                }
                if danger_tick_counter < animation_duration(ArmadilloState::Unrolling) {
                    body.play_sound_default(level, SoundEvent::ENTITY_ARMADILLO_UNROLL_START);
                    armadillo(body).switch_to_state(ArmadilloState::Unrolling);
                }
            } else if state == ArmadilloState::Unrolling
                && danger_tick_counter > animation_duration(ArmadilloState::Unrolling)
            {
                armadillo(body).switch_to_state(ArmadilloState::Scared);
            }
        }
    }

    fn check_extra_start_conditions(
        &mut self,
        _level: &mut ServerLevel,
        body: &mut dyn Entity,
    ) -> bool {
        body.base().on_ground() && !body.is_in_liquid()
    }

    fn can_still_use(
        &mut self,
        _level: &mut ServerLevel,
        body: &mut dyn Entity,
        _timestamp: i64,
    ) -> bool {
        is_threatened(armadillo(body).get_state())
    }

    fn start(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        armadillo(body).roll_up(level);
    }

    fn stop(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        if !armadillo(body).can_stay_rolled_up_now() {
            armadillo(body).roll_out(level);
        }
    }
}

/// `ArmadilloAi.ArmadilloPanic extends AnimalPanic`: rolls out before panicking.
struct ArmadilloPanic(AnimalPanic);

fn armadillo_panic(speed_multiplier: f32) -> BehaviorBase<ArmadilloPanic> {
    let base = AnimalPanic::with_damage_types(speed_multiplier, |_| {
        DamageTypeTags::PANIC_ENVIRONMENTAL_CAUSES
    });
    let entry_condition = base.entry_condition.clone();
    BehaviorBase::with_duration(
        entry_condition,
        PANIC_MIN_DURATION,
        PANIC_MAX_DURATION,
        ArmadilloPanic(base.inner),
    )
}

impl Behavior for ArmadilloPanic {
    fn check_extra_start_conditions(
        &mut self,
        level: &mut ServerLevel,
        body: &mut dyn Entity,
    ) -> bool {
        self.0.check_extra_start_conditions(level, body)
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
        armadillo(body).roll_out(level);
        self.0.start(level, body, timestamp);
    }
    fn stop(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, timestamp: i64) {
        self.0.stop(level, body, timestamp);
    }
}

/// The `MoveToTargetSink` of the core activity: it does not start while the armadillo is rolled up.
struct ArmadilloMoveToTargetSink(MoveToTargetSink);

fn armadillo_move_to_target_sink() -> BehaviorBase<ArmadilloMoveToTargetSink> {
    let base = MoveToTargetSink::with_timeout(
        MOVE_TO_TARGET_SINK_TIMEOUT.0,
        MOVE_TO_TARGET_SINK_TIMEOUT.1,
    );
    let entry_condition = base.entry_condition.clone();
    BehaviorBase::with_duration(
        entry_condition,
        MOVE_TO_TARGET_SINK_TIMEOUT.0,
        MOVE_TO_TARGET_SINK_TIMEOUT.1,
        ArmadilloMoveToTargetSink(base.inner),
    )
}

impl Behavior for ArmadilloMoveToTargetSink {
    fn check_extra_start_conditions(
        &mut self,
        level: &mut ServerLevel,
        body: &mut dyn Entity,
    ) -> bool {
        !armadillo(body).is_scared() && self.0.check_extra_start_conditions(level, body)
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
        self.0.start(level, body, timestamp);
    }
    fn stop(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, timestamp: i64) {
        self.0.stop(level, body, timestamp);
    }
}
