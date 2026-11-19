//! `net.minecraft.util.datafix.schemas.V3078`.

use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::schema::{Schema, SchemaDef, TemplateMap};

fn register_mob(schema: &Schema, map: &mut TemplateMap, name: &str) {
    schema.register_simple(map, name);
}

pub struct V3078;

impl SchemaDef for V3078 {
    fn register_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = this.super_register_entities(schema);
        register_mob(schema, &mut map, "minecraft:frog");
        register_mob(schema, &mut map, "minecraft:tadpole");
        map
    }
    fn register_block_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = this.super_register_block_entities(schema);
        schema.register_with(&mut map, "minecraft:sculk_shrieker", |s| {
            DSL::optional_fields(&[(
                "listener",
                DSL::optional_fields(&[(
                    "event",
                    DSL::optional_fields(&[("game_event", References::GAME_EVENT_NAME.in_(s))]),
                )]),
            )])
        });
        map
    }
    fn namespaced(&self) -> bool {
        true
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V3078)
}
