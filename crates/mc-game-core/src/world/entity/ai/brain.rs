//! `net.minecraft.world.entity.ai.Brain` + `ActivityData`.
//!
//! A [`Brain`] owns the memories, sensors and behaviors of one living entity and lives inside the entity
//! (`LivingEntityData.brain`, accessed through `Entity::brain()` / `brain_mut()`). Everything that vanilla does with
//! `Brain<E>` and a body parameter is an associated function here that takes the body as `&mut dyn Entity`
//! ([`Brain::tick`], [`Brain::stop_all`]): while one behavior or sensor runs it is checked out of its slot (so it can
//! be `&mut` next to the `&mut` body); the memories, activity state and the other behaviors stay in the brain and
//! stay reachable through `body.brain()` — behaviors read/write memories, switch activities, even replace the whole
//! brain (`Villager.refreshBrain`) exactly like in vanilla. When a behavior replaced the brain during the pass the
//! rest of the pass is dropped (`shortcut:` vanilla keeps ticking the old brain's remaining behaviors once; upgrade
//! if a mob depends on that).
//!
//! Iteration order is vanilla's: behaviors by ascending priority, then by activity in `HashMap<Activity, _>` order
//! (emulated from `String.hashCode` of the name, see [`java_hash_map_order`]), then in registration order.

use super::behavior::{BehaviorControl, Status};
use super::memory::memory_map::BrainPacked;
use super::memory::memory_module_type::REGISTRY;
use super::memory::{
    ExpirableValue, MemoryData, MemoryMap, MemoryModuleType, MemorySlot, MemoryStatus,
    MemoryTypeId, MemoryValue,
};
use super::sensing::sensor::SensorInstance;
use super::sensing::sensor_type::SensorType;
use crate::server::level::ServerLevel;
use crate::world::entity::entity::Entity;
use crate::world::entity::schedule::Activity;
use mc_core::random::RandomSource;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};

/// `Brain.SCHEDULE_UPDATE_DELAY`.
const SCHEDULE_UPDATE_DELAY: i64 = 20;
const NO_SLOT: u8 = u8::MAX;

/// `ActivityData<E>`: what one activity of a brain consists of.
pub struct ActivityData {
    pub activity_type: Activity,
    /// `behaviorPriorityPairs`.
    pub behavior_priority_pairs: Vec<(i32, Box<dyn BehaviorControl>)>,
    pub conditions: Vec<(MemoryTypeId, MemoryStatus)>,
    pub memories_to_erase_when_stopped: Vec<MemoryTypeId>,
}

impl ActivityData {
    /// `create(activity, priorityOfFirstBehavior, behaviorList)`.
    pub fn create(
        activity: Activity,
        priority_of_first_behavior: i32,
        behavior_list: Vec<Box<dyn BehaviorControl>>,
    ) -> Self {
        Self::create_pairs(
            activity,
            Self::create_priority_pairs(priority_of_first_behavior, behavior_list),
        )
    }

    /// `create(activity, priorityOfFirstBehavior, behaviorList, memoryThatMustHaveValueAndWillBeErasedAfter)`.
    pub fn create_with_erased_memory(
        activity: Activity,
        priority_of_first_behavior: i32,
        behavior_list: Vec<Box<dyn BehaviorControl>>,
        memory_that_must_have_value_and_will_be_erased_after: MemoryTypeId,
    ) -> Self {
        Self {
            activity_type: activity,
            behavior_priority_pairs: Self::create_priority_pairs(
                priority_of_first_behavior,
                behavior_list,
            ),
            conditions: vec![(
                memory_that_must_have_value_and_will_be_erased_after,
                MemoryStatus::ValuePresent,
            )],
            memories_to_erase_when_stopped: vec![
                memory_that_must_have_value_and_will_be_erased_after,
            ],
        }
    }

    /// `create(activity, behaviorPriorityPairs)`.
    pub fn create_pairs(
        activity: Activity,
        behavior_priority_pairs: Vec<(i32, Box<dyn BehaviorControl>)>,
    ) -> Self {
        Self {
            activity_type: activity,
            behavior_priority_pairs,
            conditions: Vec::new(),
            memories_to_erase_when_stopped: Vec::new(),
        }
    }

