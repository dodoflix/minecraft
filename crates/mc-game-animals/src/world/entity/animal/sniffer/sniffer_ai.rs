//! `net.minecraft.world.entity.animal.sniffer.SnifferAi`: the brain of the sniffer (idle, sniff, dig).

use super::sniffer::Sniffer;
use mc_data::EntityType;
use mc_game_ai::world::entity::ai::behavior::animal_make_love::AnimalMakeLove;
use mc_game_ai::world::entity::ai::behavior::animal_panic::{
    AnimalPanic, PANIC_MAX_DURATION, PANIC_MIN_DURATION,
};
use mc_game_ai::world::entity::ai::behavior::behaviors::{CountDownCooldownTicks, boxed};
use mc_game_ai::world::entity::ai::behavior::follow_temptation::FollowTemptation;
use mc_game_ai::world::entity::ai::behavior::random_stroll;
use mc_game_core::entry_condition;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::ai::behavior::look_at_target_sink::LookAtTargetSink;
use mc_game_core::world::entity::ai::behavior::move_to_target_sink::MoveToTargetSink;
use mc_game_core::world::entity::ai::behavior::swim::Swim;
use mc_game_core::world::entity::ai::behavior::{
    Behavior, BehaviorBase, BehaviorControl, DEFAULT_DURATION, DoNothing, RunOne,
    set_entity_look_target, set_walk_target_from_look_target,
};
use mc_game_core::world::entity::ai::brain::ActivityData;
use mc_game_core::world::entity::ai::memory::memory_module_type::{
    BREED_TARGET, IS_PANICKING, SNIFF_COOLDOWN, SNIFFER_DIGGING, SNIFFER_HAPPY,
    SNIFFER_SNIFFING_TARGET, TEMPTATION_COOLDOWN_TICKS, WALK_TARGET,
};
use mc_game_core::world::entity::ai::memory::{MemoryStatus, Unit, WalkTarget};
use mc_game_core::world::entity::entity::Entity;
use mc_game_core::world::entity::mob::animal;
use mc_game_core::world::entity::schedule::Activity;
use mc_protocol::packets::play::SnifferState;
use std::cell::Cell;

const SNIFFING_COOLDOWN_TICKS: i64 = 9600;
const SPEED_MULTIPLIER_WHEN_IDLING: f32 = 1.0;
const SPEED_MULTIPLIER_WHEN_PANICKING: f32 = 2.0;
const SPEED_MULTIPLIER_WHEN_SNIFFING: f32 = 1.25;
const SPEED_MULTIPLIER_WHEN_TEMPTED: f32 = 1.25;
/// `MAX_LOOK_DISTANCE`.
const MAX_LOOK_DISTANCE: f32 = 6.0;
/// `FollowTemptation` and `AnimalMakeLove` default timeouts.
const MAKE_LOVE_TIMEOUT: i32 = 110;

/// `body` as the sniffer the brain belongs to.
fn sniffer(body: &mut dyn Entity) -> &mut Sniffer {
    body.as_any_mut()
        .downcast_mut::<Sniffer>()
        .expect("sniffer brain on a sniffer")
}

/// `SnifferAi.getActivities()`.
pub fn get_activities(_body: &dyn Entity) -> Vec<ActivityData> {
    vec![
        init_core_activity(),
        init_idle_activity(),
        init_sniffing_activity(),
        init_dig_activity(),
    ]
}

/// `resetSniffing(body)`.
fn reset_sniffing(level: &mut ServerLevel, body: &mut dyn Entity) {
    body.brain_mut().erase_memory(&SNIFFER_DIGGING);
    body.brain_mut().erase_memory(&SNIFFER_SNIFFING_TARGET);
    sniffer(body).transition_to(level, SnifferState::Idling);
}

/// `Behavior.timedOut(timestamp)` inside `stop`: the base class keeps `endTimestamp` to itself and hands it only to
/// `timed_out`, so the last value seen there (every `tickOrStop` asks first) is what `stop` compares against.
struct EndTimestamp(Cell<i64>);

impl EndTimestamp {
    fn new() -> Self {
        Self(Cell::new(i64::MAX))
    }
    /// `start`: a fresh run has not seen its end yet.
    fn restart(&self) {
        self.0.set(i64::MAX);
    }
    /// `timedOut(timestamp)` as asked by `tickOrStop`.
    fn check(&self, timestamp: i64, end_timestamp: i64) -> bool {
        self.0.set(end_timestamp);
        timestamp > end_timestamp
    }
    /// `timedOut(timestamp)` as asked by `stop`.
    fn finished(&self, timestamp: i64) -> bool {
        timestamp > self.0.get()
    }
}

