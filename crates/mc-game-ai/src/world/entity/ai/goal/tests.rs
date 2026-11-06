//! GoalSelector arbitration (priority replacement, flag locks, disabled flags, interruptibility, tick rates).

use super::JUMP_GOAL_FLAGS;
use mc_data::EntityType;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::Entity;
use mc_game_core::world::entity::ai::goal::{
    Flag, Goal, GoalFlags, GoalSelector, reduced_tick_delay,
};
use mc_game_core::world::entity::plain::PlainEntity;
use std::cell::RefCell;
use std::rc::Rc;

type Log = Rc<RefCell<Vec<String>>>;

struct TestGoal {
    name: &'static str,
    flags: GoalFlags,
    uses: Rc<RefCell<bool>>,
    keeps: Rc<RefCell<bool>>,
    interruptable: bool,
    every_tick: bool,
    log: Log,
}

impl TestGoal {
    fn new(name: &'static str, flags: &[Flag], log: &Log) -> (Self, Rc<RefCell<bool>>) {
        let uses = Rc::new(RefCell::new(true));
        (
            Self {
                name,
                flags: GoalFlags::of(flags),
                uses: uses.clone(),
                keeps: Rc::new(RefCell::new(true)),
                interruptable: true,
                every_tick: false,
                log: log.clone(),
            },
            uses,
        )
    }
}

impl Goal for TestGoal {
    fn can_use(&mut self, _m: &mut dyn Entity, _l: &mut ServerLevel) -> bool {
        *self.uses.borrow()
    }
    fn can_continue_to_use(&mut self, _m: &mut dyn Entity, _l: &mut ServerLevel) -> bool {
        *self.keeps.borrow()
    }
    fn is_interruptable(&self) -> bool {
        self.interruptable
    }
    fn start(&mut self, _m: &mut dyn Entity, _l: &mut ServerLevel) {
        self.log.borrow_mut().push(format!("start {}", self.name));
    }
    fn stop(&mut self, _m: &mut dyn Entity, _l: &mut ServerLevel) {
        self.log.borrow_mut().push(format!("stop {}", self.name));
    }
    fn requires_update_every_tick(&self) -> bool {
        self.every_tick
    }
    fn tick(&mut self, _m: &mut dyn Entity, _l: &mut ServerLevel) {
        self.log.borrow_mut().push(format!("tick {}", self.name));
    }
    fn flags(&self) -> GoalFlags {
        self.flags
    }
}

fn setup() -> (PlainEntity, ServerLevel, Log) {
    (
        PlainEntity::new(EntityType::PIG, 1),
        ServerLevel::detached(mc_core::ident::Identifier::new("minecraft", "overworld").unwrap()),
        Log::default(),
    )
}

fn take(log: &Log) -> Vec<String> {
    std::mem::take(&mut *log.borrow_mut())
}

#[test]
fn lower_priority_number_replaces_running_goal_sharing_a_flag() {
    let (mut mob, mut level, log) = setup();
    let mut sel = GoalSelector::new();
    let (low, low_uses) = TestGoal::new("low", &[Flag::Move], &log);
    sel.add_goal(5, Box::new(low));
    sel.tick(&mut mob, &mut level);
    assert_eq!(take(&log), ["start low", "tick low"]);
    // a more important goal (smaller number) becomes usable: replaces `low`
    let (high, _) = TestGoal::new("high", &[Flag::Move, Flag::Look], &log);
    sel.add_goal(1, Box::new(high));
    *low_uses.borrow_mut() = true;
    sel.tick(&mut mob, &mut level);
    assert_eq!(take(&log), ["stop low", "start high", "tick high"]);
}

