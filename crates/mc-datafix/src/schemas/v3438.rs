//! `net.minecraft.util.datafix.schemas.V3438`.

use crate::schema::{Schema, SchemaDef, TemplateMap};

pub struct V3438;

impl SchemaDef for V3438 {
    fn register_block_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = this.super_register_block_entities(schema);
        let sand = map
            .remove("minecraft:suspicious_sand")
            .expect("suspicious_sand is registered");
        map.put_supplier("minecraft:brushable_block", sand);
        schema.register_simple(&mut map, "minecraft:calibrated_sculk_sensor");
        map
    }
    fn namespaced(&self) -> bool {
        true
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V3438)
}
