//! `net.minecraft.util.datafix.schemas.V1125`.

use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::schema::{Schema, SchemaDef, TemplateMap};

pub struct V1125;

impl SchemaDef for V1125 {
    fn register_block_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = this.super_register_block_entities(schema);
        schema.register_simple(&mut map, "minecraft:bed");
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
        schema.register_type(false, &References::ADVANCEMENTS, |s| {
            DSL::optional_fields(&[
                (
                    "minecraft:adventure/adventuring_time",
                    DSL::optional_fields(&[(
                        "criteria",
                        DSL::compound_list_kv(
                            &References::BIOME.in_(s),
                            &DSL::const_type(&DSL::string()),
                        ),
                    )]),
                ),
                (
                    "minecraft:adventure/kill_a_mob",
                    DSL::optional_fields(&[(
                        "criteria",
                        DSL::compound_list_kv(
                            &References::ENTITY_NAME.in_(s),
                            &DSL::const_type(&DSL::string()),
                        ),
                    )]),
                ),
                (
                    "minecraft:adventure/kill_all_mobs",
                    DSL::optional_fields(&[(
                        "criteria",
                        DSL::compound_list_kv(
                            &References::ENTITY_NAME.in_(s),
                            &DSL::const_type(&DSL::string()),
                        ),
                    )]),
                ),
                (
                    "minecraft:husbandry/bred_all_animals",
                    DSL::optional_fields(&[(
                        "criteria",
                        DSL::compound_list_kv(
                            &References::ENTITY_NAME.in_(s),
                            &DSL::const_type(&DSL::string()),
                        ),
                    )]),
                ),
            ])
        });
        schema.register_type(false, &References::BIOME, |_| {
            DSL::const_type(&DSL::namespaced_string())
        });
        schema.register_type(false, &References::ENTITY_NAME, |_| {
            DSL::const_type(&DSL::namespaced_string())
        });
    }
    fn namespaced(&self) -> bool {
        true
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V1125)
}
