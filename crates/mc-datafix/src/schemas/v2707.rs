//! `net.minecraft.util.datafix.schemas.V2707`.

use crate::schema::{Schema, SchemaDef, TemplateMap};

pub struct V2707;

impl SchemaDef for V2707 {
    fn register_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = this.super_register_entities(schema);
        this.register_simple(&mut map, "minecraft:marker");
        map
    }
    fn namespaced(&self) -> bool {
        true
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V2707)
}
