//! `net.minecraft.util.datafix.schemas.V1466`.

use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::schema::{Schema, SchemaDef, TemplateMap};

pub struct V1466;

impl SchemaDef for V1466 {
    fn register_block_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = this.super_register_block_entities(schema);
        map.put("DUMMY", |_| DSL::remainder());
        map
    }
    fn register_types(
        &self,
        this: &Schema,
        schema: &Schema,
        entity_types: &TemplateMap,
        block_entity_types: &TemplateMap,
    ) {
        this.super_register_types(schema, entity_types, block_entity_types);
        schema.register_type(false, &References::CHUNK, |s| {
            DSL::fields(&[(
                "Level",
                DSL::optional_fields(&[
                    ("Entities", DSL::list(&References::ENTITY_TREE.in_(s))),
                    (
                        "TileEntities",
                        DSL::list(&DSL::or(
                            &References::BLOCK_ENTITY.in_(s),
                            &DSL::remainder(),
                        )),
                    ),
                    (
                        "TileTicks",
                        DSL::list(&DSL::fields(&[("i", References::BLOCK_NAME.in_(s))])),
                    ),
                    (
                        "Sections",
                        DSL::list(&DSL::optional_fields(&[(
                            "Palette",
                            DSL::list(&References::BLOCK_STATE.in_(s)),
                        )])),
                    ),
                    (
                        "Structures",
                        DSL::optional_fields(&[(
                            "Starts",
                            DSL::compound_list(&References::STRUCTURE_FEATURE.in_(s)),
                        )]),
                    ),
                ]),
            )])
        });
    }
    fn namespaced(&self) -> bool {
        true
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V1466)
}
