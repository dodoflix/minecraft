//! `GateBehavior<E>` and `RunOne<E>`.

use super::shuffling_list::ShufflingList;
use super::{BehaviorControl, Status};
use crate::server::level::ServerLevel;
use crate::world::entity::ai::memory::{MemoryStatus, MemoryTypeId};
use crate::world::entity::entity::Entity;

/// `GateBehavior.OrderPolicy`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OrderPolicy {
    Ordered,
    Shuffled,
}

impl OrderPolicy {
    /// `apply(list)`.
    pub fn apply<U>(self, list: &mut ShufflingList<U>) {
        if self == OrderPolicy::Shuffled {
            list.shuffle();
        }
    }
}

/// `GateBehavior.RunningPolicy`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RunningPolicy {
    RunOne,
    TryAll,
}

impl RunningPolicy {
    /// `apply(behaviors, level, body, timestamp)`: tries the stopped ones in order (`RunOne` stops at the first that starts).
    pub fn apply(
        self,
        behaviors: &mut ShufflingList<Box<dyn BehaviorControl>>,
        level: &mut ServerLevel,
        body: &mut dyn Entity,
        timestamp: i64,
    ) {
        for behavior in behaviors.iter_mut() {
            if behavior.get_status() == Status::Stopped
                && behavior.try_start(level, body, timestamp)
                && self == RunningPolicy::RunOne
            {
                break;
            }
        }
    }
}

pub struct GateBehavior {
    entry_condition: Vec<(MemoryTypeId, MemoryStatus)>,
    exit_erased_memories: Vec<MemoryTypeId>,
    order_policy: OrderPolicy,
    running_policy: RunningPolicy,
    behaviors: ShufflingList<Box<dyn BehaviorControl>>,
    status: Status,
    name: &'static str,
}

impl GateBehavior {
    /// `new GateBehavior(entryCondition, exitErasedMemories, orderPolicy, runningPolicy, weightedBehaviors)`.
    pub fn new(
        entry_condition: Vec<(MemoryTypeId, MemoryStatus)>,
        exit_erased_memories: Vec<MemoryTypeId>,
        order_policy: OrderPolicy,
        running_policy: RunningPolicy,
        behaviors: Vec<(Box<dyn BehaviorControl>, i32)>,
    ) -> Self {
        Self::with_list(
            entry_condition,
            exit_erased_memories,
            order_policy,
            running_policy,
            behaviors,
            ShufflingList::new(),
            "GateBehavior",
        )
    }

    /// As [`GateBehavior::new`] with the shuffling list's random source chosen by the caller (tests).
    pub fn with_list(
        entry_condition: Vec<(MemoryTypeId, MemoryStatus)>,
        exit_erased_memories: Vec<MemoryTypeId>,
        order_policy: OrderPolicy,
        running_policy: RunningPolicy,
        behaviors: Vec<(Box<dyn BehaviorControl>, i32)>,
        mut list: ShufflingList<Box<dyn BehaviorControl>>,
        name: &'static str,
    ) -> Self {
        for (behavior, weight) in behaviors {
            list.add(behavior, weight);
        }
        Self {
            entry_condition,
            exit_erased_memories,
            order_policy,
            running_policy,
            behaviors: list,
            status: Status::Stopped,
            name,
        }
    }

    fn has_required_memories(&self, body: &dyn Entity) -> bool {
        self.entry_condition
            .iter()
            .all(|&(t, s)| body.brain().check_memory(t, s))
    }
}

impl BehaviorControl for GateBehavior {
    fn get_status(&self) -> Status {
        self.status
    }

    fn get_required_memories(&self) -> Vec<MemoryTypeId> {
        let mut memories: Vec<MemoryTypeId> =
            self.entry_condition.iter().map(|&(t, _)| t).collect();
        for behavior in self.behaviors.iter() {
            for m in behavior.get_required_memories() {
                if !memories.contains(&m) {
                    memories.push(m);
                }
            }
        }
        memories
    }

    fn try_start(
        &mut self,
        level: &mut ServerLevel,
        body: &mut dyn Entity,
        timestamp: i64,
    ) -> bool {
        if self.has_required_memories(body) {
            self.status = Status::Running;
            self.order_policy.apply(&mut self.behaviors);
            self.running_policy
                .apply(&mut self.behaviors, level, body, timestamp);
            true
        } else {
            false
        }
    }

    fn tick_or_stop(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, timestamp: i64) {
        for goal in self.behaviors.iter_mut() {
            if goal.get_status() == Status::Running {
                goal.tick_or_stop(level, body, timestamp);
            }
        }
        if !self
            .behaviors
            .iter()
            .any(|g| g.get_status() == Status::Running)
        {
            self.do_stop(level, body, timestamp);
        }
    }

    fn do_stop(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, timestamp: i64) {
        self.status = Status::Stopped;
        for goal in self.behaviors.iter_mut() {
            if goal.get_status() == Status::Running {
                goal.do_stop(level, body, timestamp);
            }
        }
        for &m in &self.exit_erased_memories {
            body.brain_mut().erase_memory_id(m);
        }
    }

    fn debug_string(&self) -> String {
        let running: Vec<String> = self
            .behaviors
            .iter()
            .filter(|g| g.get_status() == Status::Running)
            .map(|b| b.debug_string())
            .collect();
        format!("{}: {:?}", self.name, running)
    }
}

/// `RunOne<E>`: a shuffled gate that starts the first behavior (by weighted random order) that can start.
pub struct RunOne;

impl RunOne {
    /// `new RunOne(weightedBehaviors)`.
    pub fn new(weighted_behaviors: Vec<(Box<dyn BehaviorControl>, i32)>) -> GateBehavior {
        Self::with_entry_condition(Vec::new(), weighted_behaviors)
    }

    /// `new RunOne(entryCondition, weightedBehaviors)`.
    pub fn with_entry_condition(
        entry_condition: Vec<(MemoryTypeId, MemoryStatus)>,
        weighted_behaviors: Vec<(Box<dyn BehaviorControl>, i32)>,
    ) -> GateBehavior {
        GateBehavior::with_list(
            entry_condition,
            Vec::new(),
            OrderPolicy::Shuffled,
            RunningPolicy::RunOne,
            weighted_behaviors,
            ShufflingList::new(),
            "RunOne",
        )
    }
}
