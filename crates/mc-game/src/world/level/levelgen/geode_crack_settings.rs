//! `GeodeCrackSettings`.

use super::codec::{Fields, LoadResult, parse_double_range, parse_int_range};
use serde_json::Value;

pub struct GeodeCrackSettings {
    pub generate_crack_chance: f64,
    pub base_crack_size: f64,
    pub crack_point_offset: i32,
}

impl GeodeCrackSettings {
    pub fn from_json(value: &Value) -> LoadResult<GeodeCrackSettings> {
        let fields = Fields::new(value)?;
        Ok(GeodeCrackSettings {
            generate_crack_chance: fields.optional("generate_crack_chance", |v| parse_double_range(v, 0.0, 1.0))?.unwrap_or(1.0),
            base_crack_size: fields.optional("base_crack_size", |v| parse_double_range(v, 0.0, 5.0))?.unwrap_or(2.0),
            crack_point_offset: fields.optional("crack_point_offset", |v| parse_int_range(v, 0, 10))?.unwrap_or(2),
        })
    }
}
