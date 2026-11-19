//! `net.minecraft.util.datafix.schemas.V1481`.

use crate::schema::{Schema, SchemaDef, TemplateMap};

pub struct V1481;

impl SchemaDef for V1481 {
    fn register_block_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = this.super_register_block_entities(schema);
        schema.register_simple(&mut map, "minecraft:conduit");
        map
    }
    fn namespaced(&self) -> bool {
        true
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V1481)
}
