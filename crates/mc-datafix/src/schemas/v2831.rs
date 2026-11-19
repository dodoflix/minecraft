//! `net.minecraft.util.datafix.schemas.V2831`.

use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::schema::{Schema, SchemaDef, TemplateMap};

pub struct V2831;

impl SchemaDef for V2831 {
    fn register_types(
        &self,
        this: &Schema,
        schema: &Schema,
        entity_types: &TemplateMap,
        block_entity_types: &TemplateMap,
    ) {
        this.super_register_types(schema, entity_types, block_entity_types);
        schema.register_type(true, &References::UNTAGGED_SPAWNER, |s| {
            DSL::optional_fields(&[
                (
                    "SpawnPotentials",
                    DSL::list(&DSL::fields(&[(
                        "data",
                        DSL::fields(&[("entity", References::ENTITY_TREE.in_(s))]),
                    )])),
                ),
                (
                    "SpawnData",
                    DSL::fields(&[("entity", References::ENTITY_TREE.in_(s))]),
                ),
            ])
        });
    }
    fn namespaced(&self) -> bool {
        true
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V2831)
}
