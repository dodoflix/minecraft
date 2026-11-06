//! `net.minecraft.world.entity.ai.behavior`: the behavior framework. The concrete behaviors of vanilla's
//! `ai/behavior/*.java` are the modules below, written against this API.
//!
//! # Mapping from vanilla
//!
//! | vanilla | here |
//! |---|---|
//! | `BehaviorControl<E>` | [`BehaviorControl`] (object-safe; `body` is `&mut dyn Entity`, downcast with `as_any_mut()` for mob-specific state) |
//! | `Behavior<E>` (abstract class with `start/tick/stop/canStillUse/timedOut/checkExtraStartConditions`) | trait [`Behavior`] (the hooks, defaults = vanilla) wrapped by [`BehaviorBase`] (the `final` template methods: entry-condition map, status, duration, `endTimestamp`) |
//! | `OneShot<E>` + `Trigger<E>` | [`OneShot`] + [`Trigger`] (any `FnMut(&mut ServerLevel, &mut dyn Entity, i64) -> bool` is a `Trigger`) |
//! | `BehaviorBuilder.create(i -> i.group(i.present(A), i.absent(B)).apply(i, (a, b) -> trigger))` | [`BehaviorBuilder`] (see below) |
//! | `GateBehavior`/`RunOne`/`TriggerGate`/`ShufflingList` | same names, same RNG consumption |
//! | `PositionTracker`/`BlockPosTracker`/`EntityTracker` | [`position_tracker`] |
//!
//! `level` is the `ServerLevel` the mob lives in, `body` the (already taken-out) mob; the brain stays inside the
//! mob (`body.brain()`), only the one behavior/sensor being called is temporarily checked out of it, so behaviors may
//! freely read and write memories of `body` (and replace its brain, as `Villager.refreshBrain` does).
//! All times are game-time ticks (`i64`), durations `i32` exactly like vanilla.
//!
//! # Writing a behavior
//!
//! ## 1. Declarative (`BehaviorBuilder`): memory conditions + one trigger closure
//!
//! Vanilla's `SetWalkTargetFromLookTarget` is a `BehaviorBuilder.create` call that requires the `WALK_TARGET` memory to
//! be absent and `LOOK_TARGET` to be present; its trigger returns `false` when the `canWalkTo` predicate fails and
//! otherwise sets `WALK_TARGET` from the look target, the speed modifier and the close-enough distance. Here the
//! memories are declared with [`BehaviorBuilder::absent`] / [`BehaviorBuilder::present`] and the trigger is a closure over
//! a [`TriggerContext`]; see [`set_walk_target_from_look_target`] for the real implementation.
//!
//! Conditions are evaluated in declaration order when the behavior is tried; the first failing one aborts (vanilla:
//! `tryTrigger` returns `null`), the closure runs only if all passed. The memories' values are cloned out when the
//! conditions are checked ([`TriggerContext::get`] / [`TriggerContext::try_get`]), the accessors write through to
//! the brain ([`TriggerContext::set`], `set_or_erase`, `set_with_expiry`, `erase`).
//!
//! ## 2. Classic ([`Behavior`] + [`BehaviorBase`]): `start / tick / stop / can_still_use` with a duration
//!
//! Vanilla's `LookAtTargetSink` is a `Behavior` with an entry condition (`LOOK_TARGET` present) and a duration range:
//! `canStillUse` checks that the look target is visible, `stop` erases the memory and `tick` makes the mob look at the
//! target's current position. Here the entry condition goes into [`BehaviorBase::with_duration`] and the three hooks are
//! methods of the [`Behavior`] impl; see [`look_at_target_sink`] for the real implementation.
//!
//! ## 3. Register with an activity
//!
//! `ActivityData::create(Activity::IDLE, 0, vec![Box::new(behavior), ...])` / the other `ActivityData` constructors
//! and `Brain::provider(...)` (see [`crate::world::entity::ai::brain`]). Control flow behaviors:
//! [`RunOne`], [`GateBehavior`] (weights/order/running policy), [`trigger_gate`], [`DoNothing`].

