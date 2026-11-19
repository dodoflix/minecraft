//! `net.minecraft.util.datafix.schemas.V1931`.

use crate::schema::{Schema, SchemaDef, TemplateMap};

pub struct V1931;

impl SchemaDef for V1931 {
    fn register_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = this.super_register_entities(schema);
        schema.register_simple(&mut map, "minecraft:fox");
        map
    }
    fn namespaced(&self) -> bool {
        true
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V1931)
}
