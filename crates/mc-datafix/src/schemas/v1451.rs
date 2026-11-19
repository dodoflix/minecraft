//! `net.minecraft.util.datafix.schemas.V1451`.

use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::schema::{Schema, SchemaDef, TemplateMap};

pub struct V1451;

impl SchemaDef for V1451 {
    fn register_block_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = this.super_register_block_entities(schema);
        schema.register_with(&mut map, "minecraft:trapped_chest", |s| {
            DSL::optional_fields(&[("Items", DSL::list(&References::ITEM_STACK.in_(s)))])
        });
        map
    }
    fn namespaced(&self) -> bool {
        true
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V1451)
}
