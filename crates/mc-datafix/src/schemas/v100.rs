//! `net.minecraft.util.datafix.schemas.V100`.

use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::schema::{Schema, SchemaDef, TemplateMap};

pub struct V100;

impl SchemaDef for V100 {
    fn register_types(
        &self,
        this: &Schema,
        schema: &Schema,
        entity_types: &TemplateMap,
        block_entity_types: &TemplateMap,
    ) {
        this.super_register_types(schema, entity_types, block_entity_types);
        schema.register_type(true, &References::ENTITY_EQUIPMENT, |s| {
            DSL::and_list(&[
                DSL::optional(&DSL::field(
                    "ArmorItems",
                    &DSL::list(&References::ITEM_STACK.in_(s)),
                )),
                DSL::optional(&DSL::field(
                    "HandItems",
                    &DSL::list(&References::ITEM_STACK.in_(s)),
                )),
                DSL::optional(&DSL::field(
                    "body_armor_item",
                    &References::ITEM_STACK.in_(s),
                )),
                DSL::optional(&DSL::field("saddle", &References::ITEM_STACK.in_(s))),
            ])
        });
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V100)
}
