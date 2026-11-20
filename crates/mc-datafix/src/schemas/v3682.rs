//! `net.minecraft.util.datafix.schemas.V3682`.

use crate::schema::{Schema, SchemaDef, TemplateMap};
use crate::schemas::v1458;

pub struct V3682;

impl SchemaDef for V3682 {
    fn register_block_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = this.super_register_block_entities(schema);
        schema.register_with(&mut map, "minecraft:crafter", v1458::nameable_inventory);
        map
    }
    fn namespaced(&self) -> bool {
        true
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V3682)
}