    /// `create(activity, priorityOfFirstBehavior, behaviorList, conditions)`.
    pub fn create_with_conditions(
        activity: Activity,
        priority_of_first_behavior: i32,
        behavior_list: Vec<Box<dyn BehaviorControl>>,
        conditions: Vec<(MemoryTypeId, MemoryStatus)>,
    ) -> Self {
        Self::create_pairs_with_conditions(
            activity,
            Self::create_priority_pairs(priority_of_first_behavior, behavior_list),
            conditions,
        )
    }

    /// `create(activity, behaviorPriorityPairs, conditions)`.
    pub fn create_pairs_with_conditions(
        activity: Activity,
        behavior_priority_pairs: Vec<(i32, Box<dyn BehaviorControl>)>,
        conditions: Vec<(MemoryTypeId, MemoryStatus)>,
    ) -> Self {
        Self {
            activity_type: activity,
            behavior_priority_pairs,
            conditions,
            memories_to_erase_when_stopped: Vec::new(),
        }
    }

    /// `create(activity, behaviorPriorityPairs, conditions, memoriesToEraseWhenStopped)`.
    pub fn create_full(
        activity: Activity,
        behavior_priority_pairs: Vec<(i32, Box<dyn BehaviorControl>)>,
        conditions: Vec<(MemoryTypeId, MemoryStatus)>,
        memories_to_erase_when_stopped: Vec<MemoryTypeId>,
    ) -> Self {
        Self {
            activity_type: activity,
            behavior_priority_pairs,
            conditions,
            memories_to_erase_when_stopped,
        }
    }

    /// `createPriorityPairs(priorityOfFirstBehavior, behaviorList)`: consecutive priorities.
    pub fn create_priority_pairs(
        priority_of_first_behavior: i32,
        behavior_list: Vec<Box<dyn BehaviorControl>>,
    ) -> Vec<(i32, Box<dyn BehaviorControl>)> {
        let mut next_prio = priority_of_first_behavior;
        behavior_list
            .into_iter()
            .map(|b| {
                let pair = (next_prio, b);
                next_prio = next_prio.wrapping_add(1);
                pair
            })
            .collect()
    }
}

/// `Brain.ActivitySupplier<E>`: builds the activities for a body (called when the brain is made).
pub type ActivitySupplier = fn(&dyn Entity) -> Vec<ActivityData>;

/// `Brain.Provider<E>`.
pub struct Provider {
    memory_types: &'static [MemoryTypeId],
    sensor_types: &'static [&'static SensorType],
    activities: ActivitySupplier,
}

fn no_activities(_: &dyn Entity) -> Vec<ActivityData> {
    Vec::new()
}

impl Provider {
    /// `Brain.provider(sensorTypes)`.
    pub const fn new(sensor_types: &'static [&'static SensorType]) -> Self {
        Self {
            memory_types: &[],
            sensor_types,
            activities: no_activities,
        }
    }

    /// `Brain.provider(sensorTypes, activities)`.
    pub const fn with_activities(
        sensor_types: &'static [&'static SensorType],
        activities: ActivitySupplier,
    ) -> Self {
        Self {
            memory_types: &[],
            sensor_types,
            activities,
        }
    }

    /// `Brain.provider(memoryTypes, sensorTypes, activities)`.
    pub const fn with_memories(
        memory_types: &'static [MemoryTypeId],
        sensor_types: &'static [&'static SensorType],
        activities: ActivitySupplier,
    ) -> Self {
        Self {
            memory_types,
            sensor_types,
            activities,
        }
    }

    /// `makeBrain(body, packed)`: the activities are built from the body, the sensors' initial scan delay consumes
    /// `body.getRandom()`.
    pub fn make_brain(&self, body: &mut dyn Entity, packed: &BrainPacked) -> Brain {
        let activities = (self.activities)(&*body);
        Brain::construct(
            self.memory_types,
            self.sensor_types,
            activities,
            &packed.memories,
            &mut body.base_mut().random,
        )
    }
}

/// One `Activity` of a priority level with its behaviors (`Map<Activity, Set<BehaviorControl>>` entry).
struct ActivityBehaviors {
    activity: Activity,
    /// Insertion number of the `HashMap` entry (the order inside a bucket).
    seq: u32,
    /// `None` while the behavior is checked out.
    behaviors: Vec<Option<Box<dyn BehaviorControl>>>,
}

