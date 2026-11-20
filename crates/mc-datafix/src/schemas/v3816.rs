//! `net.minecraft.util.datafix.schemas.V3816`.

use crate::schema::{Schema, SchemaDef, TemplateMap};

pub struct V3816;

impl SchemaDef for V3816 {
    fn register_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = this.super_register_entities(schema);
        schema.register_simple(&mut map, "minecraft:bogged");
        map
    }
    fn namespaced(&self) -> bool {
        true
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V3816)
}
