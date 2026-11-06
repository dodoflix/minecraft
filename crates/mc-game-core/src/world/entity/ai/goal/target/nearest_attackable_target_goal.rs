//! `NearestAttackableTargetGoal`.

use super::TargetType;
use super::target_goal::TargetGoal;
use crate::server::level::ServerLevel;
use crate::world::entity::ai::goal::{Flag, Goal, GoalFlags, reduced_tick_delay};
use crate::world::entity::ai::targeting::{
    SourceTargetSelector, TargetSelector, TargetingConditions,
};
use crate::world::entity::mob::ext;
use crate::world::entity::{Entity, EntityId};
use mc_core::phys::AABB;
use mc_core::random::RandomSource;

pub const DEFAULT_RANDOM_INTERVAL: i32 = 10;

/// `getTargetSearchArea(followDistance)` override (`Shulker`).
pub type SearchAreaFn = Box<dyn Fn(&dyn Entity, f64) -> AABB + Send + Sync>;

pub struct NearestAttackableTargetGoal {
    pub target_goal: TargetGoal,
    pub target_type: TargetType,
    pub random_interval: i32,
    /// `target`.
    pub target: Option<EntityId>,
    pub target_conditions: TargetingConditions,
    /// Overrides `getTargetSearchArea` (default: the mob's bounding box inflated by the follow distance).
    pub search_area: Option<SearchAreaFn>,
}

impl NearestAttackableTargetGoal {
    /// `NearestAttackableTargetGoal(mob, targetType, mustSee)`.
    pub fn new(target_type: TargetType, must_see: bool) -> Self {
        Self::build(target_type, DEFAULT_RANDOM_INTERVAL, must_see, false, None)
    }

    /// `NearestAttackableTargetGoal(mob, targetType, mustSee, selector)`.
    pub fn with_selector(
        target_type: TargetType,
        must_see: bool,
        selector: TargetSelector,
    ) -> Self {
        Self::build(
            target_type,
            DEFAULT_RANDOM_INTERVAL,
            must_see,
            false,
            Some(selector),
        )
    }

    /// `NearestAttackableTargetGoal(mob, targetType, mustSee, mustReach)`.
    pub fn with_must_reach(target_type: TargetType, must_see: bool, must_reach: bool) -> Self {
        Self::build(
            target_type,
            DEFAULT_RANDOM_INTERVAL,
            must_see,
            must_reach,
            None,
        )
    }

    /// `NearestAttackableTargetGoal(mob, targetType, randomInterval, mustSee, mustReach, selector)`.
    pub fn build(
        target_type: TargetType,
        random_interval: i32,
        must_see: bool,
        must_reach: bool,
        selector: Option<TargetSelector>,
    ) -> Self {
        Self {
            target_goal: TargetGoal::new(must_see, must_reach),
            target_type,
            random_interval: reduced_tick_delay(random_interval),
            target: None,
            // `.range(getFollowDistance())` is re-applied by every `getTargetConditions()`
            target_conditions: TargetingConditions::for_combat().selector(selector),
            search_area: None,
        }
    }

    /// The selector lambda captures the mob (`Bee`): evaluated as `selector.test(target, level)` with the mob as receiver.
    pub fn with_source_selector(mut self, selector: SourceTargetSelector) -> Self {
        self.target_conditions = self.target_conditions.source_selector(Some(selector));
        self
    }

    /// `setUnseenMemoryTicks(unseenMemoryTicks)`.
    pub fn set_unseen_memory_ticks(mut self, unseen_memory_ticks: i32) -> Self {
        self.target_goal.unseen_memory_ticks = unseen_memory_ticks;
        self
    }

    /// `setTarget(target)`.
    pub fn set_target(&mut self, target: Option<EntityId>) {
        self.target = target;
    }

    /// `getTargetSearchArea(followDistance)`.
    pub fn get_target_search_area(&self, mob: &dyn Entity, follow_distance: f64) -> AABB {
        match &self.search_area {
            Some(f) => f(mob, follow_distance),
            None => {
                mob.base()
                    .bounding_box()
                    .inflate(follow_distance, follow_distance, follow_distance)
            }
        }
    }

    /// `findTarget()`.
    pub fn find_target(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        let follow = self.target_goal.get_follow_distance(mob);
        // `getTargetConditions()` = `targetConditions.range(getFollowDistance())`
        self.target_conditions = self.target_conditions.clone().range(follow);
        let (x, eye_y, z) = (mob.base().x(), mob.base().get_eye_y(), mob.base().z());
        self.target = match &self.target_type {
            TargetType::Player => {
                ext::get_nearest_player(level, &self.target_conditions, mob, x, eye_y, z)
            }
            TargetType::Class(class) => {
                let area =
                    self.get_target_search_area(mob, self.target_goal.get_follow_distance(mob));
                let candidates = level.get_entities_of_class(
                    &area,
                    |e| e.living().is_some() && class(e),
                    |_| true,
                );
                ext::get_nearest_entity(
                    level,
                    &candidates,
                    &self.target_conditions,
                    mob,
                    x,
                    eye_y,
                    z,
                )
            }
        };
    }

    /// The goal's `canUse` body (subclass-callable `super.canUse()`).
    pub fn can_use_impl(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        if self.random_interval > 0
            && mob.base_mut().random.next_int_bounded(self.random_interval) != 0
        {
            false
        } else {
            self.find_target(mob, level);
            self.target.is_some()
        }
    }
}

impl Goal for NearestAttackableTargetGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.can_use_impl(mob, level)
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.target_goal.can_continue_to_use(mob, level)
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        mob.set_target(level, self.target);
        self.target_goal.start();
    }

    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.target_goal.stop(mob, level);
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Target])
    }
}
