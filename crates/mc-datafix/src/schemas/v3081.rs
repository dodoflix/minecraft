//! `net.minecraft.util.datafix.schemas.V3081`.

use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::schema::{Schema, SchemaDef, TemplateMap};

pub struct V3081;

impl SchemaDef for V3081 {
    fn register_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = this.super_register_entities(schema);
        schema.register_with(&mut map, "minecraft:warden", |s| {
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
    Schema::new(key, parent, V3081)
}
