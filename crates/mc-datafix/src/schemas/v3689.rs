//! `net.minecraft.util.datafix.schemas.V3689`.

use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::schema::{Schema, SchemaDef, TemplateMap};

pub struct V3689;

impl SchemaDef for V3689 {
    fn register_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = this.super_register_entities(schema);
        schema.register_simple(&mut map, "minecraft:breeze");
        schema.register_simple(&mut map, "minecraft:wind_charge");
        schema.register_simple(&mut map, "minecraft:breeze_wind_charge");
        map
    }
    fn register_block_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = this.super_register_block_entities(schema);
        schema.register_with(&mut map, "minecraft:trial_spawner", |s| {
            DSL::optional_fields(&[
                (
                    "spawn_potentials",
                    DSL::list(&DSL::fields(&[(
                        "data",
                        DSL::fields(&[("entity", References::ENTITY_TREE.in_(s))]),
                    )])),
                ),
                (
                    "spawn_data",
                    DSL::fields(&[("entity", References::ENTITY_TREE.in_(s))]),
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
    Schema::new(key, parent, V3689)
}
