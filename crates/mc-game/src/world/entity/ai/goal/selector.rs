//! `GoalSelector` + `WrappedGoal`: exact priority/flag arbitration.
//!
//! `availableGoals` is an insertion-ordered set (NOT sorted by priority): iteration order decides which goal gets a
//! contested flag first, priorities only decide who may *replace* a running goal (`canBeReplacedBy`).

use super::{Flag, Goal, GoalFlags};
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;

/// `WrappedGoal`.
pub struct WrappedGoal {
    id: u32,
    priority: i32,
    goal: Box<dyn Goal>,
    running: bool,
}

impl WrappedGoal {
    pub fn priority(&self) -> i32 {
        self.priority
    }

    pub fn is_running(&self) -> bool {
        self.running
    }

    pub fn goal(&self) -> &dyn Goal {
        &*self.goal
    }

    pub fn goal_mut(&mut self) -> &mut dyn Goal {
        &mut *self.goal
    }

    /// `canBeReplacedBy(goal)`.
    fn can_be_replaced_by(&self, priority: i32) -> bool {
        self.goal.is_interruptable() && priority < self.priority
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        if !self.running {
            self.running = true;
            self.goal.start(mob, level);
        }
    }

    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        if self.running {
            self.running = false;
            self.goal.stop(mob, level);
        }
    }
}

/// `GoalSelector`.
#[derive(Default)]
pub struct GoalSelector {
    /// `availableGoals` (insertion order).
    available_goals: Vec<WrappedGoal>,
    /// `lockedFlags`: per flag the id of the goal holding it.
    locked_flags: [Option<u32>; 4],
    /// `disabledFlags`.
    disabled_flags: GoalFlags,
    next_id: u32,
}

impl GoalSelector {
    pub fn new() -> Self {
        Self::default()
    }

    /// `addGoal(prio, goal)`.
    pub fn add_goal(&mut self, prio: i32, goal: Box<dyn Goal>) {
        let id = self.next_id;
        self.next_id += 1;
        self.available_goals.push(WrappedGoal { id, priority: prio, goal, running: false });
    }

    /// `removeAllGoals(predicate)`: running matches are stopped first.
    pub fn remove_all_goals(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel, mut predicate: impl FnMut(&dyn Goal) -> bool) {
        for g in &mut self.available_goals {
            if predicate(&*g.goal) && g.running {
                g.stop(mob, level);
            }
        }
        self.available_goals.retain(|g| !predicate(&*g.goal));
    }

    /// `getAvailableGoals()`.
    pub fn get_available_goals(&self) -> &[WrappedGoal] {
        &self.available_goals
    }

    pub fn get_available_goals_mut(&mut self) -> &mut [WrappedGoal] {
        &mut self.available_goals
    }

    /// Whether a running goal satisfies `predicate` (`getAvailableGoals()` scan for `isRunning() && instanceof`).
    pub fn is_running_where(&self, mut predicate: impl FnMut(&dyn Goal) -> bool) -> bool {
        self.available_goals.iter().any(|g| g.running && predicate(&*g.goal))
    }

    /// The first goal of concrete type `T`.
    pub fn find_goal<T: Goal>(&self) -> Option<&T> {
        self.available_goals.iter().find_map(|g| (&*g.goal as &dyn std::any::Any).downcast_ref::<T>())
    }

    pub fn find_goal_mut<T: Goal>(&mut self) -> Option<&mut T> {
        self.available_goals.iter_mut().find_map(|g| (&mut *g.goal as &mut dyn std::any::Any).downcast_mut::<T>())
    }

    pub fn disable_control_flag(&mut self, flag: Flag) {
        self.disabled_flags.insert(flag);
    }

    pub fn enable_control_flag(&mut self, flag: Flag) {
        self.disabled_flags.remove(flag);
    }

    pub fn set_control_flag(&mut self, flag: Flag, enabled: bool) {
        if enabled {
            self.enable_control_flag(flag);
        } else {
            self.disable_control_flag(flag);
        }
    }

    pub fn is_flag_disabled(&self, flag: Flag) -> bool {
        self.disabled_flags.contains(flag)
    }

    /// `lockedFlags.getOrDefault(flag, NO_GOAL).canBeReplacedBy(goal)` for all flags of `idx`.
    fn can_be_replaced_for_all_flags(&self, idx: usize) -> bool {
        let goal = &self.available_goals[idx];
        goal.goal.flags().iter().all(|flag| match self.locked_flags[flag as usize] {
            // NO_GOAL: interruptable, priority MAX
            None => true,
            Some(id) => match self.available_goals.iter().find(|g| g.id == id) {
                Some(holder) => holder.can_be_replaced_by(goal.priority),
                None => true,
            },
        })
    }

    /// `tick()`.
    pub fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        // goalCleanup
        for i in 0..self.available_goals.len() {
            if self.available_goals[i].running {
                let flags = self.available_goals[i].goal.flags();
                if flags.intersects(self.disabled_flags) || !self.available_goals[i].goal.can_continue_to_use(mob, level) {
                    self.available_goals[i].stop(mob, level);
                }
            }
        }
        for slot in &mut self.locked_flags {
            if let Some(id) = *slot {
                if !self.available_goals.iter().any(|g| g.id == id && g.running) {
                    *slot = None;
                }
            }
        }
        // goalUpdate
        for i in 0..self.available_goals.len() {
            let flags = self.available_goals[i].goal.flags();
            if !self.available_goals[i].running
                && !flags.intersects(self.disabled_flags)
                && self.can_be_replaced_for_all_flags(i)
                && self.available_goals[i].goal.can_use(mob, level)
            {
                let id = self.available_goals[i].id;
                for flag in flags.iter() {
                    if let Some(current) = self.locked_flags[flag as usize] {
                        if let Some(c) = self.available_goals.iter().position(|g| g.id == current) {
                            self.available_goals[c].stop(mob, level);
                        }
                    }
                    self.locked_flags[flag as usize] = Some(id);
                }
                self.available_goals[i].start(mob, level);
            }
        }
        self.tick_running_goals(mob, level, true);
    }

    /// `tickRunningGoals(forceTickAllRunningGoals)`.
    pub fn tick_running_goals(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel, force_tick_all_running_goals: bool) {
        for g in &mut self.available_goals {
            if g.running && (force_tick_all_running_goals || g.goal.requires_update_every_tick()) {
                g.goal.tick(mob, level);
            }
        }
    }
}
