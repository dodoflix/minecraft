//! `AvoidEntityGoal<T extends LivingEntity>`.

use super::{Flag, Goal, GoalFlags};
use crate::server::level::ServerLevel;
use crate::world::entity::ai::targeting::TargetingConditions;
use crate::world::entity::ai::util::default_random_pos;
use crate::world::entity::mob::{ext, navigation};
use crate::world::entity::{Entity, EntityId};
use crate::world::level::pathfinder::Path;
use std::sync::Arc;

/// `Predicate<? super LivingEntity>` / `Class<T>` test (`instanceof`).
pub type EntityTest = Arc<dyn Fn(&dyn Entity) -> bool + Send + Sync>;

pub struct AvoidEntityGoal {
    walk_speed_modifier: f64,
    sprint_speed_modifier: f64,
    /// `toAvoid`.
    pub to_avoid: Option<EntityId>,
    pub max_dist: f32,
    /// `path`.
    pub path: Option<Path>,
    /// `avoidClass` (`T`): an `instanceof` test.
    avoid_class: EntityTest,
    avoid_entity_targeting: TargetingConditions,
}

impl AvoidEntityGoal {
    /// `AvoidEntityGoal(mob, avoidClass, maxDist, walkSpeedModifier, sprintSpeedModifier)`.
    pub fn new(avoid_class: EntityTest, max_dist: f32, walk_speed_modifier: f64, sprint_speed_modifier: f64) -> Self {
        Self::with_predicates(avoid_class, Arc::new(|_| true), max_dist, walk_speed_modifier, sprint_speed_modifier, Arc::new(ext::no_creative_or_spectator))
    }

    /// `AvoidEntityGoal(mob, avoidClass, maxDist, walkSpeedModifier, sprintSpeedModifier, predicateOnAvoidEntity)`.
    pub fn with_predicate_on_avoid_entity(avoid_class: EntityTest, max_dist: f32, walk_speed_modifier: f64, sprint_speed_modifier: f64, predicate_on_avoid_entity: EntityTest) -> Self {
        Self::with_predicates(avoid_class, Arc::new(|_| true), max_dist, walk_speed_modifier, sprint_speed_modifier, predicate_on_avoid_entity)
    }

    /// `AvoidEntityGoal(mob, avoidClass, avoidPredicate, maxDist, walkSpeedModifier, sprintSpeedModifier, predicateOnAvoidEntity)`.
    pub fn with_predicates(
        avoid_class: EntityTest,
        avoid_predicate: EntityTest,
        max_dist: f32,
        walk_speed_modifier: f64,
        sprint_speed_modifier: f64,
        predicate_on_avoid_entity: EntityTest,
    ) -> Self {
        let selector = Arc::new(move |target: &dyn Entity, _level: &ServerLevel| predicate_on_avoid_entity(target) && avoid_predicate(target));
        Self {
            walk_speed_modifier,
            sprint_speed_modifier,
            to_avoid: None,
            max_dist,
            path: None,
            avoid_class,
            avoid_entity_targeting: TargetingConditions::for_combat().range(f64::from(max_dist)).selector(Some(selector)),
        }
    }

    /// The `predicateOnAvoidEntity` lambda captures the mob (`Fox`: `!this.trusts(entity) && !this.isDefending()`): evaluated
    /// after the plain selector with the mob as `source`.
    pub fn with_source_selector(mut self, selector: crate::world::entity::ai::targeting::SourceTargetSelector) -> Self {
        self.avoid_entity_targeting = self.avoid_entity_targeting.source_selector(Some(selector));
        self
    }

    /// `canUse()`.
    pub fn can_use_impl(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        let max_dist = f64::from(self.max_dist);
        let bb = mob.base().bounding_box().inflate(max_dist, 3.0, max_dist);
        let class = &self.avoid_class;
        let candidates = level.get_entities_of_class(&bb, |e| e.living().is_some() && class(e), |_| true);
        let pos = mob.position();
        self.to_avoid = ext::get_nearest_entity(level, &candidates, &self.avoid_entity_targeting, mob, pos.x, pos.y, pos.z);
        let Some(to_avoid) = self.to_avoid else { return false };
        let to_avoid_pos = match level.get_entity(to_avoid) {
            Some(e) => e.position(),
            None => return false,
        };
        let Some(away) = default_random_pos::get_pos_away(mob, level, 16, 7, to_avoid_pos) else { return false };
        let Some(avoid) = level.get_entity(to_avoid) else { return false };
        if avoid.base().distance_to_sqr_xyz(away.x, away.y, away.z) < avoid.base().distance_to_sqr_vec(mob.position()) {
            return false;
        }
        self.path = navigation::with_own_navigation(mob, level, |n, m, l| n.create_path_xyz(m, &*l, away.x, away.y, away.z, 0)).flatten();
        self.path.is_some()
    }

    /// `canContinueToUse()` (`pathNav.isDone()` on the mob's own navigation).
    pub fn can_continue_to_use_impl(&self, mob: &dyn Entity) -> bool {
        !navigation::own_is_done(mob)
    }

    /// `start()`.
    pub fn start_impl(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        let path = self.path.clone();
        let speed = self.walk_speed_modifier;
        navigation::with_own_navigation(mob, level, |n, m, l| n.move_to_path(m, &*l, path, speed));
    }

    /// `stop()`.
    pub fn stop_impl(&mut self) {
        self.to_avoid = None;
    }

    /// `tick()`.
    pub fn tick_impl(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        let Some(avoid_pos) = self.to_avoid.and_then(|a| level.get_entity(a)).map(|a| a.position()) else { return };
        let speed = if mob.base().distance_to_sqr_vec(avoid_pos) < 49.0 { self.sprint_speed_modifier } else { self.walk_speed_modifier };
        navigation::set_speed_modifier(mob, level, speed);
    }
}

impl Goal for AvoidEntityGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.can_use_impl(mob, level)
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) -> bool {
        self.can_continue_to_use_impl(mob)
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.start_impl(mob, level);
    }

    fn stop(&mut self, _mob: &mut dyn Entity, _level: &mut ServerLevel) {
        self.stop_impl();
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.tick_impl(mob, level);
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Move])
    }
}
