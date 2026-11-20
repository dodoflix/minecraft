//! `net.minecraft.util.datafix.schemas.V4292`.

use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::schema::{Schema, SchemaDef, TemplateMap};

pub struct V4292;

impl SchemaDef for V4292 {
    fn register_types(
        &self,
        this: &Schema,
        schema: &Schema,
        entity_types: &TemplateMap,
        block_entity_types: &TemplateMap,
    ) {
        this.super_register_types(schema, entity_types, block_entity_types);
        schema.register_type(true, &References::TEXT_COMPONENT, |s| {
            DSL::or(
                &DSL::or(
                    &DSL::const_type(&DSL::string()),
                    &DSL::list(&References::TEXT_COMPONENT.in_(s)),
                ),
                &DSL::optional_fields(&[
                    ("extra", DSL::list(&References::TEXT_COMPONENT.in_(s))),
                    ("separator", References::TEXT_COMPONENT.in_(s)),
                    (
                        "hover_event",
                        DSL::tagged_choice(
                            "action",
                            &DSL::string(),
                            // Java builds this with `Map.of`, whose iteration order is fixed per JVM run; this is the
                            // order the recorded vanilla types have.
                            vec![
                                (
                                    "show_text".to_string(),
                                    DSL::optional_fields(&[(
                                        "value",
                                        References::TEXT_COMPONENT.in_(s),
                                    )]),
                                ),
                                ("show_item".to_string(), References::ITEM_STACK.in_(s)),
                                (
                                    "show_entity".to_string(),
                                    DSL::optional_fields(&[
                                        ("id", References::ENTITY_NAME.in_(s)),
                                        ("name", References::TEXT_COMPONENT.in_(s)),
                                    ]),
                                ),
                            ],
                        ),
                    ),
                ]),
            )
        });
    }
    fn namespaced(&self) -> bool {
        true
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V4292)
}
