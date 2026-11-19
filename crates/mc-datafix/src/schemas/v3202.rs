//! `net.minecraft.util.datafix.schemas.V3202`.

use crate::schema::{Schema, SchemaDef, TemplateMap};
use crate::schemas::v99;

pub struct V3202;

impl SchemaDef for V3202 {
    fn register_block_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = this.super_register_block_entities(schema);
        map.put("minecraft:hanging_sign", v99::sign);
        map
    }
    fn namespaced(&self) -> bool {
        true
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V3202)
}
