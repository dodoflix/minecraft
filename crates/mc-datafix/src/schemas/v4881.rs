//! `net.minecraft.util.datafix.schemas.V4881`.

use crate::schema::{Schema, SchemaDef, TemplateMap};

pub struct V4881;

impl SchemaDef for V4881 {
    fn register_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = this.super_register_entities(schema);
        schema.register_simple(&mut map, "minecraft:sulfur_cube");
        map
    }
    fn register_block_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = this.super_register_block_entities(schema);
        schema.register_simple(&mut map, "minecraft:potent_sulfur");
        map
    }
    fn namespaced(&self) -> bool {
        true
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V4881)
}