/// Java `HashMap` iteration order of keys with the given `hashCode`s inserted in order: by bucket index of the final
/// table (16 slots, doubled whenever the size exceeds 75%), ties in insertion order.
pub fn java_hash_map_order(hashes: &[i32]) -> Vec<usize> {
    let mut capacity = 16usize;
    while hashes.len() > capacity * 3 / 4 {
        capacity *= 2;
    }
    let mut order: Vec<usize> = (0..hashes.len()).collect();
    order.sort_by_key(|&i| {
        let h = hashes[i] as u32;
        ((h ^ (h >> 16)) as usize) & (capacity - 1)
    });
    order
}

static NEXT_BRAIN_ID: AtomicU64 = AtomicU64::new(1);

pub struct Brain {
    /// Identity of this brain object (a replaced brain has a different id).
    id: u64,
    slot_index: [u8; 128],
    /// Registered memories in registration order.
    slots: Vec<(MemoryTypeId, MemorySlot)>,
    /// `sensors` (`LinkedHashMap`: insertion order, a repeated type keeps its place).
    sensors: Vec<(&'static SensorType, SensorInstance)>,
    /// `availableBehaviorsByPriority`: a `TreeMap` by priority; per priority the activities in `HashMap` order.
    behaviors: BTreeMap<i32, Vec<ActivityBehaviors>>,
    next_seq: u32,
    /// `schedule`: the name of the `EnvironmentAttribute<Activity>` (e.g. `gameplay/villager_activity`).
    schedule: Option<&'static str>,
    activity_requirements: Vec<(Activity, Vec<(MemoryTypeId, MemoryStatus)>)>,
    activity_memories_to_erase_when_stopped: Vec<(Activity, Vec<MemoryTypeId>)>,
    core_activities: Vec<Activity>,
    active_activities: Vec<Activity>,
    default_activity: Activity,
    last_schedule_update: i64,
}

impl Default for Brain {
    fn default() -> Self {
        Self::new()
    }
}

impl Brain {
    /// `new Brain<>()`: an empty brain (no memories, sensors or behaviors).
    pub fn new() -> Self {
        debug_assert!(REGISTRY.len() < usize::from(NO_SLOT));
        let mut brain = Self {
            id: NEXT_BRAIN_ID.fetch_add(1, Ordering::Relaxed),
            slot_index: [NO_SLOT; 128],
            slots: Vec::new(),
            sensors: Vec::new(),
            behaviors: BTreeMap::new(),
            next_seq: 0,
            schedule: None,
            activity_requirements: Vec::new(),
            activity_memories_to_erase_when_stopped: Vec::new(),
            core_activities: Vec::new(),
            active_activities: Vec::new(),
            default_activity: Activity::IDLE,
            last_schedule_update: -9999,
        };
        brain.set_core_activities(vec![Activity::CORE]);
        brain.use_default_activity();
        brain
    }

