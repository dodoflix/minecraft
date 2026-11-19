//! `net.minecraft.util.datafix.schemas.V2505`.

use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::schema::{Schema, SchemaDef, TemplateMap};

pub struct V2505;

impl SchemaDef for V2505 {
    fn register_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = this.super_register_entities(schema);
        schema.register_with(&mut map, "minecraft:piglin", |s| {
            DSL::optional_fields(&[("Inventory", DSL::list(&References::ITEM_STACK.in_(s)))])
        });
        map
    }
    fn namespaced(&self) -> bool {
        true
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V2505)
}