/// An anonymous subclass of a shared behaviour that calls `resetSniffing(body)` before the super `start`
/// (`AnimalPanic`, `AnimalMakeLove`, `FollowTemptation`).
struct ResetSniffingFirst<B: Behavior>(B);

/// `new B() { start(..) { resetSniffing(body); super.start(..); } }` with the base's entry condition and durations.
fn reset_sniffing_first<B: Behavior>(
    base: BehaviorBase<B>,
    min_duration: i32,
    max_duration: i32,
) -> BehaviorBase<ResetSniffingFirst<B>> {
    BehaviorBase::with_duration(
        base.entry_condition.clone(),
        min_duration,
        max_duration,
        ResetSniffingFirst(base.inner),
    )
}

impl<B: Behavior> Behavior for ResetSniffingFirst<B> {
    fn start(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, timestamp: i64) {
        reset_sniffing(level, body);
        self.0.start(level, body, timestamp);
    }
    fn tick(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, timestamp: i64) {
        self.0.tick(level, body, timestamp);
    }
    fn stop(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, timestamp: i64) {
        self.0.stop(level, body, timestamp);
    }
    fn can_still_use(
        &mut self,
        level: &mut ServerLevel,
        body: &mut dyn Entity,
        timestamp: i64,
    ) -> bool {
        self.0.can_still_use(level, body, timestamp)
    }
    fn timed_out(&self, timestamp: i64, end_timestamp: i64) -> bool {
        self.0.timed_out(timestamp, end_timestamp)
    }
    fn check_extra_start_conditions(
        &mut self,
        level: &mut ServerLevel,
        body: &mut dyn Entity,
    ) -> bool {
        self.0.check_extra_start_conditions(level, body)
    }
}

/// `initCoreActivity()`.
fn init_core_activity() -> ActivityData {
    ActivityData::create(
        Activity::CORE,
        0,
        vec![
            boxed(Swim::new(0.8)),
            boxed(reset_sniffing_first(
                AnimalPanic::new(SPEED_MULTIPLIER_WHEN_PANICKING),
                PANIC_MIN_DURATION,
                PANIC_MAX_DURATION,
            )),
            boxed(MoveToTargetSink::with_timeout(500, 700)),
            boxed(CountDownCooldownTicks::new(TEMPTATION_COOLDOWN_TICKS)),
        ],
    )
}

/// `initSniffingActivity()`.
fn init_sniffing_activity() -> ActivityData {
    ActivityData::create_pairs_with_conditions(
        Activity::SNIFF,
        vec![(0, boxed(searching()))],
        vec![
            (IS_PANICKING.id(), MemoryStatus::ValueAbsent),
            (SNIFFER_SNIFFING_TARGET.id(), MemoryStatus::ValuePresent),
            (WALK_TARGET.id(), MemoryStatus::ValuePresent),
        ],
    )
}

/// `initDigActivity()`.
fn init_dig_activity() -> ActivityData {
    ActivityData::create_pairs_with_conditions(
        Activity::DIG,
        vec![
            (0, boxed(digging(160, 180))),
            (0, boxed(finished_digging(40))),
        ],
        vec![
            (IS_PANICKING.id(), MemoryStatus::ValueAbsent),
            (WALK_TARGET.id(), MemoryStatus::ValueAbsent),
            (SNIFFER_DIGGING.id(), MemoryStatus::ValuePresent),
        ],
    )
}

/// `initIdleActivity()`.
fn init_idle_activity() -> ActivityData {
    let run_one: Box<dyn BehaviorControl> = boxed(RunOne::new(vec![
        (
            boxed(set_walk_target_from_look_target::create(
                SPEED_MULTIPLIER_WHEN_IDLING,
                3,
            )),
            2,
        ),
        (boxed(scenting(40, 80)), 1),
        (boxed(sniffing(40, 80)), 1),
        (
            boxed(set_entity_look_target::create_type(
                EntityType::PLAYER,
                MAX_LOOK_DISTANCE,
            )),
            1,
        ),
        (
            boxed(random_stroll::stroll(SPEED_MULTIPLIER_WHEN_IDLING)),
            1,
        ),
        (boxed(DoNothing::new(5, 20)), 2),
    ]));
    ActivityData::create_pairs_with_conditions(
        Activity::IDLE,
        vec![
            (
                0,
                boxed(reset_sniffing_first(
                    AnimalMakeLove::new(EntityType::SNIFFER),
                    MAKE_LOVE_TIMEOUT,
                    MAKE_LOVE_TIMEOUT,
                )),
            ),
            (
                1,
                boxed(reset_sniffing_first(
                    FollowTemptation::with_distance(
                        |_| SPEED_MULTIPLIER_WHEN_TEMPTED,
                        |sniffer| if sniffer.is_baby() { 2.5 } else { 3.5 },
                    ),
                    DEFAULT_DURATION,
                    DEFAULT_DURATION,
                )),
            ),
            (2, boxed(LookAtTargetSink::new(45, 90))),
            (3, boxed(feeling_happy(40, 100))),
            (4, run_one),
        ],
        vec![(SNIFFER_DIGGING.id(), MemoryStatus::ValueAbsent)],
    )
}

