//! `ai/behavior/declarative`: `BehaviorBuilder`, `MemoryCondition`, `MemoryAccessor`.
//!
//! Vanilla expresses a one-shot behavior as an applicative over memory conditions
//! (`i.group(i.present(A), i.absent(B), i.registered(C)).apply(i, (a, b, c) -> trigger)`); here the conditions are
//! declared on a [`BehaviorBuilder`] (the `Instance`), which hands out typed handles ([`Present`], [`Absent`],
//! [`Registered`] = `MemoryAccessor`s), and [`BehaviorBuilder::create`] takes the trigger closure.
//! Semantics preserved exactly: the conditions (and [`BehaviorBuilder::if_triggered`] gates) are evaluated in
//! declaration order, each one at the moment of the attempt; the first failure aborts without running anything
//! after it; the closure runs last. `getRequiredMemories()` is the set of declared memories.

use super::one_shot::{OneShot, Trigger};
use crate::server::level::ServerLevel;
use crate::world::entity::ai::memory::{MemoryData, MemoryModuleType, MemoryTypeId, MemoryValue};
use crate::world::entity::entity::Entity;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Present,
    Absent,
    Registered,
}

/// `i.present(memory)`: the memory must hold a value; `get` gives it.
pub struct Present<U> {
    index: usize,
    memory: MemoryModuleType<U>,
}
/// `i.absent(memory)`: the memory must be registered and empty; only the setters apply.
pub struct Absent<U> {
    memory: MemoryModuleType<U>,
}
/// `i.registered(memory)`: the memory only has to be registered; `try_get` gives the value if any.
pub struct Registered<U> {
    index: usize,
    memory: MemoryModuleType<U>,
}

/// `i.present(memory)` for an untyped `MemoryModuleType<?>` (generic behaviors like `EraseMemoryIf`).
pub struct PresentUntyped {
    index: usize,
    id: MemoryTypeId,
}

/// What every `MemoryAccessor` can do (`set`, `setOrErase`, `setWithExpiry`, `erase`).
pub trait MemoryAccessor<U: MemoryData> {
    fn memory_type(&self) -> &MemoryModuleType<U>;
}
impl<U: MemoryData> MemoryAccessor<U> for Present<U> {
    fn memory_type(&self) -> &MemoryModuleType<U> {
        &self.memory
    }
}
impl<U: MemoryData> MemoryAccessor<U> for Absent<U> {
    fn memory_type(&self) -> &MemoryModuleType<U> {
        &self.memory
    }
}
impl<U: MemoryData> MemoryAccessor<U> for Registered<U> {
    fn memory_type(&self) -> &MemoryModuleType<U> {
        &self.memory
    }
}

enum Condition {
    Memory {
        id: MemoryTypeId,
        kind: Kind,
    },
    /// `i.ifTriggered(trigger)`.
    Gate(Box<dyn Trigger>),
}

/// `BehaviorBuilder.Instance`: collects the conditions of one behavior.
#[derive(Default)]
pub struct BehaviorBuilder {
    conditions: Vec<Condition>,
    memory_values: usize,
}

/// The evaluated conditions plus level, body and timestamp: the argument of the trigger closure.
pub struct TriggerContext<'a> {
    pub level: &'a mut ServerLevel,
    pub body: &'a mut dyn Entity,
    pub timestamp: i64,
    values: Vec<Option<MemoryValue>>,
}

impl<'a> TriggerContext<'a> {
    pub fn level(&mut self) -> &mut ServerLevel {
        self.level
    }

    pub fn body(&self) -> &dyn Entity {
        &*self.body
    }

    pub fn body_mut(&mut self) -> &mut dyn Entity {
        &mut *self.body
    }

    /// `i.get(present)`.
    pub fn get<U: MemoryData>(&self, handle: &Present<U>) -> &U {
        self.values[handle.index]
            .as_ref()
            .and_then(U::from_value)
            .expect("present memory holds its value")
    }

    /// `i.tryGet(registered)`.
    pub fn try_get<U: MemoryData>(&self, handle: &Registered<U>) -> Option<&U> {
        self.values[handle.index].as_ref().and_then(U::from_value)
    }

    /// `i.get(present)` of an untyped memory.
    pub fn get_value(&self, handle: &PresentUntyped) -> &MemoryValue {
        self.values[handle.index]
            .as_ref()
            .expect("present memory holds its value")
    }

    /// `accessor.erase()` of an untyped memory.
    pub fn erase_untyped(&mut self, handle: &PresentUntyped) {
        self.body.brain_mut().erase_memory_id(handle.id);
    }

    /// `accessor.set(value)`.
    pub fn set<U: MemoryData>(&mut self, handle: &impl MemoryAccessor<U>, value: U) {
        self.body
            .brain_mut()
            .set_memory(handle.memory_type(), value);
    }

    /// `accessor.setOrErase(value)`.
    pub fn set_or_erase<U: MemoryData>(
        &mut self,
        handle: &impl MemoryAccessor<U>,
        value: Option<U>,
    ) {
        self.body
            .brain_mut()
            .set_memory_opt(handle.memory_type(), value);
    }

    /// `accessor.setWithExpiry(value, timeToLive)`.
    pub fn set_with_expiry<U: MemoryData>(
        &mut self,
        handle: &impl MemoryAccessor<U>,
        value: U,
        time_to_live: i64,
    ) {
        self.body
            .brain_mut()
            .set_memory_with_expiry(handle.memory_type(), value, time_to_live);
    }

