//! `NearestHealableRaiderTargetGoal` (a witch of an active raid looks for a raider to heal).

use mc_core::random::RandomSource;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::Entity;
use mc_game_core::world::entity::ai::goal::target::NearestAttackableTargetGoal;
use mc_game_core::world::entity::ai::goal::{Goal, GoalFlags, reduced_tick_delay};
use mc_game_core::world::entity::raid::raider::has_active_raid;
use std::sync::Arc;
use std::sync::atomic::{AtomicI32, Ordering};

const DEFAULT_COOLDOWN: i32 = 200;

pub struct NearestHealableRaiderTargetGoal {
    inner: NearestAttackableTargetGoal,
    /// `cooldown`, shared with the witch (`getCooldown`/`decrementCooldown`).
    cooldown: Arc<AtomicI32>,
}

impl NearestHealableRaiderTargetGoal {
    /// `NearestHealableRaiderTargetGoal(raider, targetType, mustSee, subselector)`: `inner` is built with a random interval of 500 and
    /// no `mustReach` ([`NearestAttackableTargetGoal::build`]).
    pub fn new(inner: NearestAttackableTargetGoal, cooldown: Arc<AtomicI32>) -> Self {
        Self { inner, cooldown }
    }
}

impl Goal for NearestHealableRaiderTargetGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        if self.cooldown.load(Ordering::Relaxed) > 0
            || !mob.base_mut().random.next_boolean()
            || !has_active_raid(mob, level)
        {
            false
        } else {
            self.inner.find_target(mob, level);
            self.inner.target.is_some()
        }
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.inner.can_continue_to_use(mob, level)
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.cooldown
            .store(reduced_tick_delay(DEFAULT_COOLDOWN), Ordering::Relaxed);
        self.inner.start(mob, level);
    }

    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.inner.stop(mob, level);
    }

    fn flags(&self) -> GoalFlags {
        self.inner.flags()
    }
}
