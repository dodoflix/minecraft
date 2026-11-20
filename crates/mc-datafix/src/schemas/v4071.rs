//! `net.minecraft.util.datafix.schemas.V4071`.

use crate::schema::{Schema, SchemaDef, TemplateMap};

pub struct V4071;

impl SchemaDef for V4071 {
    fn register_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = this.super_register_entities(schema);
        schema.register_simple(&mut map, "minecraft:creaking");
        schema.register_simple(&mut map, "minecraft:creaking_transient");
        map
    }
    fn register_block_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = this.super_register_block_entities(schema);
        schema.register_simple(&mut map, "minecraft:creaking_heart");
        map
    }
    fn namespaced(&self) -> bool {
        true
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V4071)
}
