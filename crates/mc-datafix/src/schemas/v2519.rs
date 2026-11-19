//! `net.minecraft.util.datafix.schemas.V2519`.

use crate::schema::{Schema, SchemaDef, TemplateMap};

pub struct V2519;

impl SchemaDef for V2519 {
    fn register_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = this.super_register_entities(schema);
        schema.register_simple(&mut map, "minecraft:strider");
        map
    }
    fn namespaced(&self) -> bool {
        true
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V2519)
}
