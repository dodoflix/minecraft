//! `net.minecraft.util.datafix.schemas.V3439_1`.

use crate::schema::{Schema, SchemaDef, TemplateMap};
use crate::schemas::v3439;

pub struct V3439_1;

impl SchemaDef for V3439_1 {
    fn register_block_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = this.super_register_block_entities(schema);
        schema.register_with(&mut map, "minecraft:hanging_sign", v3439::sign);
        map
    }
    fn namespaced(&self) -> bool {
        true
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V3439_1)
}