    /// `Brain(memoryTypes, sensorTypes, activities, memories, randomSource)`.
    pub fn construct(
        memory_types: &[MemoryTypeId],
        sensor_types: &[&'static SensorType],
        activities: Vec<ActivityData>,
        memories: &MemoryMap,
        random: &mut dyn RandomSource,
    ) -> Self {
        let mut brain = Self::new();
        // `new Brain()` already ran the activity setup; vanilla's big constructor runs it at the end, after the
        // activities exist (same result: IDLE active unless the default activity is in the requirements).
        for &t in memory_types {
            brain.register_memory(t);
        }
        for &sensor_type in sensor_types {
            let mut instance = SensorInstance::new(sensor_type.create());
            instance.randomly_delay_start(random);
            for t in instance.requires() {
                brain.register_memory(t);
            }
            match brain
                .sensors
                .iter_mut()
                .find(|(t, _)| t.name == sensor_type.name)
            {
                Some(entry) => entry.1 = instance,
                None => brain.sensors.push((sensor_type, instance)),
            }
        }
        for activity in activities {
            brain.add_activity(
                activity.activity_type,
                activity.behavior_priority_pairs,
                activity.conditions,
                activity.memories_to_erase_when_stopped,
            );
        }
        for (type_id, value) in memories.iter() {
            brain.set_memory_internal_expirable(type_id, value);
        }
        brain.set_core_activities(vec![Activity::CORE]);
        brain.use_default_activity();
        brain
    }

    /// Unique id of this brain object.
    pub fn id(&self) -> u64 {
        self.id
    }

    // ---- memories -------------------------------------------------------------------------------------------------

    fn register_memory(&mut self, type_id: MemoryTypeId) {
        if self.slot_index[type_id.0 as usize] == NO_SLOT {
            self.slot_index[type_id.0 as usize] = self.slots.len() as u8;
            self.slots.push((type_id, MemorySlot::create()));
        }
    }

    fn slot(&self, type_id: MemoryTypeId) -> Option<&MemorySlot> {
        match self.slot_index[type_id.0 as usize] {
            NO_SLOT => None,
            i => Some(&self.slots[i as usize].1),
        }
    }

    fn slot_mut(&mut self, type_id: MemoryTypeId) -> Option<&mut MemorySlot> {
        match self.slot_index[type_id.0 as usize] {
            NO_SLOT => None,
            i => Some(&mut self.slots[i as usize].1),
        }
    }

    /// `getMemorySlot(type)`: throws `IllegalStateException` for an unregistered memory.
    fn slot_or_panic(&self, type_id: MemoryTypeId) -> &MemorySlot {
        self.slot(type_id)
            .unwrap_or_else(|| panic!("Unregistered memory fetched: {type_id}"))
    }

    /// `pack()`: the serializable memories with value (`Brain.Packed`).
    pub fn pack(&self) -> BrainPacked {
        let mut map = MemoryMap::default();
        for (type_id, slot) in &self.slots {
            if let Some(value) = slot.value()
                && type_id.can_serialize()
            {
                let expirable = if slot.can_expire() {
                    ExpirableValue::with_ttl(value.clone(), slot.time_to_live())
                } else {
                    ExpirableValue::of(value.clone())
                };
                map.insert(*type_id, expirable);
            }
        }
        BrainPacked { memories: map }
    }

    /// `hasMemoryValue(type)`.
    pub fn has_memory_value(&self, type_id: impl Into<MemoryTypeId>) -> bool {
        self.check_memory(type_id.into(), MemoryStatus::ValuePresent)
    }

    /// `clearMemories()`.
    pub fn clear_memories(&mut self) {
        for (_, slot) in &mut self.slots {
            slot.clear();
        }
    }

    /// `eraseMemory(type)`.
    pub fn erase_memory<U: MemoryData>(&mut self, type_: &MemoryModuleType<U>) {
        self.erase_memory_id(type_.id());
    }

    /// `eraseMemory(type)` for an untyped memory type.
    pub fn erase_memory_id(&mut self, type_id: MemoryTypeId) {
        if let Some(slot) = self.slot_mut(type_id) {
            slot.clear();
        }
    }

    /// `setMemory(type, value)`.
    pub fn set_memory<U: MemoryData>(&mut self, type_: &MemoryModuleType<U>, value: U) {
        self.set_memory_internal(type_.id(), Some(value.into_value()));
    }

    /// `setMemory(type, @Nullable value)` / `setMemory(type, Optional)`.
    pub fn set_memory_opt<U: MemoryData>(&mut self, type_: &MemoryModuleType<U>, value: Option<U>) {
        self.set_memory_internal(type_.id(), value.map(U::into_value));
    }

    /// `setMemoryWithExpiry(type, value, timeToLive)`.
    pub fn set_memory_with_expiry<U: MemoryData>(
        &mut self,
        type_: &MemoryModuleType<U>,
        value: U,
        time_to_live: i64,
    ) {
        self.set_memory_with_expiry_value(type_.id(), Some(value.into_value()), time_to_live);
    }

    fn set_memory_internal_expirable(
        &mut self,
        type_id: MemoryTypeId,
        value: &ExpirableValue<MemoryValue>,
    ) {
        match value.time_to_live {
            Some(ttl) => self.set_memory_with_expiry_value(type_id, Some(value.value.clone()), ttl),
            None => self.set_memory_internal(type_id, Some(value.value.clone())),
        }
    }

    fn set_memory_with_expiry_value(
        &mut self,
        type_id: MemoryTypeId,
        value: Option<MemoryValue>,
        time_to_live: i64,
    ) {
        if let Some(slot) = self.slot_mut(type_id) {
            match value.filter(|v| !v.is_empty_collection()) {
                None => slot.clear(),
                Some(v) => slot.set_with_ttl(v, time_to_live),
            }
        }
    }

    fn set_memory_internal(&mut self, type_id: MemoryTypeId, value: Option<MemoryValue>) {
        if let Some(slot) = self.slot_mut(type_id) {
            match value.filter(|v| !v.is_empty_collection()) {
                None => slot.clear(),
                Some(v) => slot.set(v),
            }
        }
    }

    /// `setMemory`/`setMemoryWithExpiry` for an untyped memory (`time_to_live` = `Some` for the expiring variant); used by data-driven
    /// test scenarios that set memories by registry id.
    pub fn set_memory_value(
        &mut self,
        type_id: MemoryTypeId,
        value: Option<MemoryValue>,
        time_to_live: Option<i64>,
    ) {
        match time_to_live {
            Some(ttl) => self.set_memory_with_expiry_value(type_id, value, ttl),
            None => self.set_memory_internal(type_id, value),
        }
    }

    /// `getMemory(type)`; panics (`IllegalStateException`) when the memory is not registered in this brain.
    pub fn get_memory<U: MemoryData>(&self, type_: &MemoryModuleType<U>) -> Option<&U> {
        self.slot_or_panic(type_.id())
            .value()
            .and_then(U::from_value)
    }

    /// Mutable access to the value of a registered memory (vanilla mutates the stored object in place, e.g. adds to
    /// a `Set<GlobalPos>`).
    pub fn get_memory_mut<U: MemoryData>(&mut self, type_: &MemoryModuleType<U>) -> Option<&mut U> {
        match self.slot_index[type_.id().0 as usize] {
            NO_SLOT => panic!("Unregistered memory fetched: {}", type_.id()),
            i => self.slots[i as usize]
                .1
                .value_mut()
                .and_then(U::from_value_mut),
        }
    }

    /// `getMemoryInternal(type)`: `None` when the memory is not registered, else its (maybe empty) value.
    pub fn get_memory_internal<U: MemoryData>(
        &self,
        type_: &MemoryModuleType<U>,
    ) -> Option<Option<&U>> {
        self.slot(type_.id())
            .map(|s| s.value().and_then(U::from_value))
    }

    /// `getMemoryInternal` for the untyped value.
    pub fn get_memory_value(&self, type_id: MemoryTypeId) -> Option<Option<&MemoryValue>> {
        self.slot(type_id).map(MemorySlot::value)
    }

    /// `getTimeUntilExpiry(type)`.
    pub fn get_time_until_expiry<U: MemoryData>(&self, type_: &MemoryModuleType<U>) -> i64 {
        self.slot_or_panic(type_.id()).time_to_live()
    }

    /// `isMemoryValue(type, value)`.
    pub fn is_memory_value<U: MemoryData>(&self, type_: &MemoryModuleType<U>, value: &U) -> bool {
        self.slot(type_.id())
            .and_then(MemorySlot::value)
            .and_then(U::from_value)
            .is_some_and(|v| v == value)
    }

    /// `checkMemory(type, status)`.
    pub fn check_memory(&self, type_id: MemoryTypeId, status: MemoryStatus) -> bool {
        match self.slot(type_id) {
            None => false,
            Some(slot) => {
                status == MemoryStatus::Registered
                    || status == MemoryStatus::ValuePresent && slot.has_value()
                    || status == MemoryStatus::ValueAbsent && !slot.has_value()
            }
        }
    }

    /// `forgetOutdatedMemories()`: ticks every slot.
    pub fn forget_outdated_memories(&mut self) {
        for (_, slot) in &mut self.slots {
            slot.tick();
        }
    }

    /// `forEach(visitor)`: registered memories with their value state, `(type, value, time to live if it expires)`.
    pub fn for_each(
        &self,
        mut visitor: impl FnMut(MemoryTypeId, Option<(&MemoryValue, Option<i64>)>),
    ) {
        for (type_id, slot) in &self.slots {
            visitor(
                *type_id,
                slot.value()
                    .map(|v| (v, slot.can_expire().then(|| slot.time_to_live()))),
            );
        }
    }

    /// Registered memory types in registration order.
    pub fn registered_memories(&self) -> impl Iterator<Item = MemoryTypeId> + '_ {
        self.slots.iter().map(|(t, _)| *t)
    }

