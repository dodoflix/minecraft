//! `net.minecraft.util.datafix.schemas.V3325`.

use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::schema::{Schema, SchemaDef, TemplateMap};

pub struct V3325;

impl SchemaDef for V3325 {
    fn register_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = this.super_register_entities(schema);
        schema.register_with(&mut map, "minecraft:item_display", |s| {
            DSL::optional_fields(&[("item", References::ITEM_STACK.in_(s))])
        });
        schema.register_with(&mut map, "minecraft:block_display", |s| {
            DSL::optional_fields(&[("block_state", References::BLOCK_STATE.in_(s))])
        });
        schema.register_with(&mut map, "minecraft:text_display", |s| {
            DSL::optional_fields(&[("text", References::TEXT_COMPONENT.in_(s))])
        });
        map
    }
    fn namespaced(&self) -> bool {
        true
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V3325)
}
