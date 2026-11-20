//! `net.minecraft.util.datafix.schemas.V3813`.

use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::schema::{Schema, SchemaDef, TemplateMap};

pub struct V3813;

impl SchemaDef for V3813 {
    fn register_types(
        &self,
        this: &Schema,
        schema: &Schema,
        entity_types: &TemplateMap,
        block_entity_types: &TemplateMap,
    ) {
        this.super_register_types(schema, entity_types, block_entity_types);
        schema.register_type(false, &References::SAVED_DATA_MAP_DATA, |s| {
            DSL::optional_fields(&[(
                "data",
                DSL::optional_fields(&[(
                    "banners",
                    DSL::list(&DSL::optional_fields(&[(
                        "name",
                        References::TEXT_COMPONENT.in_(s),
                    )])),
                )]),
            )])
        });
    }
    fn namespaced(&self) -> bool {
        true
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V3813)
}