    // ---- activities -------------------------------------------------------------------------------------------------

    /// `setSchedule(attribute)`: the name of an activity-typed environment attribute.
    pub fn set_schedule(&mut self, schedule: &'static str) {
        self.schedule = Some(schedule);
    }

    /// `setCoreActivities(activities)`.
    pub fn set_core_activities(&mut self, activities: Vec<Activity>) {
        self.core_activities = activities;
    }

    /// `getActiveActivities()`.
    pub fn get_active_activities(&self) -> &[Activity] {
        &self.active_activities
    }

    /// `useDefaultActivity()`.
    pub fn use_default_activity(&mut self) {
        self.set_active_activity(self.default_activity);
    }

    /// `getActiveNonCoreActivity()`.
    pub fn get_active_non_core_activity(&self) -> Option<Activity> {
        self.active_activities
            .iter()
            .copied()
            .find(|a| !self.core_activities.contains(a))
    }

    /// `setActiveActivityIfPossible(activity)`.
    pub fn set_active_activity_if_possible(&mut self, activity: Activity) {
        if self.activity_requirements_are_met(activity) {
            self.set_active_activity(activity);
        } else {
            self.use_default_activity();
        }
    }

    fn set_active_activity(&mut self, activity: Activity) {
        if !self.is_active(activity) {
            self.erase_memories_for_other_activities_than(activity);
            self.active_activities.clear();
            self.active_activities
                .extend(self.core_activities.iter().copied());
            if !self.active_activities.contains(&activity) {
                self.active_activities.push(activity);
            }
        }
    }

