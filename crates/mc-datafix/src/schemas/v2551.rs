//! `net.minecraft.util.datafix.schemas.V2551`.

use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::schema::{Schema, SchemaDef, TemplateMap};
use crate::template::Template;

/// The `WORLD_GEN_SETTINGS` template shared by V2551 and V2832, which differ in the `minecraft:noise` biome sources.
pub(crate) fn world_gen_settings(schema: &Schema, biome_sources: TemplateMap) -> Template {
    let mut generators = TemplateMap::new();
    generators.put("minecraft:debug", |_| DSL::remainder());
    generators.put("minecraft:flat", |s| {
        DSL::optional_fields(&[(
            "settings",
            DSL::optional_fields(&[
                ("biome", References::BIOME.in_(s)),
                (
                    "layers",
                    DSL::list(&DSL::optional_fields(&[(
                        "block",
                        References::BLOCK_NAME.in_(s),
                    )])),
                ),
            ]),
        )])
    });
    generators.put("minecraft:noise", move |s| {
        DSL::optional_fields(&[
            (
                "biome_source",
                DSL::tagged_choice_lazy("type", &DSL::string(), &biome_sources, s),
            ),
            (
                "settings",
                DSL::or(
                    &DSL::const_type(&DSL::string()),
                    &DSL::optional_fields(&[
                        ("default_block", References::BLOCK_NAME.in_(s)),
                        ("default_fluid", References::BLOCK_NAME.in_(s)),
                    ]),
                ),
            ),
        ])
    });
    DSL::fields(&[(
        "dimensions",
        DSL::compound_list_kv(
            &DSL::const_type(&DSL::namespaced_string()),
            &DSL::fields(&[(
                "generator",
                DSL::tagged_choice_lazy("type", &DSL::string(), &generators, schema),
            )]),
        ),
    )])
}

pub struct V2551;

impl SchemaDef for V2551 {
    fn register_types(
        &self,
        this: &Schema,
        schema: &Schema,
        entity_types: &TemplateMap,
        block_entity_types: &TemplateMap,
    ) {
        this.super_register_types(schema, entity_types, block_entity_types);
        schema.register_type(false, &References::WORLD_GEN_SETTINGS, |s| {
            let mut biome_sources = TemplateMap::new();
            biome_sources.put("minecraft:fixed", |s| {
                DSL::fields(&[("biome", References::BIOME.in_(s))])
            });
            biome_sources.put("minecraft:multi_noise", |s| {
                DSL::list(&DSL::fields(&[("biome", References::BIOME.in_(s))]))
            });
            biome_sources.put("minecraft:checkerboard", |s| {
                DSL::fields(&[("biomes", DSL::list(&References::BIOME.in_(s)))])
            });
            biome_sources.put("minecraft:vanilla_layered", |_| DSL::remainder());
            biome_sources.put("minecraft:the_end", |_| DSL::remainder());
            world_gen_settings(s, biome_sources)
        });
    }
    fn namespaced(&self) -> bool {
        true
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V2551)
}
