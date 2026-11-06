//! `TriggerGate`: a one-shot that runs a weighted list of triggers.

use super::declarative::BehaviorBuilder;
use super::gate_behavior::{OrderPolicy, RunningPolicy};
use super::one_shot::{OneShot, Trigger};
use super::shuffling_list::ShufflingList;
use crate::server::level::ServerLevel;
use crate::world::entity::entity::Entity;

/// `TriggerGate.triggerOneShuffled(weightedTriggers)`.
pub fn trigger_one_shuffled(weighted_triggers: Vec<(Box<dyn Trigger>, i32)>) -> OneShot {
    trigger_gate(
        weighted_triggers,
        OrderPolicy::Shuffled,
        RunningPolicy::RunOne,
    )
}

/// `TriggerGate.triggerGate(weightedBehaviors, orderPolicy, runningPolicy)`.
pub fn trigger_gate(
    weighted_behaviors: Vec<(Box<dyn Trigger>, i32)>,
    order_policy: OrderPolicy,
    running_policy: RunningPolicy,
) -> OneShot {
    trigger_gate_with_list(
        weighted_behaviors,
        order_policy,
        running_policy,
        ShufflingList::new(),
    )
}

/// As [`trigger_gate`] with the list's random source chosen by the caller (tests).
pub fn trigger_gate_with_list(
    weighted_behaviors: Vec<(Box<dyn Trigger>, i32)>,
    order_policy: OrderPolicy,
    running_policy: RunningPolicy,
    mut behaviors: ShufflingList<Box<dyn Trigger>>,
) -> OneShot {
    for (trigger, weight) in weighted_behaviors {
        behaviors.add(trigger, weight);
    }
    BehaviorBuilder::point(
        move |level: &mut ServerLevel, body: &mut dyn Entity, timestamp: i64| {
            order_policy.apply(&mut behaviors);
            for behavior in behaviors.iter_mut() {
                if behavior.trigger(level, body, timestamp)
                    && running_policy == RunningPolicy::RunOne
                {
                    break;
                }
            }
            true
        },
    )
}