    fn erase_memories_for_other_activities_than(&mut self, activity: Activity) {
        let old: Vec<Activity> = self
            .active_activities
            .iter()
            .copied()
            .filter(|&a| a != activity)
            .collect();
        for old_activity in old {
            let memories = self
                .activity_memories_to_erase_when_stopped
                .iter()
                .find(|(a, _)| *a == old_activity)
                .map(|(_, m)| m.clone());
            for m in memories.into_iter().flatten() {
                self.erase_memory_id(m);
            }
        }
    }

    /// `updateActivityFromSchedule(environmentAttributes, gameTime, pos)`: `scheduled_activity` evaluates the schedule
    /// attribute (given its name) at the entity's position; it is only called when the update is due.
    pub fn update_activity_from_schedule(
        &mut self,
        game_time: i64,
        scheduled_activity: impl FnOnce(&'static str) -> Activity,
    ) {
        if game_time - self.last_schedule_update > SCHEDULE_UPDATE_DELAY {
            self.last_schedule_update = game_time;
            let scheduled = match self.schedule {
                Some(attribute) => scheduled_activity(attribute),
                None => Activity::IDLE,
            };
            if !self.active_activities.contains(&scheduled) {
                self.set_active_activity_if_possible(scheduled);
            }
        }
    }

    /// `setActiveActivityToFirstValid(activities)`.
    pub fn set_active_activity_to_first_valid(&mut self, activities: &[Activity]) {
        for &activity in activities {
            if self.activity_requirements_are_met(activity) {
                self.set_active_activity(activity);
                break;
            }
        }
    }

    /// `setDefaultActivity(activity)`.
    pub fn set_default_activity(&mut self, activity: Activity) {
        self.default_activity = activity;
    }

    /// `addActivity(activity, behaviorPriorityPairs, conditions, memoriesToEraseWhenStopped)`.
    pub fn add_activity(
        &mut self,
        activity: Activity,
        behavior_priority_pairs: Vec<(i32, Box<dyn BehaviorControl>)>,
        conditions: Vec<(MemoryTypeId, MemoryStatus)>,
        memories_to_erase_when_stopped: Vec<MemoryTypeId>,
    ) {
        match self
            .activity_requirements
            .iter_mut()
            .find(|(a, _)| *a == activity)
        {
            Some(entry) => entry.1 = conditions,
            None => self.activity_requirements.push((activity, conditions)),
        }
        if !memories_to_erase_when_stopped.is_empty() {
            match self
                .activity_memories_to_erase_when_stopped
                .iter_mut()
                .find(|(a, _)| *a == activity)
            {
                Some(entry) => entry.1 = memories_to_erase_when_stopped,
                None => self
                    .activity_memories_to_erase_when_stopped
                    .push((activity, memories_to_erase_when_stopped)),
            }
        }
        for (priority, behavior) in behavior_priority_pairs {
            for required in behavior.get_required_memories() {
                self.register_memory(required);
            }
            let level = self.behaviors.entry(priority).or_default();
            let index = match level.iter().position(|a| a.activity == activity) {
                Some(i) => i,
                None => {
                    level.push(ActivityBehaviors {
                        activity,
                        seq: self.next_seq,
                        behaviors: Vec::new(),
                    });
                    self.next_seq += 1;
                    // restore insertion order, then re-derive the `HashMap` iteration order
                    level.sort_by_key(|a| a.seq);
                    let hashes: Vec<i32> = level.iter().map(|a| a.activity.hash_code()).collect();
                    let mut by_insertion: Vec<Option<ActivityBehaviors>> =
                        std::mem::take(level).into_iter().map(Some).collect();
                    *level = java_hash_map_order(&hashes)
                        .into_iter()
                        .filter_map(|i| by_insertion[i].take())
                        .collect();
                    level
                        .iter()
                        .position(|a| a.activity == activity)
                        .expect("just inserted")
                }
            };
            level[index].behaviors.push(Some(behavior));
        }
    }

    /// `removeAllBehaviors()`.
    pub fn remove_all_behaviors(&mut self) {
        self.behaviors.clear();
    }

    /// `isActive(activity)`.
    pub fn is_active(&self, activity: Activity) -> bool {
        self.active_activities.contains(&activity)
    }

    fn activity_requirements_are_met(&self, activity: Activity) -> bool {
        match self
            .activity_requirements
            .iter()
            .find(|(a, _)| *a == activity)
        {
            None => false,
            Some((_, requirements)) => requirements.iter().all(|&(t, s)| self.check_memory(t, s)),
        }
    }

    /// `isBrainDead()`.
    pub fn is_brain_dead(&self) -> bool {
        self.slots.is_empty() && self.sensors.is_empty() && self.behaviors.is_empty()
    }

    // ---- behaviors ------------------------------------------------------------------------------------------------

    /// `getRunningBehaviors()` (debug): the debug strings of the behaviors in `RUNNING` state.
    pub fn get_running_behaviors(&self) -> Vec<String> {
        self.running_behavior_coordinates()
            .into_iter()
            .filter_map(|(p, a, b)| {
                self.behaviors
                    .get(&p)?
                    .get(a)?
                    .behaviors
                    .get(b)?
                    .as_ref()
                    .map(|b| b.debug_string())
            })
            .collect()
    }

    fn running_behavior_coordinates(&self) -> Vec<(i32, usize, usize)> {
        let mut out = Vec::new();
        for (&priority, level) in &self.behaviors {
            for (ai, entry) in level.iter().enumerate() {
                for (bi, behavior) in entry.behaviors.iter().enumerate() {
                    if behavior
                        .as_ref()
                        .is_some_and(|b| b.get_status() == Status::Running)
                    {
                        out.push((priority, ai, bi));
                    }
                }
            }
        }
        out
    }

    fn take_behavior(
        &mut self,
        (p, a, b): (i32, usize, usize),
    ) -> Option<Box<dyn BehaviorControl>> {
        self.behaviors
            .get_mut(&p)?
            .get_mut(a)?
            .behaviors
            .get_mut(b)?
            .take()
    }

    fn put_back_behavior(
        &mut self,
        (p, a, b): (i32, usize, usize),
        behavior: Box<dyn BehaviorControl>,
    ) {
        if let Some(slot) = self
            .behaviors
            .get_mut(&p)
            .and_then(|l| l.get_mut(a))
            .and_then(|e| e.behaviors.get_mut(b))
        {
            *slot = Some(behavior);
        }
    }

    /// `tick(level, body)`.
    pub fn tick(level: &mut ServerLevel, body: &mut dyn Entity) {
        body.brain_mut().forget_outdated_memories();
        Self::tick_sensors(level, body);
        Self::start_each_non_running_behavior(level, body);
        Self::tick_each_running_behavior(level, body);
    }

    fn tick_sensors(level: &mut ServerLevel, body: &mut dyn Entity) {
        let id = body.brain().id;
        for i in 0..body.brain().sensors.len() {
            let brain = body.brain_mut();
            if brain.id != id || i >= brain.sensors.len() {
                return;
            }
            if !brain.sensors[i].1.due() {
                continue;
            }
            let Some(mut sensor) = brain.sensors[i].1.sensor.take() else {
                continue;
            };
            super::sensing::sensor::update_targeting_condition_ranges(&*body);
            sensor.do_tick(level, body);
            let brain = body.brain_mut();
            if brain.id == id && i < brain.sensors.len() {
                brain.sensors[i].1.sensor = Some(sensor);
            }
        }
    }

    fn start_each_non_running_behavior(level: &mut ServerLevel, body: &mut dyn Entity) {
        let time = level.game_time();
        let id = body.brain().id;
        let priorities: Vec<i32> = body.brain().behaviors.keys().copied().collect();
        for priority in priorities {
            let mut ai = 0;
            loop {
                let brain = body.brain();
                if brain.id != id {
                    return;
                }
                let Some(entry) = brain.behaviors.get(&priority).and_then(|l| l.get(ai)) else {
                    break;
                };
                let (activity, count) = (entry.activity, entry.behaviors.len());
                if brain.is_active(activity) {
                    for bi in 0..count {
                        let coordinate = (priority, ai, bi);
                        let brain = body.brain_mut();
                        if brain.id != id {
                            return;
                        }
                        if !brain
                            .behaviors
                            .get(&priority)
                            .and_then(|l| l.get(ai))
                            .and_then(|e| e.behaviors.get(bi))
                            .and_then(Option::as_ref)
                            .is_some_and(|b| b.get_status() == Status::Stopped)
                        {
                            continue;
                        }
                        let Some(mut behavior) = brain.take_behavior(coordinate) else {
                            continue;
                        };
                        behavior.try_start(level, body, time);
                        let brain = body.brain_mut();
                        if brain.id == id {
                            brain.put_back_behavior(coordinate, behavior);
                        }
                    }
                }
                ai += 1;
            }
        }
    }

    fn tick_each_running_behavior(level: &mut ServerLevel, body: &mut dyn Entity) {
        let timestamp = level.game_time();
        let id = body.brain().id;
        for coordinate in body.brain().running_behavior_coordinates() {
            let brain = body.brain_mut();
            if brain.id != id {
                return;
            }
            let Some(mut behavior) = brain.take_behavior(coordinate) else {
                continue;
            };
            behavior.tick_or_stop(level, body, timestamp);
            let brain = body.brain_mut();
            if brain.id == id {
                brain.put_back_behavior(coordinate, behavior);
            }
        }
    }

    /// `stopAll(level, body)`: stops every running behavior (one that is currently executing is skipped).
    pub fn stop_all(level: &mut ServerLevel, body: &mut dyn Entity) {
        let timestamp = level.game_time();
        let id = body.brain().id;
        for coordinate in body.brain().running_behavior_coordinates() {
            let brain = body.brain_mut();
            if brain.id != id {
                return;
            }
            let Some(mut behavior) = brain.take_behavior(coordinate) else {
                continue;
            };
            behavior.do_stop(level, body, timestamp);
            let brain = body.brain_mut();
            if brain.id == id {
                brain.put_back_behavior(coordinate, behavior);
            }
        }
    }
}

/// `input.read("Brain", Brain.Packed.CODEC).ifPresent(packed -> this.brain = this.makeBrain(packed))` (the `Brain`
/// step of `LivingEntity.readAdditionalSaveData`; the mob's NBT chain calls it right after the living part).
pub fn read_brain(entity: &mut dyn Entity, input: &mc_nbt::CompoundTag) {
    if let Some(packed) = input.get_compound("Brain").and_then(BrainPacked::from_nbt) {
        let brain = entity.make_brain(&packed);
        entity.brain_mut().replace_with(brain);
    }
}

impl Brain {
    /// Replaces this brain object by `other` (`this.brain = other`).
    pub fn replace_with(&mut self, other: Brain) {
        *self = other;
    }
}

/// `body.getBrain().updateActivityFromSchedule(level.environmentAttributes(), level.getGameTime(), body.position())`.
pub fn update_activity_from_schedule(level: &ServerLevel, body: &mut dyn Entity) {
    use crate::world::attribute::AttrValue;
    let pos = body.position();
    let game_time = level.game_time();
    body.brain_mut()
        .update_activity_from_schedule(game_time, |attribute| {
            match level.environment_value(attribute, &pos) {
                AttrValue::Activity(id) => Activity::by_id(&id).unwrap_or(Activity::IDLE),
                _ => Activity::IDLE,
            }
        });
}
