//! Small generic behaviors of `ai/behavior` that every brain-using mob shares.

use mc_game_core::world::entity::ai::behavior::declarative::BehaviorBuilder;
use mc_game_core::world::entity::ai::behavior::{Behavior, BehaviorBase, BehaviorControl, OneShot};
use mc_game_core::entry_condition;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::ai::memory::{MemoryData, MemoryModuleType, MemoryTypeId};
use mc_game_core::world::entity::entity::Entity;
use mc_core::random::RandomSource;

/// `CountDownCooldownTicks`: counts an `Integer` memory down to 0 and erases it.
pub struct CountDownCooldownTicks {
    cooldown_ticks: MemoryModuleType<i32>,
}

impl CountDownCooldownTicks {
    /// `new CountDownCooldownTicks(cooldownTicks)`.
    pub fn new(cooldown_ticks: MemoryModuleType<i32>) -> BehaviorBase<CountDownCooldownTicks> {
        BehaviorBase::new(entry_condition![cooldown_ticks => ValuePresent], CountDownCooldownTicks { cooldown_ticks })
    }
}

impl Behavior for CountDownCooldownTicks {
    fn timed_out(&self, _timestamp: i64, _end_timestamp: i64) -> bool {
        false
    }

    fn can_still_use(&mut self, _level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) -> bool {
        body.brain().get_memory(&self.cooldown_ticks).is_some_and(|&t| t > 0)
    }

    fn tick(&mut self, _level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        let ticks = body.brain().get_memory(&self.cooldown_ticks).copied().expect("memory present while ticking");
        body.brain_mut().set_memory(&self.cooldown_ticks, ticks.wrapping_sub(1));
    }

    fn stop(&mut self, _level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        body.brain_mut().erase_memory(&self.cooldown_ticks);
    }
}

/// `EraseMemoryIf.create(predicate, memoryType)`.
pub fn erase_memory_if(mut predicate: impl FnMut(&dyn Entity) -> bool + 'static, memory_type: MemoryTypeId) -> OneShot {
    let mut i = BehaviorBuilder::instance();
    let memory = i.present_untyped(memory_type);
    i.create(move |c| {
        if predicate(c.body()) {
            c.erase_untyped(&memory);
            true
        } else {
            false
        }
    })
}

/// `CopyMemoryWithExpiry.create(copyIfTrue, sourceMemory, targetMemory, durationOfCopy)`; `durationOfCopy` is the
/// `UniformInt` bounds (inclusive), sampled with the level's random.
pub fn copy_memory_with_expiry<U: MemoryData>(
    mut copy_if_true: impl FnMut(&dyn Entity) -> bool + 'static,
    source_memory: MemoryModuleType<U>,
    target_memory: MemoryModuleType<U>,
    duration_min_inclusive: i32,
    duration_max_inclusive: i32,
) -> OneShot {
    let mut i = BehaviorBuilder::instance();
    let source = i.present(&source_memory);
    let target = i.absent(&target_memory);
    i.create(move |c| {
        if !copy_if_true(c.body()) {
            return false;
        }
        let value = c.get(&source).clone();
        let duration = c.level.tick_state.random.next_int_between_inclusive(duration_min_inclusive, duration_max_inclusive);
        c.set_with_expiry(&target, value, i64::from(duration));
        true
    })
}

/// `BecomePassiveIfMemoryPresent.create(pacifyingMemory, pacifyDuration)`.
pub fn become_passive_if_memory_present(pacifying_memory: MemoryTypeId, pacify_duration: i32) -> OneShot {
    use mc_game_core::world::entity::ai::memory::memory_module_type::{ATTACK_TARGET, PACIFIED};
    let mut i = BehaviorBuilder::instance();
    let attack_target = i.registered(&ATTACK_TARGET);
    let pacified = i.absent(&PACIFIED);
    let _pacifying = i.present_untyped(pacifying_memory);
    i.create(move |c| {
        c.set_with_expiry(&pacified, true, i64::from(pacify_duration));
        c.erase(&attack_target);
        true
    })
}

/// Boxes a behavior for `ActivityData` lists.
pub fn boxed(behavior: impl BehaviorControl) -> Box<dyn BehaviorControl> {
    Box::new(behavior)
}

/// `UpdateActivityFromSchedule.create()`.
pub fn update_activity_from_schedule() -> OneShot {
    BehaviorBuilder::point(|level: &mut ServerLevel, body: &mut dyn Entity, _: i64| {
        mc_game_core::world::entity::ai::brain::update_activity_from_schedule(level, body);
        true
    })
}
