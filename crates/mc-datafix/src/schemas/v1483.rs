//! `net.minecraft.util.datafix.schemas.V1483`.

use crate::schema::{Schema, SchemaDef, TemplateMap};

fn move_entry(map: &mut TemplateMap, old: &str, new: &str) {
    let supplier = map
        .remove(old)
        .unwrap_or_else(|| panic!("Didn't find {old} in schema"));
    map.put_supplier(new, supplier);
}

pub struct V1483;

impl SchemaDef for V1483 {
    fn register_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = this.super_register_entities(schema);
        move_entry(&mut map, "minecraft:puffer_fish", "minecraft:pufferfish");
        map
    }
    fn namespaced(&self) -> bool {
        true
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V1483)
}
