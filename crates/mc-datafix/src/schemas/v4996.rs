//! `net.minecraft.util.datafix.schemas.V4996`.

use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::schema::{Schema, SchemaDef, TemplateMap};
use crate::schemas::v3818_3::Components;
use crate::schemas::v4059;

pub struct V4996;

impl SchemaDef for V4996 {
    fn register_types(
        &self,
        this: &Schema,
        schema: &Schema,
        entity_types: &TemplateMap,
        block_entity_types: &TemplateMap,
    ) {
        this.super_register_types(schema, entity_types, block_entity_types);
        schema.register_type(true, &References::DATA_COMPONENTS, |s| {
            components().to_template(s)
        });
    }
    fn namespaced(&self) -> bool {
        true
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V4996)
}

/// `V4996.components`.
pub(crate) fn components() -> Components {
    let mut components = v4059::components();
    components.put("minecraft:pot_decorations", |s| {
        DSL::optional_fields(&[
            ("back", References::ITEM_STACK.in_(s)),
            ("left", References::ITEM_STACK.in_(s)),
            ("right", References::ITEM_STACK.in_(s)),
            ("front", References::ITEM_STACK.in_(s)),
        ])
    });
    components
}
