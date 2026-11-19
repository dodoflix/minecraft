//! `net.minecraft.util.datafix.schemas.V3082`.

use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::schema::{Schema, SchemaDef, TemplateMap};

pub struct V3082;

impl SchemaDef for V3082 {
    fn register_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = this.super_register_entities(schema);
        schema.register_with(&mut map, "minecraft:chest_boat", |s| {
            DSL::optional_fields(&[("Items", DSL::list(&References::ITEM_STACK.in_(s)))])
        });
        map
    }
    fn namespaced(&self) -> bool {
        true
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V3082)
}
