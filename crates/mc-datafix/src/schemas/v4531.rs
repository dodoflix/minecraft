//! `net.minecraft.util.datafix.schemas.V4531`.

use crate::schema::{Schema, SchemaDef, TemplateMap};

pub struct V4531;

impl SchemaDef for V4531 {
    fn register_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = this.super_register_entities(schema);
        schema.register_simple(&mut map, "minecraft:copper_golem");
        map
    }
    fn namespaced(&self) -> bool {
        true
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V4531)
}
