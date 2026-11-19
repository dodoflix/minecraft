//! `net.minecraft.util.datafix.schemas.V1906`.

use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::schema::{Schema, SchemaDef, TemplateMap};
use crate::schemas::v1458;

fn register_inventory(schema: &Schema, map: &mut TemplateMap, name: &str) {
    schema.register_with(map, name, v1458::nameable_inventory);
}

pub struct V1906;

impl SchemaDef for V1906 {
    fn register_block_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = this.super_register_block_entities(schema);
        register_inventory(schema, &mut map, "minecraft:barrel");
        register_inventory(schema, &mut map, "minecraft:smoker");
        register_inventory(schema, &mut map, "minecraft:blast_furnace");
        schema.register_with(&mut map, "minecraft:lectern", |s| {
            DSL::optional_fields(&[("Book", References::ITEM_STACK.in_(s))])
        });
        schema.register_simple(&mut map, "minecraft:bell");
        map
    }
    fn namespaced(&self) -> bool {
        true
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V1906)
}