#[test]
fn equal_or_higher_number_cannot_replace_and_non_overlapping_flags_run_together() {
    let (mut mob, mut level, log) = setup();
    let mut sel = GoalSelector::new();
    let (a, _) = TestGoal::new("a", &[Flag::Move], &log);
    let (b, _) = TestGoal::new("b", &[Flag::Move], &log);
    let (c, _) = TestGoal::new("c", &[Flag::Look], &log);
    sel.add_goal(3, Box::new(a));
    sel.add_goal(3, Box::new(b));
    sel.add_goal(9, Box::new(c));
    sel.tick(&mut mob, &mut level);
    // insertion order decides: a gets MOVE, b (same priority) cannot replace it, c uses the free LOOK flag
    assert_eq!(take(&log), ["start a", "start c", "tick a", "tick c"]);
}

#[test]
fn uninterruptible_goal_keeps_its_flags() {
    let (mut mob, mut level, log) = setup();
    let mut sel = GoalSelector::new();
    let (mut a, _) = TestGoal::new("a", &[Flag::Move], &log);
    a.interruptable = false;
    sel.add_goal(5, Box::new(a));
    sel.tick(&mut mob, &mut level);
    take(&log);
    let (b, _) = TestGoal::new("b", &[Flag::Move], &log);
    sel.add_goal(0, Box::new(b));
    sel.tick(&mut mob, &mut level);
    assert_eq!(take(&log), ["tick a"]);
}

#[test]
fn disabled_flag_stops_running_goal_and_blocks_start() {
    let (mut mob, mut level, log) = setup();
    let mut sel = GoalSelector::new();
    let (a, _) = TestGoal::new("a", &[Flag::Move], &log);
    sel.add_goal(1, Box::new(a));
    sel.tick(&mut mob, &mut level);
    take(&log);
    sel.disable_control_flag(Flag::Move);
    sel.tick(&mut mob, &mut level);
    assert_eq!(take(&log), ["stop a"]);
    sel.enable_control_flag(Flag::Move);
    sel.tick(&mut mob, &mut level);
    assert_eq!(take(&log), ["start a", "tick a"]);
}

#[test]
fn goal_that_cannot_continue_is_stopped_then_restarted_in_the_same_tick() {
    let (mut mob, mut level, log) = setup();
    let mut sel = GoalSelector::new();
    let (a, _) = TestGoal::new("a", &[Flag::Jump], &log);
    let keeps = a.keeps.clone();
    sel.add_goal(1, Box::new(a));
    sel.tick(&mut mob, &mut level);
    take(&log);
    *keeps.borrow_mut() = false;
    sel.tick(&mut mob, &mut level);
    assert_eq!(take(&log), ["stop a", "start a", "tick a"]);
}

#[test]
fn off_ticks_only_run_goals_requiring_every_tick_update() {
    let (mut mob, mut level, log) = setup();
    let mut sel = GoalSelector::new();
    let (a, _) = TestGoal::new("a", &[Flag::Move], &log);
    let (mut b, _) = TestGoal::new("b", &[Flag::Look], &log);
    b.every_tick = true;
    sel.add_goal(1, Box::new(a));
    sel.add_goal(1, Box::new(b));
    sel.tick(&mut mob, &mut level);
    take(&log);
    sel.tick_running_goals(&mut mob, &mut level, false);
    assert_eq!(take(&log), ["tick b"]);
}

#[test]
fn remove_all_goals_stops_running_matches() {
    let (mut mob, mut level, log) = setup();
    let mut sel = GoalSelector::new();
    let (a, _) = TestGoal::new("a", &[Flag::Move], &log);
    sel.add_goal(1, Box::new(a));
    sel.tick(&mut mob, &mut level);
    take(&log);
    sel.remove_all_goals(&mut mob, &mut level, |_| true);
    assert_eq!(take(&log), ["stop a"]);
    assert!(sel.get_available_goals().is_empty());
}

#[test]
fn reduced_tick_delay_is_ceil_half() {
    assert_eq!(
        (
            reduced_tick_delay(1),
            reduced_tick_delay(10),
            reduced_tick_delay(61)
        ),
        (1, 5, 31)
    );
}

#[test]
fn jump_goal_flags_are_move_and_jump() {
    assert_eq!(JUMP_GOAL_FLAGS, GoalFlags::of(&[Flag::Move, Flag::Jump]));
    assert!(!JUMP_GOAL_FLAGS.contains(Flag::Look));
}
