//! `net.minecraft.util.datafix.schemas.V102`.

use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::schema::{Schema, SchemaDef, TemplateMap};
use crate::schemas::v99;
use crate::ty::HookFunction;

pub struct V102;

impl SchemaDef for V102 {
    fn register_types(
        &self,
        this: &Schema,
        schema: &Schema,
        entity_types: &TemplateMap,
        block_entity_types: &TemplateMap,
    ) {
        this.super_register_types(schema, entity_types, block_entity_types);
        schema.register_type(true, &References::ITEM_STACK, |s| {
            DSL::hook(
                &DSL::optional_fields(&[
                    ("id", References::ITEM_NAME.in_(s)),
                    ("tag", v99::item_stack_tag(s)),
                ]),
                &v99::ADD_NAMES,
                &HookFunction::identity(),
            )
        });
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V102)
}
