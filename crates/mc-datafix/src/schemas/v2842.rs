//! `net.minecraft.util.datafix.schemas.V2842`.

use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::schema::{Schema, SchemaDef, TemplateMap};

pub struct V2842;

impl SchemaDef for V2842 {
    fn register_types(
        &self,
        this: &Schema,
        schema: &Schema,
        entity_types: &TemplateMap,
        block_entity_types: &TemplateMap,
    ) {
        this.super_register_types(schema, entity_types, block_entity_types);
        schema.register_type(false, &References::CHUNK, |s| {
            DSL::optional_fields(&[
                ("entities", DSL::list(&References::ENTITY_TREE.in_(s))),
                (
                    "block_entities",
                    DSL::list(&DSL::or(
                        &References::BLOCK_ENTITY.in_(s),
                        &DSL::remainder(),
                    )),
                ),
                (
                    "block_ticks",
                    DSL::list(&DSL::fields(&[("i", References::BLOCK_NAME.in_(s))])),
                ),
                (
                    "sections",
                    DSL::list(&DSL::optional_fields(&[
                        (
                            "biomes",
                            DSL::optional_fields(&[(
                                "palette",
                                DSL::list(&References::BIOME.in_(s)),
                            )]),
                        ),
                        (
                            "block_states",
                            DSL::optional_fields(&[(
                                "palette",
                                DSL::list(&References::BLOCK_STATE.in_(s)),
                            )]),
                        ),
                    ])),
                ),
                (
                    "structures",
                    DSL::optional_fields(&[(
                        "starts",
                        DSL::compound_list(&References::STRUCTURE_FEATURE.in_(s)),
                    )]),
                ),
            ])
        });
    }
    fn namespaced(&self) -> bool {
        true
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V2842)
}
