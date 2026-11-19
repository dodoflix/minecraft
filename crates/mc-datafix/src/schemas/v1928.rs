//! `net.minecraft.util.datafix.schemas.V1928`.

use crate::schema::{Schema, SchemaDef, TemplateMap};

fn register_mob(schema: &Schema, map: &mut TemplateMap, name: &str) {
    schema.register_simple(map, name);
}

pub struct V1928;

impl SchemaDef for V1928 {
    fn register_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = this.super_register_entities(schema);
        map.remove("minecraft:illager_beast");
        register_mob(schema, &mut map, "minecraft:ravager");
        map
    }
    fn namespaced(&self) -> bool {
        true
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V1928)
}
