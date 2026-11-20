//! `net.minecraft.util.datafix.schemas.V4421`.

use crate::schema::{Schema, SchemaDef, TemplateMap};

pub struct V4421;

impl SchemaDef for V4421 {
    fn register_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = this.super_register_entities(schema);
        schema.register_simple(&mut map, "minecraft:happy_ghast");
        map
    }
    fn namespaced(&self) -> bool {
        true
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V4421)
}