    /// `accessor.erase()`.
    pub fn erase<U: MemoryData>(&mut self, handle: &impl MemoryAccessor<U>) {
        self.body.brain_mut().erase_memory(handle.memory_type());
    }
}

impl BehaviorBuilder {
    /// `BehaviorBuilder.instance()`.
    pub fn instance() -> Self {
        Self::default()
    }

    fn add_memory(&mut self, id: MemoryTypeId, kind: Kind) -> usize {
        let index = self.memory_values;
        self.memory_values += 1;
        self.conditions.push(Condition::Memory { id, kind });
        index
    }

    /// `i.present(memory)`.
    pub fn present<U: MemoryData>(&mut self, memory: &MemoryModuleType<U>) -> Present<U> {
        Present {
            index: self.add_memory(memory.id(), Kind::Present),
            memory: *memory,
        }
    }

    /// `i.present(memory)` for an untyped memory type.
    pub fn present_untyped(&mut self, id: MemoryTypeId) -> PresentUntyped {
        PresentUntyped {
            index: self.add_memory(id, Kind::Present),
            id,
        }
    }

    /// `i.absent(memory)`.
    pub fn absent<U: MemoryData>(&mut self, memory: &MemoryModuleType<U>) -> Absent<U> {
        self.add_memory(memory.id(), Kind::Absent);
        Absent { memory: *memory }
    }

    /// `i.registered(memory)`.
    pub fn registered<U: MemoryData>(&mut self, memory: &MemoryModuleType<U>) -> Registered<U> {
        Registered {
            index: self.add_memory(memory.id(), Kind::Registered),
            memory: *memory,
        }
    }

    /// `i.ifTriggered(trigger)`: the gate runs (and may have side effects) at this position of the evaluation.
    pub fn if_triggered(&mut self, trigger: impl Trigger) {
        self.conditions.push(Condition::Gate(Box::new(trigger)));
    }

    /// `BehaviorBuilder.create(i -> i.group(...).apply(i, (...) -> trigger))` for the conditions declared so far.
    pub fn create(
        self,
        mut trigger: impl FnMut(&mut TriggerContext<'_>) -> bool + 'static,
    ) -> OneShot {
        let BehaviorBuilder {
            mut conditions,
            memory_values,
        } = self;
        let required: Vec<MemoryTypeId> = conditions
            .iter()
            .filter_map(|c| match c {
                Condition::Memory { id, .. } => Some(*id),
                Condition::Gate(_) => None,
            })
            .collect();
        let debug = format!(
            "OneShot[{}]",
            required
                .iter()
                .map(|m| format!("M[{m}]"))
                .collect::<Vec<_>>()
                .join(" * ")
        );
        let evaluate =
            move |level: &mut ServerLevel, body: &mut dyn Entity, timestamp: i64| -> bool {
                let mut values: Vec<Option<MemoryValue>> = Vec::with_capacity(memory_values);
                for condition in conditions.iter_mut() {
                    match condition {
                        Condition::Memory { id, kind } => {
                            // `Brain.getMemoryInternal`: null when the memory is not registered
                            let Some(slot) = body.brain().get_memory_value(*id) else {
                                return false;
                            };
                            let ok = match kind {
                                Kind::Present => slot.is_some(),
                                Kind::Absent => slot.is_none(),
                                Kind::Registered => true,
                            };
                            if !ok {
                                return false;
                            }
                            values.push(slot.cloned());
                        }
                        Condition::Gate(gate) => {
                            if !gate.trigger(level, body, timestamp) {
                                return false;
                            }
                        }
                    }
                }
                trigger(&mut TriggerContext {
                    level,
                    body,
                    timestamp,
                    values,
                })
            };
        OneShot::new(evaluate, required, debug)
    }

    /// `i.point(trigger)` as a whole behavior: `BehaviorBuilder.create(i -> i.point(trigger))`.
    pub fn point(trigger: impl Trigger) -> OneShot {
        OneShot::new(trigger, Vec::new(), "OneShot[C[trigger]]".to_string())
    }

    /// `BehaviorBuilder.sequence(first, second)`.
    pub fn sequence(mut first: impl Trigger, mut second: OneShot) -> OneShot {
        use super::BehaviorControl;
        let required = second.get_required_memories();
        OneShot::new(
            move |level: &mut ServerLevel, body: &mut dyn Entity, timestamp: i64| {
                first.trigger(level, body, timestamp) && second.trigger(level, body, timestamp)
            },
            required,
            "OneShot[stuff]".to_string(),
        )
    }

    /// `BehaviorBuilder.triggerIf(predicate, behavior)`.
    pub fn trigger_if(
        mut predicate: impl FnMut(&dyn Entity) -> bool + 'static,
        behavior: OneShot,
    ) -> OneShot {
        Self::sequence(
            move |_: &mut ServerLevel, body: &mut dyn Entity, _: i64| predicate(&*body),
            behavior,
        )
    }

    /// `BehaviorBuilder.triggerIf(predicate)` (`Predicate<E>`).
    pub fn trigger_if_alone(mut predicate: impl FnMut(&dyn Entity) -> bool + 'static) -> OneShot {
        Self::point(move |_: &mut ServerLevel, body: &mut dyn Entity, _: i64| predicate(&*body))
    }

    /// `BehaviorBuilder.triggerIf(BiPredicate<ServerLevel, E>)`.
    pub fn trigger_if_level(
        mut predicate: impl FnMut(&mut ServerLevel, &dyn Entity) -> bool + 'static,
    ) -> OneShot {
        Self::point(
            move |level: &mut ServerLevel, body: &mut dyn Entity, _: i64| predicate(level, &*body),
        )
    }
}
