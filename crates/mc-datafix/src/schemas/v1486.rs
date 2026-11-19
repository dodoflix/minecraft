//! `net.minecraft.util.datafix.schemas.V1486`.

use crate::schema::{Schema, SchemaDef, TemplateMap};

fn move_entry(map: &mut TemplateMap, old: &str, new: &str) {
    let supplier = map
        .remove(old)
        .unwrap_or_else(|| panic!("Didn't find {old} in schema"));
    map.put_supplier(new, supplier);
}

pub struct V1486;

impl SchemaDef for V1486 {
    fn register_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = this.super_register_entities(schema);
        move_entry(&mut map, "minecraft:cod_mob", "minecraft:cod");
        move_entry(&mut map, "minecraft:salmon_mob", "minecraft:salmon");
        map
    }
    fn namespaced(&self) -> bool {
        true
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V1486)
}
