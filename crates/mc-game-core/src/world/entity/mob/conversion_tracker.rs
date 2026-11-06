//! `net.minecraft.world.entity.ConversionTracker`: the "afflicted for N ticks -> converting for M ticks -> converted"
//! state machine of zombies (drowning) and skeletons (freezing).
//!
//! The tracker lives in a field of its owner, so it works on the owner's synched data instead of the whole entity; the
//! owner passes the values vanilla reads through its suppliers (`isAfflicted`) and runs `doConversion` itself when
//! [`ConversionTracker::tick`] asks for it (`convertsTo` / `conversionSound` / `postConversionStep` are class specific).

use crate::world::entity::synched_data::{EntityDataAccessor, SynchedEntityData, serializer};
use mc_nbt::CompoundTag;

const NOT_CONVERTING: i32 = -1;

/// The constructor arguments of `ConversionTracker` that are constants.
#[derive(Clone, Copy)]
pub struct ConversionTrackerConfig {
    pub conversion_id: EntityDataAccessor<serializer::Boolean>,
    pub affliction_time_tag: &'static str,
    pub total_affliction_time: i32,
    pub conversion_time_tag: &'static str,
    pub total_conversion_time: i32,
}

/// `ConversionTracker` state (`afflictionTime`, `conversionTime`).
pub struct ConversionTracker {
    config: ConversionTrackerConfig,
    affliction_time: i32,
    conversion_time: i32,
}

impl ConversionTracker {
    pub fn new(config: ConversionTrackerConfig) -> Self {
        Self {
            config,
            affliction_time: 0,
            conversion_time: 0,
        }
    }

    /// `tick()` for a mob that is `isAlive() && !isNoAi()` (`ready`); returns true when `doConversion` must run now.
    pub fn tick(&mut self, data: &mut SynchedEntityData, ready: bool, is_afflicted: bool) -> bool {
        if !ready {
            return false;
        }
        if is_afflicted {
            if self.is_converting(data) {
                self.conversion_time -= 1;
                if self.conversion_time < 0 {
                    return true;
                }
            } else {
                self.affliction_time += 1;
                if self.affliction_time >= self.config.total_affliction_time {
                    self.start_conversion(data, self.config.total_conversion_time);
                }
            }
        } else {
            self.affliction_time = -1;
            self.set_converting(data, false);
        }
        false
    }

    /// `isConverting()`.
    pub fn is_converting(&self, data: &SynchedEntityData) -> bool {
        data.get(self.config.conversion_id)
    }

    fn set_converting(&self, data: &mut SynchedEntityData, converting: bool) {
        data.set(self.config.conversion_id, converting);
    }

    /// `addAdditionalSaveData(output)`.
    pub fn add_additional_save_data(
        &self,
        data: &SynchedEntityData,
        is_afflicted: bool,
        out: &mut CompoundTag,
    ) {
        out.put_int(
            self.config.conversion_time_tag,
            if self.is_converting(data) {
                self.conversion_time
            } else {
                NOT_CONVERTING
            },
        );
        out.put_int(
            self.config.affliction_time_tag,
            if is_afflicted {
                self.affliction_time
            } else {
                -1
            },
        );
    }

    /// `readAdditionalSaveData(input)`.
    pub fn read_additional_save_data(&mut self, data: &mut SynchedEntityData, input: &CompoundTag) {
        self.affliction_time = input.get_int_or(self.config.affliction_time_tag, 0);
        let conversion_time = input.get_int_or(self.config.conversion_time_tag, NOT_CONVERTING);
        if conversion_time != NOT_CONVERTING {
            self.start_conversion(data, conversion_time);
        } else {
            self.set_converting(data, false);
        }
    }

    /// `startConversion(time)`.
    pub fn start_conversion(&mut self, data: &mut SynchedEntityData, time: i32) {
        self.conversion_time = time;
        self.set_converting(data, true);
    }

    /// `setAfflictionTime(afflictionTime)`.
    pub fn set_affliction_time(&mut self, affliction_time: i32) {
        self.affliction_time = affliction_time;
    }

    /// `setConversionTime(conversionTime)`.
    pub fn set_conversion_time(&mut self, conversion_time: i32) {
        self.conversion_time = conversion_time;
    }
}