/// `SnifferAi.updateActivity(body)`.
pub fn update_activity(body: &mut dyn Entity) {
    body.brain_mut().set_active_activity_to_first_valid(&[
        Activity::DIG,
        Activity::SNIFF,
        Activity::IDLE,
    ]);
}

/// `SnifferAi.Digging`.
struct Digging {
    end: EndTimestamp,
}

fn digging(min: i32, max: i32) -> BehaviorBase<Digging> {
    BehaviorBase::with_duration(
        entry_condition![
            IS_PANICKING => ValueAbsent,
            WALK_TARGET => ValueAbsent,
            SNIFFER_DIGGING => ValuePresent,
            SNIFF_COOLDOWN => ValueAbsent,
        ],
        min,
        max,
        Digging {
            end: EndTimestamp::new(),
        },
    )
}

impl Behavior for Digging {
    fn check_extra_start_conditions(
        &mut self,
        _level: &mut ServerLevel,
        body: &mut dyn Entity,
    ) -> bool {
        sniffer(body).can_sniff()
    }

    fn can_still_use(
        &mut self,
        level: &mut ServerLevel,
        body: &mut dyn Entity,
        _timestamp: i64,
    ) -> bool {
        body.brain().get_memory(&SNIFFER_DIGGING).is_some()
            && sniffer(body).can_dig(level)
            && !animal::is_in_love(body)
    }

    fn timed_out(&self, timestamp: i64, end_timestamp: i64) -> bool {
        self.end.check(timestamp, end_timestamp)
    }

    fn start(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        self.end.restart();
        sniffer(body).transition_to(level, SnifferState::Digging);
    }

    fn stop(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, timestamp: i64) {
        if self.end.finished(timestamp) {
            body.brain_mut()
                .set_memory_with_expiry(&SNIFF_COOLDOWN, Unit, SNIFFING_COOLDOWN_TICKS);
        } else {
            reset_sniffing(level, body);
        }
    }
}

/// `SnifferAi.FeelingHappy`.
struct FeelingHappy;

fn feeling_happy(min: i32, max: i32) -> BehaviorBase<FeelingHappy> {
    BehaviorBase::with_duration(
        entry_condition![SNIFFER_HAPPY => ValuePresent],
        min,
        max,
        FeelingHappy,
    )
}

impl Behavior for FeelingHappy {
    fn can_still_use(
        &mut self,
        _level: &mut ServerLevel,
        _body: &mut dyn Entity,
        _timestamp: i64,
    ) -> bool {
        true
    }

    fn start(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        sniffer(body).transition_to(level, SnifferState::FeelingHappy);
    }

    fn stop(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        sniffer(body).transition_to(level, SnifferState::Idling);
        body.brain_mut().erase_memory(&SNIFFER_HAPPY);
    }
}

/// `SnifferAi.FinishedDigging`.
struct FinishedDigging {
    end: EndTimestamp,
}

fn finished_digging(duration: i32) -> BehaviorBase<FinishedDigging> {
    BehaviorBase::with_duration(
        entry_condition![
            IS_PANICKING => ValueAbsent,
            WALK_TARGET => ValueAbsent,
            SNIFFER_DIGGING => ValuePresent,
            SNIFF_COOLDOWN => ValuePresent,
        ],
        duration,
        duration,
        FinishedDigging {
            end: EndTimestamp::new(),
        },
    )
}

impl Behavior for FinishedDigging {
    fn can_still_use(
        &mut self,
        _level: &mut ServerLevel,
        body: &mut dyn Entity,
        _timestamp: i64,
    ) -> bool {
        body.brain().get_memory(&SNIFFER_DIGGING).is_some()
    }

    fn timed_out(&self, timestamp: i64, end_timestamp: i64) -> bool {
        self.end.check(timestamp, end_timestamp)
    }

    fn start(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        self.end.restart();
        sniffer(body).transition_to(level, SnifferState::Rising);
    }

    fn stop(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, timestamp: i64) {
        let finished = self.end.finished(timestamp);
        sniffer(body).transition_to(level, SnifferState::Idling);
        sniffer(body).on_digging_complete(level, finished);
        body.brain_mut().erase_memory(&SNIFFER_DIGGING);
        body.brain_mut().set_memory(&SNIFFER_HAPPY, true);
    }
}