pub mod acquire_poi;
pub mod assign_profession_from_job_site;
pub mod behavior_utils;
pub mod celebrate_villagers_survived_raid;
pub mod declarative;
pub mod do_nothing;
pub mod gate_behavior;
pub mod give_gift_to_hero;
pub mod go_to_closest_village;
pub mod go_to_potential_job_site;
pub mod go_to_wanted_item;
pub mod harvest_farmland;
pub mod inside_brownian_walk;
pub mod interact_with;
pub mod interact_with_door;
pub mod jump_on_bed;
pub mod locate_hiding_place;
pub mod look_and_follow_trading_player_sink;
pub mod look_at_target_sink;
pub mod move_to_sky_seeing_spot;
pub mod move_to_target_sink;
pub mod one_shot;
pub mod play_tag_with_other_kids;
pub mod poi_competitor_scan;
pub mod position_tracker;
pub mod react_to_bell;
pub mod reset_profession;
pub mod reset_raid_status;
pub mod ring_bell;
pub mod set_closest_home_as_walk_target;
pub mod set_entity_look_target;
pub mod set_hidden_state;
pub mod set_look_and_interact;
pub mod set_raid_status;
pub mod set_walk_target_away_from;
pub mod set_walk_target_from_block_memory;
pub mod set_walk_target_from_look_target;
pub mod show_trades_to_player;
pub mod shuffling_list;
pub mod sleep_in_bed;
pub mod socialize_at_bell;
pub mod stroll_around_poi;
pub mod stroll_to_poi;
pub mod stroll_to_poi_list;
pub mod swim;
pub mod trade_with_villager;
pub mod trigger_gate;
pub mod update_activity_from_schedule;
pub mod use_bonemeal;
pub mod validate_nearby_poi;
pub mod village_bound_random_stroll;
pub mod village_util;
pub mod villager_calm_down;
pub mod villager_goal_packages;
pub mod villager_make_love;
pub mod villager_panic_trigger;
pub mod wake_up;
pub mod work_at_composter;
pub mod work_at_poi;
pub mod yield_job_site;

pub use declarative::{Absent, BehaviorBuilder, Present, Registered, TriggerContext};
pub use do_nothing::DoNothing;
pub use gate_behavior::{GateBehavior, OrderPolicy, RunOne, RunningPolicy};
pub use one_shot::{OneShot, Trigger};
pub use shuffling_list::ShufflingList;

use crate::server::level::ServerLevel;
use crate::world::entity::ai::memory::{MemoryStatus, MemoryTypeId};
use crate::world::entity::entity::Entity;

/// `Behavior.Status`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Status {
    Stopped,
    Running,
}

/// `BehaviorControl<E>`.
pub trait BehaviorControl: 'static {
    fn get_status(&self) -> Status;
    fn get_required_memories(&self) -> Vec<MemoryTypeId>;
    fn try_start(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, timestamp: i64)
    -> bool;
    fn tick_or_stop(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, timestamp: i64);
    fn do_stop(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, timestamp: i64);
    fn debug_string(&self) -> String;
}

/// The overridable hooks of vanilla `Behavior<E>` (defaults = the base class). Wrap it in a [`BehaviorBase`] to get
/// the `BehaviorControl` template methods.
pub trait Behavior: 'static {
    /// `start(level, body, timestamp)`.
    fn start(&mut self, _level: &mut ServerLevel, _body: &mut dyn Entity, _timestamp: i64) {}
    /// `tick(level, body, timestamp)`.
    fn tick(&mut self, _level: &mut ServerLevel, _body: &mut dyn Entity, _timestamp: i64) {}
    /// `stop(level, body, timestamp)`.
    fn stop(&mut self, _level: &mut ServerLevel, _body: &mut dyn Entity, _timestamp: i64) {}
    /// `canStillUse(level, body, timestamp)`.
    fn can_still_use(
        &mut self,
        _level: &mut ServerLevel,
        _body: &mut dyn Entity,
        _timestamp: i64,
    ) -> bool {
        false
    }
    /// `timedOut(timestamp)`; `end_timestamp` is the base class field `endTimestamp`.
    fn timed_out(&self, timestamp: i64, end_timestamp: i64) -> bool {
        timestamp > end_timestamp
    }
    /// `checkExtraStartConditions(level, body)`.
    fn check_extra_start_conditions(
        &mut self,
        _level: &mut ServerLevel,
        _body: &mut dyn Entity,
    ) -> bool {
        true
    }
    /// `getClass().getSimpleName()` for `debugString()`.
    fn debug_name(&self) -> String {
        let full = std::any::type_name::<Self>();
        full.rsplit("::").next().unwrap_or(full).to_string()
    }
}

