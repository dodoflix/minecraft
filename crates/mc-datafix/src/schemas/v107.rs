//! `net.minecraft.util.datafix.schemas.V107`.

use crate::schema::{Schema, SchemaDef, TemplateMap};

pub struct V107;

impl SchemaDef for V107 {
    fn register_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = this.super_register_entities(schema);
        map.remove("Minecart");
        map
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V107)
}
