//! `net.minecraft.util.datafix.schemas.V1451_5`.

use crate::schema::{Schema, SchemaDef, TemplateMap};

pub struct V1451_5;

impl SchemaDef for V1451_5 {
    fn register_block_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = this.super_register_block_entities(schema);
        map.remove("minecraft:flower_pot");
        map.remove("minecraft:noteblock");
        map
    }
    fn namespaced(&self) -> bool {
        true
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V1451_5)
}
