//! `net.minecraft.util.datafix.schemas.V1920`.

use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::schema::{Schema, SchemaDef, TemplateMap};

fn register_inventory(schema: &Schema, map: &mut TemplateMap, name: &str) {
    schema.register_with(map, name, |s| {
        DSL::optional_fields(&[("Items", DSL::list(&References::ITEM_STACK.in_(s)))])
    });
}

pub struct V1920;

impl SchemaDef for V1920 {
    fn register_block_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = this.super_register_block_entities(schema);
        register_inventory(schema, &mut map, "minecraft:campfire");
        map
    }
    fn namespaced(&self) -> bool {
        true
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V1920)
}
