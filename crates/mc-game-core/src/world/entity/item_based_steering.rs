//! `ItemBasedSteering` (`Pig`, `Strider`): the carrot-on-a-stick speed boost.

use super::synched_data::{EntityDataAccessor, SynchedEntityData, serializer};
use mc_core::math::mth;
use mc_core::random::RandomSource;

pub struct ItemBasedSteering {
    boost_time_accessor: EntityDataAccessor<serializer::Int>,
    boosting: bool,
    boost_time: i32,
}

impl ItemBasedSteering {
    /// `new ItemBasedSteering(entityData, boostTimeAccessor)`.
    pub fn new(boost_time_accessor: EntityDataAccessor<serializer::Int>) -> Self {
        Self { boost_time_accessor, boosting: false, boost_time: 0 }
    }

    /// `onSynced()`.
    pub fn on_synced(&mut self) {
        self.boosting = true;
        self.boost_time = 0;
    }

    /// `boost(random)`.
    pub fn boost(&mut self, data: &mut SynchedEntityData, random: &mut dyn RandomSource) -> bool {
        if self.boosting {
            false
        } else {
            self.boosting = true;
            self.boost_time = 0;
            data.set(self.boost_time_accessor, random.next_int_bounded(841) + 140);
            true
        }
    }

    /// `tickBoost()`.
    pub fn tick_boost(&mut self, data: &SynchedEntityData) {
        let time = self.boost_time;
        self.boost_time += 1;
        if self.boosting && time > data.get(self.boost_time_accessor) {
            self.boosting = false;
        }
    }

    /// `boostFactor()`.
    pub fn boost_factor(&self, data: &SynchedEntityData) -> f32 {
        if self.boosting {
            1.0 + 1.15 * mth::sin(f64::from(self.boost_time as f32 / data.get(self.boost_time_accessor) as f32 * std::f32::consts::PI))
        } else {
            1.0
        }
    }
}
