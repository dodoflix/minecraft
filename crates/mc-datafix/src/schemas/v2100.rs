//! `net.minecraft.util.datafix.schemas.V2100`.

use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::schema::{Schema, SchemaDef, TemplateMap};

fn register_mob(schema: &Schema, map: &mut TemplateMap, name: &str) {
    schema.register_simple(map, name);
}

pub struct V2100;

impl SchemaDef for V2100 {
    fn register_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = this.super_register_entities(schema);
        register_mob(schema, &mut map, "minecraft:bee");
        register_mob(schema, &mut map, "minecraft:bee_stinger");
        map
    }
    fn register_block_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = this.super_register_block_entities(schema);
        schema.register_with(&mut map, "minecraft:beehive", |s| {
            DSL::optional_fields(&[(
                "Bees",
                DSL::list(&DSL::optional_fields(&[(
                    "EntityData",
                    References::ENTITY_TREE.in_(s),
                )])),
            )])
        });
        map
    }
    fn namespaced(&self) -> bool {
        true
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V2100)
}
