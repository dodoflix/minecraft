//! `net.minecraft.util.datafix.schemas.V3818_4`.

use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::schema::{Schema, SchemaDef, TemplateMap};

pub struct V3818_4;

impl SchemaDef for V3818_4 {
    fn register_types(
        &self,
        this: &Schema,
        schema: &Schema,
        entity_types: &TemplateMap,
        block_entity_types: &TemplateMap,
    ) {
        this.super_register_types(schema, entity_types, block_entity_types);
        schema.register_type(true, &References::PARTICLE, |s| {
            DSL::optional_fields(&[
                ("item", References::ITEM_STACK.in_(s)),
                ("block_state", References::BLOCK_STATE.in_(s)),
            ])
        });
    }
    fn namespaced(&self) -> bool {
        true
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V3818_4)
}
