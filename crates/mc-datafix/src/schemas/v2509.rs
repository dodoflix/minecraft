//! `net.minecraft.util.datafix.schemas.V2509`.

use crate::schema::{Schema, SchemaDef, TemplateMap};

pub struct V2509;

impl SchemaDef for V2509 {
    fn register_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = this.super_register_entities(schema);
        map.remove("minecraft:zombie_pigman");
        schema.register_simple(&mut map, "minecraft:zombified_piglin");
        map
    }
    fn namespaced(&self) -> bool {
        true
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V2509)
}
