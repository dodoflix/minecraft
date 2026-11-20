//! `net.minecraft.util.datafix.schemas.V4302`.

use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::schema::{Schema, SchemaDef, TemplateMap};

pub struct V4302;

impl SchemaDef for V4302 {
    fn register_block_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = this.super_register_block_entities(schema);
        schema.register_simple(&mut map, "minecraft:test_block");
        schema.register_with(&mut map, "minecraft:test_instance_block", |s| {
            DSL::optional_fields(&[
                (
                    "data",
                    DSL::optional_fields(&[("error_message", References::TEXT_COMPONENT.in_(s))]),
                ),
                (
                    "errors",
                    DSL::list(&DSL::optional_fields(&[(
                        "text",
                        References::TEXT_COMPONENT.in_(s),
                    )])),
                ),
            ])
        });
        map
    }
    fn namespaced(&self) -> bool {
        true
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V4302)
}
