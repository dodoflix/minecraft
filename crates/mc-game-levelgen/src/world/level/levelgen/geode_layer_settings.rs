//! `GeodeLayerSettings`.

use mc_game_core::world::level::levelgen::codec::{Fields, LoadResult, parse_double_range};
use serde_json::Value;

pub struct GeodeLayerSettings {
    pub filling: f64,
    pub inner_layer: f64,
    pub middle_layer: f64,
    pub outer_layer: f64,
}

impl GeodeLayerSettings {
    pub fn from_json(value: &Value) -> LoadResult<GeodeLayerSettings> {
        let fields = Fields::new(value)?;
        let range = |v: &Value| parse_double_range(v, 0.01, 50.0);
        Ok(GeodeLayerSettings {
            filling: fields.optional("filling", range)?.unwrap_or(1.7),
            inner_layer: fields.optional("inner_layer", range)?.unwrap_or(2.2),
            middle_layer: fields.optional("middle_layer", range)?.unwrap_or(3.2),
            outer_layer: fields.optional("outer_layer", range)?.unwrap_or(4.2),
        })
    }
}
