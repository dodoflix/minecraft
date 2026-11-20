//! `net.minecraft.util.datafix.schemas.V4307`.

use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::schema::{Schema, SchemaDef, TemplateMap};
use crate::schemas::v3818_3::Components;
use crate::schemas::v4059;
use crate::template::Template;

pub struct V4307;

impl SchemaDef for V4307 {
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
    Schema::new(key, parent, V4307)
}

fn adventure_mode_predicate(schema: &Schema) -> Template {
    let predicate = || {
        DSL::optional_fields(&[(
            "blocks",
            DSL::or(
                &References::BLOCK_NAME.in_(schema),
                &DSL::list(&References::BLOCK_NAME.in_(schema)),
            ),
        )])
    };
    DSL::or(&predicate(), &DSL::list(&predicate()))
}

/// `V4307.components`.
pub(crate) fn components() -> Components {
    let mut components = v4059::components();
    components.put("minecraft:can_place_on", adventure_mode_predicate);
    components.put("minecraft:can_break", adventure_mode_predicate);
    components
}
