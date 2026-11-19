//! `net.minecraft.util.datafix.schemas.V2684`.

use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::schema::{Schema, SchemaDef, TemplateMap};

pub struct V2684;

impl SchemaDef for V2684 {
    fn register_block_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = this.super_register_block_entities(schema);
        schema.register_with(&mut map, "minecraft:sculk_sensor", |s| {
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
    fn register_types(
        &self,
        this: &Schema,
        schema: &Schema,
        entity_types: &TemplateMap,
        block_entity_types: &TemplateMap,
    ) {
        this.super_register_types(schema, entity_types, block_entity_types);
        schema.register_type(false, &References::GAME_EVENT_NAME, |_| {
            DSL::const_type(&DSL::namespaced_string())
        });
    }
    fn namespaced(&self) -> bool {
        true
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V2684)
}