/// `Behavior.DEFAULT_DURATION`.
pub const DEFAULT_DURATION: i32 = 60;

/// The non-overridable part of `Behavior<E>`: entry condition, status, duration roll and the
/// `tryStart`/`tickOrStop`/`doStop` template methods.
pub struct BehaviorBase<B: Behavior> {
    pub entry_condition: Vec<(MemoryTypeId, MemoryStatus)>,
    status: Status,
    end_timestamp: i64,
    min_duration: i32,
    max_duration: i32,
    pub inner: B,
}

impl<B: Behavior> BehaviorBase<B> {
    /// `Behavior(entryCondition)` (duration 60).
    pub fn new(entry_condition: Vec<(MemoryTypeId, MemoryStatus)>, inner: B) -> Self {
        Self::with_duration(entry_condition, DEFAULT_DURATION, DEFAULT_DURATION, inner)
    }

    /// `Behavior(entryCondition, timeOutDuration)`.
    pub fn with_timeout(
        entry_condition: Vec<(MemoryTypeId, MemoryStatus)>,
        time_out_duration: i32,
        inner: B,
    ) -> Self {
        Self::with_duration(entry_condition, time_out_duration, time_out_duration, inner)
    }

    /// `Behavior(entryCondition, minDuration, maxDuration)`.
    pub fn with_duration(
        entry_condition: Vec<(MemoryTypeId, MemoryStatus)>,
        min_duration: i32,
        max_duration: i32,
        inner: B,
    ) -> Self {
        Self {
            entry_condition,
            status: Status::Stopped,
            end_timestamp: 0,
            min_duration,
            max_duration,
            inner,
        }
    }

    /// `hasRequiredMemories(body)`.
    pub fn has_required_memories(&self, body: &dyn Entity) -> bool {
        self.entry_condition
            .iter()
            .all(|&(t, s)| body.brain().check_memory(t, s))
    }
}

impl<B: Behavior> BehaviorControl for BehaviorBase<B> {
    fn get_status(&self) -> Status {
        self.status
    }

    fn get_required_memories(&self) -> Vec<MemoryTypeId> {
        self.entry_condition.iter().map(|&(t, _)| t).collect()
    }

    fn try_start(
        &mut self,
        level: &mut ServerLevel,
        body: &mut dyn Entity,
        timestamp: i64,
    ) -> bool {
        if self.has_required_memories(body) && self.inner.check_extra_start_conditions(level, body)
        {
            self.status = Status::Running;
            let bound = self
                .max_duration
                .wrapping_add(1)
                .wrapping_sub(self.min_duration);
            let duration =
                self.min_duration
                    .wrapping_add(mc_core::random::RandomSource::next_int_bounded(
                        &mut level.tick_state.random,
                        bound,
                    ));
            self.end_timestamp = timestamp + i64::from(duration);
            self.inner.start(level, body, timestamp);
            true
        } else {
            false
        }
    }

    fn tick_or_stop(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, timestamp: i64) {
        if !self.inner.timed_out(timestamp, self.end_timestamp)
            && self.inner.can_still_use(level, body, timestamp)
        {
            self.inner.tick(level, body, timestamp);
        } else {
            self.do_stop(level, body, timestamp);
        }
    }

    fn do_stop(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, timestamp: i64) {
        self.status = Status::Stopped;
        self.inner.stop(level, body, timestamp);
    }

    fn debug_string(&self) -> String {
        self.inner.debug_name()
    }
}

/// Convenience for building `Map<MemoryModuleType<?>, MemoryStatus>` entry conditions in vanilla order.
#[macro_export]
macro_rules! entry_condition {
    ($($memory:expr => $status:ident),* $(,)?) => {
        vec![$(($crate::world::entity::ai::memory::MemoryTypeId::from($memory), $crate::world::entity::ai::memory::MemoryStatus::$status)),*]
    };
}
