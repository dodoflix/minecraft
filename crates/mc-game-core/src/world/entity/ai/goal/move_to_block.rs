//! `MoveToBlockGoal` (abstract). A concrete goal embeds a [`MoveToBlockData`] (the Java fields), implements
//! [`MoveToBlock`] (the overridable methods; `is_valid_target` is the abstract one) and writes its [`Goal`] impl in terms
//! of the free functions of this module, which are the non-virtual bodies of the Java class (call them for `super.xyz()`).

use super::{Goal, reduced_tick_delay};
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::mob::navigation;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;

pub const GIVE_UP_TICKS: i32 = 1200;
pub const STAY_TICKS: i32 = 1200;
pub const INTERVAL_TICKS: i32 = 200;

/// The fields of `MoveToBlockGoal`.
#[derive(Clone, Debug)]
pub struct MoveToBlockData {
    pub speed_modifier: f64,
    pub next_start_tick: i32,
    pub try_ticks: i32,
    max_stay_ticks: i32,
    pub block_pos: BlockPos,
    reached_target: bool,
    search_range: i32,
    vertical_search_range: i32,
    pub vertical_search_start: i32,
}

impl MoveToBlockData {
    /// `MoveToBlockGoal(mob, speedModifier, searchRange)`.
    pub fn new(speed_modifier: f64, search_range: i32) -> Self {
        Self::with_vertical(speed_modifier, search_range, 1)
    }

    /// `MoveToBlockGoal(mob, speedModifier, searchRange, verticalSearchRange)`.
    pub fn with_vertical(
        speed_modifier: f64,
        search_range: i32,
        vertical_search_range: i32,
    ) -> Self {
        Self {
            speed_modifier,
            next_start_tick: 0,
            try_ticks: 0,
            max_stay_ticks: 0,
            block_pos: BlockPos::ZERO,
            reached_target: false,
            search_range,
            vertical_search_range,
            vertical_search_start: 0,
        }
    }
}

/// The overridable surface of `MoveToBlockGoal`; defaults are the vanilla bodies.
pub trait MoveToBlock: Goal {
    fn mtb(&self) -> &MoveToBlockData;
    fn mtb_mut(&mut self) -> &mut MoveToBlockData;

    /// `isValidTarget(level, pos)` (abstract).
    fn is_valid_target(&self, mob: &dyn Entity, level: &ServerLevel, pos: BlockPos) -> bool;

    /// `nextStartTick(mob)`.
    fn next_start_tick(&self, mob: &mut dyn Entity) -> i32 {
        reduced_tick_delay(200 + mob.base_mut().random.next_int_bounded(200))
    }

    /// `acceptedDistance()`.
    fn accepted_distance(&self) -> f64 {
        1.0
    }

    /// `getMoveToTarget()`.
    fn get_move_to_target(&self) -> BlockPos {
        self.mtb().block_pos.above()
    }

    /// `shouldRecalculatePath()`.
    fn should_recalculate_path(&self) -> bool {
        self.mtb().try_ticks % 40 == 0
    }

    /// `moveMobToBlock()`.
    fn move_mob_to_block(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        move_mob_to_block(self, mob, level);
    }

    /// `findNearestBlock()`.
    fn find_nearest_block(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        find_nearest_block(self, mob, level)
    }

    /// `isReachedTarget()`.
    fn is_reached_target(&self) -> bool {
        self.mtb().reached_target
    }

    /// `blockPos`.
    fn block_pos(&self) -> BlockPos {
        self.mtb().block_pos
    }
}

/// `canUse()`.
pub fn can_use<G: MoveToBlock + ?Sized>(
    goal: &mut G,
    mob: &mut dyn Entity,
    level: &mut ServerLevel,
) -> bool {
    if goal.mtb().next_start_tick > 0 {
        goal.mtb_mut().next_start_tick -= 1;
        false
    } else {
        let next = goal.next_start_tick(mob);
        goal.mtb_mut().next_start_tick = next;
        goal.find_nearest_block(mob, level)
    }
}

/// `canContinueToUse()`.
pub fn can_continue_to_use<G: MoveToBlock + ?Sized>(
    goal: &G,
    mob: &dyn Entity,
    level: &ServerLevel,
) -> bool {
    let d = goal.mtb();
    d.try_ticks >= -d.max_stay_ticks
        && d.try_ticks <= GIVE_UP_TICKS
        && goal.is_valid_target(mob, level, d.block_pos)
}

/// `start()`.
pub fn start<G: MoveToBlock + ?Sized>(goal: &mut G, mob: &mut dyn Entity, level: &mut ServerLevel) {
    goal.move_mob_to_block(mob, level);
    goal.mtb_mut().try_ticks = 0;
    let inner = mob.base_mut().random.next_int_bounded(1200) + 1200;
    goal.mtb_mut().max_stay_ticks = mob.base_mut().random.next_int_bounded(inner) + 1200;
}

/// `moveMobToBlock()`.
pub fn move_mob_to_block<G: MoveToBlock + ?Sized>(
    goal: &G,
    mob: &mut dyn Entity,
    level: &mut ServerLevel,
) {
    let d = goal.mtb();
    navigation::move_to(
        mob,
        level,
        f64::from(d.block_pos.x()) + 0.5,
        f64::from(d.block_pos.y() + 1),
        f64::from(d.block_pos.z()) + 0.5,
        d.speed_modifier,
    );
}

/// `tick()`.
pub fn tick<G: MoveToBlock + ?Sized>(goal: &mut G, mob: &mut dyn Entity, level: &mut ServerLevel) {
    let move_to_target = goal.get_move_to_target();
    if !move_to_target.closer_to_center_than(&mob.position(), goal.accepted_distance()) {
        goal.mtb_mut().reached_target = false;
        goal.mtb_mut().try_ticks += 1;
        if goal.should_recalculate_path() {
            let speed = goal.mtb().speed_modifier;
            navigation::move_to(
                mob,
                level,
                f64::from(move_to_target.x()) + 0.5,
                f64::from(move_to_target.y()),
                f64::from(move_to_target.z()) + 0.5,
                speed,
            );
        }
    } else {
        goal.mtb_mut().reached_target = true;
        goal.mtb_mut().try_ticks -= 1;
    }
}

/// `findNearestBlock()`.
pub fn find_nearest_block<G: MoveToBlock + ?Sized>(
    goal: &mut G,
    mob: &mut dyn Entity,
    level: &mut ServerLevel,
) -> bool {
    let horizontal_search = goal.mtb().search_range;
    let vertical_search = goal.mtb().vertical_search_range;
    let mob_pos = mob.block_position();
    let mut y = goal.mtb().vertical_search_start;
    while y <= vertical_search {
        for r in 0..horizontal_search {
            let mut x = 0;
            while x <= r {
                let mut z = if x < r && x > -r { r } else { 0 };
                while z <= r {
                    let pos = mob_pos.offset(x, y - 1, z);
                    if mob.mob().is_some_and(|m| m.is_within_home_pos(pos))
                        && goal.is_valid_target(mob, level, pos)
                    {
                        goal.mtb_mut().block_pos = pos;
                        return true;
                    }
                    z = if z > 0 { -z } else { 1 - z };
                }
                x = if x > 0 { -x } else { 1 - x };
            }
        }
        y = if y > 0 { -y } else { 1 - y };
    }
    false
}
