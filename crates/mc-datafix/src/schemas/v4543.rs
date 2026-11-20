//! `net.minecraft.util.datafix.schemas.V4543`.

use crate::schema::{Schema, SchemaDef, TemplateMap};

pub struct V4543;

impl SchemaDef for V4543 {
    fn register_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = this.super_register_entities(schema);
        schema.register_simple(&mut map, "minecraft:mannequin");
        map
    }
    fn namespaced(&self) -> bool {
        true
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V4543)
}
