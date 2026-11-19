//! `net.minecraft.util.datafix.schemas.V2832`.

use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::schema::{Schema, SchemaDef, TemplateMap};
use crate::schemas::v2551::world_gen_settings;

pub struct V2832;

impl SchemaDef for V2832 {
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
                        "Structures",
                        DSL::optional_fields(&[(
                            "Starts",
                            DSL::compound_list(&References::STRUCTURE_FEATURE.in_(s)),
                        )]),
                    ),
                ]),
            )])
        });
        schema.register_type(
            false,
            &References::MULTI_NOISE_BIOME_SOURCE_PARAMETER_LIST,
            |_| DSL::const_type(&DSL::namespaced_string()),
        );
        schema.register_type(false, &References::WORLD_GEN_SETTINGS, |s| {
            let mut biome_sources = TemplateMap::new();
            biome_sources.put("minecraft:fixed", |s| {
                DSL::fields(&[("biome", References::BIOME.in_(s))])
            });
            biome_sources.put("minecraft:multi_noise", |s| {
                DSL::or(
                    &DSL::fields(&[(
                        "preset",
                        References::MULTI_NOISE_BIOME_SOURCE_PARAMETER_LIST.in_(s),
                    )]),
                    &DSL::list(&DSL::fields(&[("biome", References::BIOME.in_(s))])),
                )
            });
            biome_sources.put("minecraft:checkerboard", |s| {
                DSL::fields(&[("biomes", DSL::list(&References::BIOME.in_(s)))])
            });
            biome_sources.put("minecraft:the_end", |_| DSL::remainder());
            world_gen_settings(s, biome_sources)
        });
    }
    fn namespaced(&self) -> bool {
        true
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V2832)
}
