//! `net.minecraft.util.datafix.schemas.V4059`.

use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::schema::{Schema, SchemaDef, TemplateMap};
use crate::schemas::v3818_3::{self, Components};

pub struct V4059;

impl SchemaDef for V4059 {
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
    Schema::new(key, parent, V4059)
}

/// `V4059.components`.
pub(crate) fn components() -> Components {
    let mut components = v3818_3::components();
    components.remove("minecraft:food");
    components.put("minecraft:use_remainder", |s| References::ITEM_STACK.in_(s));
    components.put("minecraft:equippable", |s| {
        DSL::optional_fields(&[(
            "allowed_entities",
            DSL::or(
                &References::ENTITY_NAME.in_(s),
                &DSL::list(&References::ENTITY_NAME.in_(s)),
            ),
        )])
    });
    components.put("minecraft:sulfur_cube_content", |s| {
        References::ITEM_STACK.in_(s)
    });
    components
}
