//! `net.minecraft.world.entity.ai.targeting.TargetingConditions`.

use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use mc_world::level_storage::Difficulty;
use std::sync::Arc;

/// `TargetingConditions.Selector`: `test(target, level)`.
pub type TargetSelector = Arc<dyn Fn(&dyn Entity, &ServerLevel) -> bool + Send + Sync>;

/// A `Selector` lambda that reads the targeting mob (`(entity, level) -> bee.isAngryAt(entity, level)`): `test(source, target, level)`.
pub type SourceTargetSelector = Arc<dyn Fn(&dyn Entity, &dyn Entity, &ServerLevel) -> bool + Send + Sync>;

#[derive(Clone)]
pub struct TargetingConditions {
    is_combat: bool,
    range: f64,
    check_line_of_sight: bool,
    test_invisible: bool,
    selector: Option<TargetSelector>,
    source_selector: Option<SourceTargetSelector>,
}

impl TargetingConditions {
    fn new(is_combat: bool) -> Self {
        Self { is_combat, range: -1.0, check_line_of_sight: true, test_invisible: true, selector: None, source_selector: None }
    }

    /// `forCombat()` (also `DEFAULT`).
    pub fn for_combat() -> Self {
        Self::new(true)
    }

    pub fn for_non_combat() -> Self {
        Self::new(false)
    }

    pub fn range(mut self, range: f64) -> Self {
        self.range = range;
        self
    }

    pub fn ignore_line_of_sight(mut self) -> Self {
        self.check_line_of_sight = false;
        self
    }

    pub fn ignore_invisibility_testing(mut self) -> Self {
        self.test_invisible = false;
        self
    }

    pub fn selector(mut self, selector: Option<TargetSelector>) -> Self {
        self.selector = selector;
        self
    }

    /// `selector(...)` for a lambda that captures the targeting mob (evaluated right after the plain selector).
    pub fn source_selector(mut self, selector: Option<SourceTargetSelector>) -> Self {
        self.source_selector = selector;
        self
    }

    /// `test(level, targeter, target)`: `targeter` is the mob doing the targeting (`None` = no source).
    pub fn test(&self, level: &ServerLevel, targeter: Option<&dyn Entity>, target: &dyn Entity) -> bool {
        if targeter.is_some_and(|t| t.id() == target.id()) {
            return false;
        }
        if !target.can_be_seen_by_anyone() {
            return false;
        }
        if self.selector.as_ref().is_some_and(|s| !s(target, level)) {
            return false;
        }
        if let (Some(s), Some(source)) = (self.source_selector.as_ref(), targeter) {
            if !s(source, target, level) {
                return false;
            }
        }
        match targeter {
            None => {
                if self.is_combat && (!target.can_be_seen_as_enemy() || level.difficulty() == Difficulty::Peaceful) {
                    return false;
                }
            }
            Some(targeter) => {
                if self.is_combat && (!targeter.can_attack(level, target) || crate::world::entity::mob::ext::is_allied_to(level, targeter, target)) {
                    return false;
                }
                if self.range > 0.0 {
                    let modifier = if self.test_invisible { target.get_visibility_percent(level, Some(targeter)) } else { 1.0 };
                    let visibility_distance = (self.range * modifier).max(2.0);
                    let distance_to_sqr = targeter.base().distance_to_sqr_xyz(target.base().x(), target.base().y(), target.base().z());
                    if distance_to_sqr > visibility_distance * visibility_distance {
                        return false;
                    }
                }
                if self.check_line_of_sight {
                    if let Some(mob) = targeter.mob() {
                        if !mob.sensing.has_line_of_sight(level, targeter, target) {
                            return false;
                        }
                    }
                }
            }
        }
        true
    }
}

/// `TargetingConditions.DEFAULT`.
pub fn default_conditions() -> TargetingConditions {
    TargetingConditions::for_combat()
}
