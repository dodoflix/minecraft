//! `net.minecraft.util.datafix.schemas.V3326`.

use crate::schema::{Schema, SchemaDef, TemplateMap};

pub struct V3326;

impl SchemaDef for V3326 {
    fn register_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = this.super_register_entities(schema);
        schema.register_simple(&mut map, "minecraft:sniffer");
        map
    }
    fn namespaced(&self) -> bool {
        true
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V3326)
}
