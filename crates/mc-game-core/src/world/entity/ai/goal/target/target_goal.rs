//! `TargetGoal` (abstract base of the target-selecting goals).
//!
//! Java subclasses `TargetGoal`; here the concrete goals *embed* a `TargetGoal` and delegate (`start`/`stop`/
//! `can_continue_to_use`/`can_attack`), exactly like vanilla's `super.x()` calls. `getFollowDistance()` (overridden by
//! `Llama.LlamaHurtByTargetGoal`/`PolarBear.PolarBearHurtByTargetGoal` as `super * factor`) is [`TargetGoal::follow_distance_factor`].

use crate::server::level::ServerLevel;
use crate::world::entity::ai::goal::reduced_tick_delay;
use crate::world::entity::ai::targeting::TargetingConditions;
use crate::world::entity::mob::{combat, navigation, tick};
use crate::world::entity::{Entity, EntityId};
use mc_core::random::RandomSource;
use mc_data::Attribute;

const EMPTY_REACH_CACHE: i32 = 0;
const CAN_REACH_CACHE: i32 = 1;
const CANT_REACH_CACHE: i32 = 2;

pub struct TargetGoal {
    pub must_see: bool,
    must_reach: bool,
    reach_cache: i32,
    reach_cache_time: i32,
    unseen_ticks: i32,
    /// `targetMob`.
    pub target_mob: Option<EntityId>,
    /// `unseenMemoryTicks`.
    pub unseen_memory_ticks: i32,
    /// Multiplier a subclass applies in its `getFollowDistance()` override (`super.getFollowDistance() * factor`).
    pub follow_distance_factor: f64,
}

impl TargetGoal {
    /// `TargetGoal(mob, mustSee)` / `TargetGoal(mob, mustSee, mustReach)`.
    pub fn new(must_see: bool, must_reach: bool) -> Self {
        Self { must_see, must_reach, reach_cache: EMPTY_REACH_CACHE, reach_cache_time: 0, unseen_ticks: 0, target_mob: None, unseen_memory_ticks: 60, follow_distance_factor: 1.0 }
    }

    /// `setUnseenMemoryTicks(unseenMemoryTicks)`.
    pub fn set_unseen_memory_ticks(mut self, unseen_memory_ticks: i32) -> Self {
        self.unseen_memory_ticks = unseen_memory_ticks;
        self
    }

    /// `getFollowDistance()`.
    pub fn get_follow_distance(&self, mob: &dyn Entity) -> f64 {
        let base = mob.living().expect("living mob").get_attribute_value(Attribute::FOLLOW_RANGE);
        if self.follow_distance_factor == 1.0 { base } else { base * self.follow_distance_factor }
    }

    /// `canContinueToUse()`.
    pub fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        let Some(target_id) = tick::get_target_checked(mob, level).or(self.target_mob) else { return false };
        let Some(target) = level.get_entity(target_id) else { return false };
        if !mob.can_attack(level, target) {
            return false;
        }
        let mob_team = combat::get_team_name(level, mob);
        if mob_team.is_some() && combat::get_team_name(level, target) == mob_team {
            return false;
        }
        let within = self.get_follow_distance(mob);
        if mob.base().distance_to_sqr_vec(target.position()) > within * within {
            return false;
        }
        if self.must_see {
            if mob.mob().expect("mob").sensing.has_line_of_sight(level, mob, target) {
                self.unseen_ticks = 0;
            } else {
                self.unseen_ticks += 1;
                if self.unseen_ticks > reduced_tick_delay(self.unseen_memory_ticks) {
                    return false;
                }
            }
        }
        mob.set_target(level, Some(target_id));
        true
    }

    /// `start()`.
    pub fn start(&mut self) {
        self.reach_cache = 0;
        self.reach_cache_time = 0;
        self.unseen_ticks = 0;
    }

    /// `stop()`.
    pub fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        mob.set_target(level, None);
        self.target_mob = None;
    }

    /// `canAttack(target, targetConditions)`.
    pub fn can_attack(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel, target: Option<EntityId>, target_conditions: &TargetingConditions) -> bool {
        let Some(target_id) = target else { return false };
        let Some(target) = level.get_entity(target_id) else { return false };
        if !target_conditions.test(level, Some(&*mob), target) {
            return false;
        }
        if !mob.mob().expect("mob").is_within_home_pos(target.block_position()) {
            return false;
        }
        if self.must_reach {
            self.reach_cache_time -= 1;
            if self.reach_cache_time <= 0 {
                self.reach_cache = 0;
            }
            if self.reach_cache == EMPTY_REACH_CACHE {
                self.reach_cache = if self.can_reach(mob, level, target_id) { CAN_REACH_CACHE } else { CANT_REACH_CACHE };
            }
            if self.reach_cache == CANT_REACH_CACHE {
                return false;
            }
        }
        true
    }

    /// `canReach(target)`.
    fn can_reach(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel, target_id: EntityId) -> bool {
        self.reach_cache_time = reduced_tick_delay(10 + mob.base_mut().random.next_int_bounded(5));
        let Some(target_pos) = level.get_entity(target_id).map(|t| t.block_position()) else { return false };
        let Some(path) = navigation::create_path_entity(mob, level, target_pos, 0) else { return false };
        let Some(last) = path.get_end_node() else { return false };
        let xx = last.x - target_pos.x();
        let zz = last.z - target_pos.z();
        f64::from(xx.wrapping_mul(xx).wrapping_add(zz.wrapping_mul(zz))) <= 2.25
    }
}
