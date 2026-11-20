//! `net.minecraft.util.datafix.schemas.V4301`.

use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::schema::{Schema, SchemaDef, TemplateMap};

pub struct V4301;

impl SchemaDef for V4301 {
    fn register_types(
        &self,
        this: &Schema,
        schema: &Schema,
        entity_types: &TemplateMap,
        block_entity_types: &TemplateMap,
    ) {
        this.super_register_types(schema, entity_types, block_entity_types);
        schema.register_type(true, &References::ENTITY_EQUIPMENT, |s| {
            DSL::optional(&DSL::field(
                "equipment",
                &DSL::optional_fields(&[
                    ("mainhand", References::ITEM_STACK.in_(s)),
                    ("offhand", References::ITEM_STACK.in_(s)),
                    ("feet", References::ITEM_STACK.in_(s)),
                    ("legs", References::ITEM_STACK.in_(s)),
                    ("chest", References::ITEM_STACK.in_(s)),
                    ("head", References::ITEM_STACK.in_(s)),
                    ("body", References::ITEM_STACK.in_(s)),
                    ("saddle", References::ITEM_STACK.in_(s)),
                ]),
            ))
        });
    }
    fn namespaced(&self) -> bool {
        true
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V4301)
}
