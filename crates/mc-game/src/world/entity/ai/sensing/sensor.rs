//! `Sensor<E>`: the scan-rate driver and the static targeting helpers (`Sensor.isEntityTargetable`, ...).

use crate::server::level::ServerLevel;
use crate::world::entity::ai::memory::memory_module_type::ATTACK_TARGET;
use crate::world::entity::ai::memory::{EntityRef, MemoryTypeId};
use crate::world::entity::ai::targeting::TargetingConditions;
use crate::world::entity::entity::Entity;
use mc_core::random::RandomSource;
use std::cell::Cell;

/// `Sensor.DEFAULT_SCAN_RATE`.
pub const DEFAULT_SCAN_RATE: i32 = 20;
/// `Sensor.DEFAULT_TARGETING_RANGE`.
pub const DEFAULT_TARGETING_RANGE: f64 = 16.0;

thread_local! {
    /// The `range` of the six shared `TargetingConditions` constants of `Sensor`: every sensor scan overwrites all of
    /// them with the scanning body's `FOLLOW_RANGE` (`updateTargetingConditionRanges`), so they always agree; the
    /// last scan of any mob wins, exactly like vanilla's shared statics.
    static TARGETING_RANGE: Cell<f64> = const { Cell::new(DEFAULT_TARGETING_RANGE) };
}

/// The current shared range of `Sensor.TARGET_CONDITIONS` & co.
pub fn targeting_range() -> f64 {
    TARGETING_RANGE.with(Cell::get)
}

fn conditions(combat: bool, ignore_invisibility: bool, ignore_line_of_sight: bool) -> TargetingConditions {
    let mut c = if combat { TargetingConditions::for_combat() } else { TargetingConditions::for_non_combat() }.range(targeting_range());
    if ignore_line_of_sight {
        c = c.ignore_line_of_sight();
    }
    if ignore_invisibility {
        c = c.ignore_invisibility_testing();
    }
    c
}

fn is_attack_target(body: &dyn Entity, entity: &dyn Entity) -> bool {
    body.brain().is_memory_value(&ATTACK_TARGET, &EntityRef(entity.id()))
}

/// `Sensor.isEntityTargetable(level, body, entity)`.
pub fn is_entity_targetable(level: &ServerLevel, body: &dyn Entity, entity: &dyn Entity) -> bool {
    conditions(false, is_attack_target(body, entity), false).test(level, Some(body), entity)
}

/// `Sensor.isEntityAttackable(level, body, target)`.
pub fn is_entity_attackable(level: &ServerLevel, body: &dyn Entity, target: &dyn Entity) -> bool {
    conditions(true, is_attack_target(body, target), false).test(level, Some(body), target)
}

/// `Sensor.isEntityAttackableIgnoringLineOfSight(level, body, target)`.
pub fn is_entity_attackable_ignoring_line_of_sight(level: &ServerLevel, body: &dyn Entity, target: &dyn Entity) -> bool {
    conditions(true, is_attack_target(body, target), true).test(level, Some(body), target)
}

/// `Sensor.rememberPositives(invocations, predicate)`: a positive result sticks for the next `invocations` calls.
pub fn remember_positives<T: ?Sized, U: ?Sized>(invocations: i32, mut predicate: impl FnMut(&T, &U) -> bool) -> impl FnMut(&T, &U) -> bool {
    let mut positives_left = 0i32;
    move |t, u| {
        if predicate(t, u) {
            positives_left = invocations;
            true
        } else {
            positives_left -= 1;
            positives_left >= 0
        }
    }
}

/// The sensor-specific part of `Sensor<E>` (`doTick`, `requires`, scan rate).
pub trait Sensor: 'static {
    /// The `scanRate` the constructor passed to `super(scanRate)` (default 20).
    fn scan_rate(&self) -> i32 {
        DEFAULT_SCAN_RATE
    }

    /// `doTick(level, body)`.
    fn do_tick(&mut self, level: &mut ServerLevel, body: &mut dyn Entity);

    /// `requires()`: the memories this sensor writes (registered with the brain).
    fn requires(&self) -> Vec<MemoryTypeId>;
}

/// The `Sensor` base class state wrapped around a boxed sensor: `timeToTick`.
pub struct SensorInstance {
    pub(crate) sensor: Option<Box<dyn Sensor>>,
    scan_rate: i32,
    time_to_tick: i64,
}

impl SensorInstance {
    pub fn new(sensor: Box<dyn Sensor>) -> Self {
        let scan_rate = sensor.scan_rate();
        Self { sensor: Some(sensor), scan_rate, time_to_tick: 0 }
    }

    /// `randomlyDelayStart(random)`: `timeToTick = random.nextInt(scanRate)`.
    pub fn randomly_delay_start(&mut self, random: &mut dyn RandomSource) {
        self.time_to_tick = i64::from(random.next_int_bounded(self.scan_rate));
    }

    pub fn requires(&self) -> Vec<MemoryTypeId> {
        self.sensor.as_ref().map(|s| s.requires()).unwrap_or_default()
    }

    pub fn time_to_tick(&self) -> i64 {
        self.time_to_tick
    }

    /// The `--timeToTick <= 0` step of `tick`: `true` when the sensor is due (the countdown is reset).
    pub fn due(&mut self) -> bool {
        self.time_to_tick -= 1;
        if self.time_to_tick <= 0 {
            self.time_to_tick = i64::from(self.scan_rate);
            true
        } else {
            false
        }
    }
}

/// `Sensor.updateTargetingConditionRanges(body)`.
pub(crate) fn update_targeting_condition_ranges(body: &dyn Entity) {
    let follow_range = body.living().map_or(DEFAULT_TARGETING_RANGE, |l| l.get_attribute_value(mc_data::Attribute::FOLLOW_RANGE));
    TARGETING_RANGE.with(|r| r.set(follow_range));
}
