//! `net.minecraft.util.datafix.schemas.V501`.

use crate::schema::{Schema, SchemaDef, TemplateMap};

pub struct V501;

impl SchemaDef for V501 {
    fn register_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = this.super_register_entities(schema);
        schema.register_simple(&mut map, "PolarBear");
        map
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V501)
}
