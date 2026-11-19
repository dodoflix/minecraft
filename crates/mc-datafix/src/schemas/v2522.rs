//! `net.minecraft.util.datafix.schemas.V2522`.

use crate::schema::{Schema, SchemaDef, TemplateMap};

pub struct V2522;

impl SchemaDef for V2522 {
    fn register_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = this.super_register_entities(schema);
        schema.register_simple(&mut map, "minecraft:zoglin");
        map
    }
    fn namespaced(&self) -> bool {
        true
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V2522)
}
