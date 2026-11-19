//! `net.minecraft.util.datafix.schemas.V3076`.

use crate::schema::{Schema, SchemaDef, TemplateMap};

pub struct V3076;

impl SchemaDef for V3076 {
    fn register_block_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = this.super_register_block_entities(schema);
        schema.register_simple(&mut map, "minecraft:sculk_catalyst");
        map
    }
    fn namespaced(&self) -> bool {
        true
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V3076)
}
