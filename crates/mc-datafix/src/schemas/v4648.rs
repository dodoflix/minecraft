//! `net.minecraft.util.datafix.schemas.V4648`.

use crate::schema::{Schema, SchemaDef, TemplateMap};

pub struct V4648;

impl SchemaDef for V4648 {
    fn register_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = this.super_register_entities(schema);
        schema.register_simple(&mut map, "minecraft:nautilus");
        schema.register_simple(&mut map, "minecraft:zombie_nautilus");
        map
    }
    fn namespaced(&self) -> bool {
        true
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V4648)
}
