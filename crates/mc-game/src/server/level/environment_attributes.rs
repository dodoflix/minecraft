//! `Level.environmentAttributes().getValue(attribute, pos)` for block code: reads the value cached for this tick
//! (`ServerLevel::tick` primes every sampler right after the weather/clock update, like vanilla's per-tick cache).

use super::ServerLevel;
use crate::world::attribute::AttributeEnv;

impl ServerLevel {
    fn environment_value(&self, name: &'static str) -> Option<crate::world::attribute::AttrValue> {
        // a cache miss (a level that has not ticked yet) evaluates without clocks or weather
        let env = AttributeEnv { clock_total_ticks: &|_| 0, rain_level: 0.0, thunder_level: 0.0 };
        self.tick_state.attributes.get_dimension_value(name, &env)
    }

    /// A boolean environment attribute (`gameplay/water_evaporates`, ...).
    pub fn environment_bool(&self, name: &'static str) -> bool {
        matches!(self.environment_value(name), Some(crate::world::attribute::AttrValue::Bool(true)))
    }

    /// A float environment attribute (`gameplay/turtle_egg_hatch_chance`, ...).
    pub fn environment_float(&self, name: &'static str) -> f32 {
        match self.environment_value(name) {
            Some(crate::world::attribute::AttrValue::Float(f)) => f,
            _ => 0.0,
        }
    }
}