/// `SnifferAi.Scenting`.
struct Scenting;

fn scenting(min: i32, max: i32) -> BehaviorBase<Scenting> {
    BehaviorBase::with_duration(
        entry_condition![
            IS_PANICKING => ValueAbsent,
            SNIFFER_DIGGING => ValueAbsent,
            SNIFFER_SNIFFING_TARGET => ValueAbsent,
            SNIFFER_HAPPY => ValueAbsent,
            BREED_TARGET => ValueAbsent,
        ],
        min,
        max,
        Scenting,
    )
}

impl Behavior for Scenting {
    fn check_extra_start_conditions(
        &mut self,
        _level: &mut ServerLevel,
        body: &mut dyn Entity,
    ) -> bool {
        !sniffer(body).is_tempted()
    }

    fn can_still_use(
        &mut self,
        _level: &mut ServerLevel,
        _body: &mut dyn Entity,
        _timestamp: i64,
    ) -> bool {
        true
    }

    fn start(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        sniffer(body).transition_to(level, SnifferState::Scenting);
    }

    fn stop(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        sniffer(body).transition_to(level, SnifferState::Idling);
    }
}

/// `SnifferAi.Searching`.
struct Searching;

fn searching() -> BehaviorBase<Searching> {
    BehaviorBase::with_timeout(
        entry_condition![
            WALK_TARGET => ValuePresent,
            IS_PANICKING => ValueAbsent,
            SNIFFER_SNIFFING_TARGET => ValuePresent,
        ],
        600,
        Searching,
    )
}

impl Behavior for Searching {
    fn check_extra_start_conditions(
        &mut self,
        _level: &mut ServerLevel,
        body: &mut dyn Entity,
    ) -> bool {
        sniffer(body).can_sniff()
    }

    fn can_still_use(
        &mut self,
        level: &mut ServerLevel,
        body: &mut dyn Entity,
        _timestamp: i64,
    ) -> bool {
        if !sniffer(body).can_sniff() {
            sniffer(body).transition_to(level, SnifferState::Idling);
            return false;
        }
        let walk_target = body
            .brain()
            .get_memory(&WALK_TARGET)
            .map(|w| w.get_target().current_block_position(level));
        let sniffing_target = body.brain().get_memory(&SNIFFER_SNIFFING_TARGET).copied();
        match (walk_target, sniffing_target) {
            (Some(walk), Some(sniffing)) => sniffing == walk,
            _ => false,
        }
    }

    fn start(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        sniffer(body).transition_to(level, SnifferState::Searching);
    }

    fn stop(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        if sniffer(body).can_dig(level) && sniffer(body).can_sniff() {
            body.brain_mut().set_memory(&SNIFFER_DIGGING, true);
        }
        body.brain_mut().erase_memory(&WALK_TARGET);
        body.brain_mut().erase_memory(&SNIFFER_SNIFFING_TARGET);
    }
}

/// `SnifferAi.Sniffing`.
struct Sniffing {
    end: EndTimestamp,
}

fn sniffing(min: i32, max: i32) -> BehaviorBase<Sniffing> {
    BehaviorBase::with_duration(
        entry_condition![
            WALK_TARGET => ValueAbsent,
            SNIFFER_SNIFFING_TARGET => ValueAbsent,
            SNIFF_COOLDOWN => ValueAbsent,
        ],
        min,
        max,
        Sniffing {
            end: EndTimestamp::new(),
        },
    )
}

impl Behavior for Sniffing {
    fn check_extra_start_conditions(
        &mut self,
        _level: &mut ServerLevel,
        body: &mut dyn Entity,
    ) -> bool {
        let sniffer = sniffer(body);
        !sniffer.is_baby() && sniffer.can_sniff()
    }

    fn can_still_use(
        &mut self,
        _level: &mut ServerLevel,
        body: &mut dyn Entity,
        _timestamp: i64,
    ) -> bool {
        sniffer(body).can_sniff()
    }

    fn timed_out(&self, timestamp: i64, end_timestamp: i64) -> bool {
        self.end.check(timestamp, end_timestamp)
    }

    fn start(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        self.end.restart();
        sniffer(body).transition_to(level, SnifferState::Sniffing);
    }

    fn stop(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, timestamp: i64) {
        let finished = self.end.finished(timestamp);
        sniffer(body).transition_to(level, SnifferState::Idling);
        if finished && let Some(position) = sniffer(body).calculate_dig_position(level) {
            body.brain_mut()
                .set_memory(&SNIFFER_SNIFFING_TARGET, position);
            body.brain_mut().set_memory(
                &WALK_TARGET,
                WalkTarget::from_block_pos(position, SPEED_MULTIPLIER_WHEN_SNIFFING, 0),
            );
        }
    }
}
