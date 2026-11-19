//! `net.minecraft.util.datafix.schemas.V701`.

use crate::schema::{Schema, SchemaDef, TemplateMap};

pub struct V701;

impl SchemaDef for V701 {
    fn register_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = this.super_register_entities(schema);
        schema.register_simple(&mut map, "WitherSkeleton");
        schema.register_simple(&mut map, "Stray");
        map
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V701)
}
