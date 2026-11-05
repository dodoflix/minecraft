//! `net.minecraft.world.entity.ai.goal`: [`Goal`], [`GoalFlags`] (`Goal.Flag` set), `WrappedGoal` and [`GoalSelector`],
//! plus the concrete goals (one module each).
//!
//! Goals hold no back-reference to their mob: every callback takes `(mob, level)`. The selector is `mem::take`n out of
//! the mob's `MobData` while it runs, so goals may freely use `mob.mob_mut()` for everything except the two goal
//! selectors themselves. A goal that needs the concrete mob type does `mob.as_any().downcast_ref::<Chicken>()`.

mod abstract_horse_access;
mod avoid_entity;
mod breath_air;
mod climb_on_top_of_powder_snow;
mod flee_sun;
mod jump_goal;
mod move_towards_restriction;
mod random_stand;
mod random_swimming;
mod restrict_sun;
mod run_around_like_crazy;
mod try_find_liquid;
pub mod move_to_block;
mod breed;
mod dolphin_jump_goal;
mod eat_block;
mod float;
mod follow_parent;
mod follow_player_ridden_entity_goal;
mod look_at_player;
mod melee_attack;
mod panic;
mod random_look_around;
mod ranged_attack;
mod random_stroll;
mod selector;
mod swell;
pub mod target;
mod tempt;
mod water_avoiding_random_stroll;
mod zombie_attack;

pub use abstract_horse_access::AbstractHorseAccess;
pub use avoid_entity::{AvoidEntityGoal, EntityTest};
pub use breath_air::BreathAirGoal;
pub use climb_on_top_of_powder_snow::ClimbOnTopOfPowderSnowGoal;
pub use flee_sun::FleeSunGoal;
pub use jump_goal::JUMP_GOAL_FLAGS;
pub use move_to_block::{MoveToBlock, MoveToBlockData};
pub use move_towards_restriction::MoveTowardsRestrictionGoal;
pub use random_stand::RandomStandGoal;
pub use random_swimming::{RandomSwimmingGoal, get_random_swimmable_pos};
pub use restrict_sun::RestrictSunGoal;
pub use run_around_like_crazy::RunAroundLikeCrazyGoal;
pub use try_find_liquid::TryFindLiquidGoal;
pub use breed::BreedGoal;
pub use dolphin_jump_goal::DolphinJumpGoal;
pub use eat_block::EatBlockGoal;
pub use float::FloatGoal;
pub use follow_parent::FollowParentGoal;
pub use follow_player_ridden_entity_goal::FollowPlayerRiddenEntityGoal;
pub use look_at_player::LookAtPlayerGoal;
pub use melee_attack::MeleeAttackGoal;
pub use panic::PanicGoal;
pub use random_look_around::RandomLookAroundGoal;
pub use ranged_attack::RangedAttackGoal;
pub use random_stroll::RandomStrollGoal;
pub use selector::{GoalSelector, WrappedGoal};
pub use swell::SwellGoal;
pub use tempt::TemptGoal;
pub use water_avoiding_random_stroll::WaterAvoidingRandomStrollGoal;
pub use zombie_attack::ZombieAttackGoal;

use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use mc_core::math::mth;
use std::any::Any;

/// `Goal.Flag`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Flag {
    Move = 0,
    Look = 1,
    Jump = 2,
    Target = 3,
}

impl Flag {
    pub const ALL: [Flag; 4] = [Flag::Move, Flag::Look, Flag::Jump, Flag::Target];
}

/// `EnumSet<Goal.Flag>`.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub struct GoalFlags(u8);

impl GoalFlags {
    pub const NONE: GoalFlags = GoalFlags(0);

    pub const fn of(flags: &[Flag]) -> Self {
        let mut bits = 0;
        let mut i = 0;
        while i < flags.len() {
            bits |= 1 << flags[i] as u8;
            i += 1;
        }
        GoalFlags(bits)
    }

    pub fn contains(self, flag: Flag) -> bool {
        self.0 & (1 << flag as u8) != 0
    }

    pub fn insert(&mut self, flag: Flag) {
        self.0 |= 1 << flag as u8;
    }

    pub fn remove(&mut self, flag: Flag) {
        self.0 &= !(1 << flag as u8);
    }

    pub fn intersects(self, other: GoalFlags) -> bool {
        self.0 & other.0 != 0
    }

    /// Iteration in `EnumSet` (ordinal) order.
    pub fn iter(self) -> impl Iterator<Item = Flag> {
        Flag::ALL.into_iter().filter(move |f| self.contains(*f))
    }
}

/// `Goal.reducedTickDelay(ticks)`.
pub fn reduced_tick_delay(ticks: i32) -> i32 {
    mth::positive_ceil_div_i32(ticks, 2)
}

/// `net.minecraft.world.entity.ai.goal.Goal`. `can_use` is `&mut self` like Java (goals keep state and use RNG there).
/// `flags` is vanilla's `setFlags(...)` in the constructor.
pub trait Goal: Any {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool;

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.can_use(mob, level)
    }

    fn is_interruptable(&self) -> bool {
        true
    }

    fn start(&mut self, _mob: &mut dyn Entity, _level: &mut ServerLevel) {}

    fn stop(&mut self, _mob: &mut dyn Entity, _level: &mut ServerLevel) {}

    fn requires_update_every_tick(&self) -> bool {
        false
    }

    fn tick(&mut self, _mob: &mut dyn Entity, _level: &mut ServerLevel) {}

    fn flags(&self) -> GoalFlags;

    /// `getClass().getSimpleName()` (debug output, `PanicGoal` checks use downcasting instead).
    fn name(&self) -> &'static str {
        std::any::type_name::<Self>().rsplit("::").next().unwrap_or("Goal")
    }

    /// `this instanceof PanicGoal` (subclasses return their embedded `PanicGoal`).
    fn as_panic_goal(&self) -> Option<&PanicGoal> {
        None
    }

    /// `adjustedTickDelay(ticks)`.
    fn adjusted_tick_delay(&self, ticks: i32) -> i32 {
        if self.requires_update_every_tick() { ticks } else { reduced_tick_delay(ticks) }
    }
}

#[cfg(test)]
mod tests;
