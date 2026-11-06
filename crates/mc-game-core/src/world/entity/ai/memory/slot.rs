//! `MemorySlot<T>`: one memory of a brain, a value with a time to live.

use super::value::MemoryValue;

const NEVER_EXPIRE: i64 = i64::MAX;

#[derive(Clone, Debug)]
pub struct MemorySlot {
    value: Option<MemoryValue>,
    time_to_live: i64,
}

impl Default for MemorySlot {
    fn default() -> Self {
        Self::create()
    }
}

impl MemorySlot {
    /// `MemorySlot.create()`.
    pub fn create() -> Self {
        Self {
            value: None,
            time_to_live: NEVER_EXPIRE,
        }
    }

    /// `tick()`: a value that can expire loses a tick, or is cleared once its time ran out.
    pub fn tick(&mut self) {
        if self.has_value() && self.can_expire() {
            if self.has_expired() {
                self.clear();
            } else {
                self.time_to_live -= 1;
            }
        }
    }

    /// `set(value, timeToLive)`.
    pub fn set_with_ttl(&mut self, value: MemoryValue, time_to_live: i64) {
        self.value = Some(value);
        self.time_to_live = time_to_live;
    }

    /// `set(value)`.
    pub fn set(&mut self, value: MemoryValue) {
        self.set_with_ttl(value, NEVER_EXPIRE);
    }

    pub fn clear(&mut self) {
        self.value = None;
        self.time_to_live = NEVER_EXPIRE;
    }

    pub fn has_value(&self) -> bool {
        self.value.is_some()
    }

    pub fn value(&self) -> Option<&MemoryValue> {
        self.value.as_ref()
    }

    pub fn value_mut(&mut self) -> Option<&mut MemoryValue> {
        self.value.as_mut()
    }

    pub fn can_expire(&self) -> bool {
        self.time_to_live != NEVER_EXPIRE
    }

    pub fn has_expired(&self) -> bool {
        self.time_to_live <= 0
    }

    pub fn time_to_live(&self) -> i64 {
        self.time_to_live
    }
}
