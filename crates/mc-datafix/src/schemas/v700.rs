//! `net.minecraft.util.datafix.schemas.V700`.

use crate::schema::{Schema, SchemaDef, TemplateMap};

pub struct V700;

impl SchemaDef for V700 {
    fn register_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = this.super_register_entities(schema);
        schema.register_simple(&mut map, "ElderGuardian");
        map
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V700)
}
