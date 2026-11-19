//! `net.minecraft.util.datafix.schemas.V143`.

use crate::schema::{Schema, SchemaDef, TemplateMap};

pub struct V143;

impl SchemaDef for V143 {
    fn register_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = this.super_register_entities(schema);
        map.remove("TippedArrow");
        map
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V143)
}
