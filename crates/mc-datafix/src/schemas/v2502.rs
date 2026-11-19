//! `net.minecraft.util.datafix.schemas.V2502`.

use crate::schema::{Schema, SchemaDef, TemplateMap};

pub struct V2502;

impl SchemaDef for V2502 {
    fn register_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = this.super_register_entities(schema);
        schema.register_simple(&mut map, "minecraft:hoglin");
        map
    }
    fn namespaced(&self) -> bool {
        true
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V2502)
}
