//! `net.minecraft.util.datafix.schemas.V4885`.

use crate::schema::{Schema, SchemaDef, TemplateMap};

pub struct V4885;

impl SchemaDef for V4885 {
    fn register_block_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = this.super_register_block_entities(schema);
        map.remove("minecraft:bed");
        map
    }
    fn namespaced(&self) -> bool {
        true
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V4885)
}
