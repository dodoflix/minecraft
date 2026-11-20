//! `net.minecraft.util.datafix.schemas.V4532`.

use crate::schema::{Schema, SchemaDef, TemplateMap};

pub struct V4532;

impl SchemaDef for V4532 {
    fn register_block_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = this.super_register_block_entities(schema);
        schema.register_simple(&mut map, "minecraft:copper_golem_statue");
        map
    }
    fn namespaced(&self) -> bool {
        true
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V4532)
}
