//! `net.minecraft.util.datafix.schemas.V3818_5`.

use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::schema::{Schema, SchemaDef, TemplateMap};
use crate::ty::HookFunction;
use std::sync::LazyLock;

/// `V3818_5.UNPACK_PLAIN_ID`: a plain item id string becomes `{id, count: 1}`.
static UNPACK_PLAIN_ID: LazyLock<HookFunction> = LazyLock::new(|| {
    HookFunction::new(|ops, value| {
        if ops.get_string_value(&value).result().is_some() {
            ops.create_map(vec![
                (ops.create_string("id"), value),
                (ops.create_string("count"), ops.create_int(1)),
            ])
        } else {
            value
        }
    })
});

pub struct V3818_5;

impl SchemaDef for V3818_5 {
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
                    ("components", References::DATA_COMPONENTS.in_(s)),
                ]),
                &UNPACK_PLAIN_ID,
                &HookFunction::identity(),
            )
        });
    }
    fn namespaced(&self) -> bool {
        true
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V3818_5)
}
