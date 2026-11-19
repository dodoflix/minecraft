//! `net.minecraft.util.datafix.schemas.V1488`.

use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::schema::{Schema, SchemaDef, TemplateMap};

pub struct V1488;

impl SchemaDef for V1488 {
    fn register_block_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = this.super_register_block_entities(schema);
        schema.register_with(&mut map, "minecraft:command_block", |s| {
            DSL::optional_fields(&[
                ("CustomName", References::TEXT_COMPONENT.in_(s)),
                ("LastOutput", References::TEXT_COMPONENT.in_(s)),
            ])
        });
        map
    }
    fn namespaced(&self) -> bool {
        true
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V1488)
}
