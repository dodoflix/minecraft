//! `net.minecraft.util.datafix.schemas.V1451_2`.

use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::schema::{Schema, SchemaDef, TemplateMap};

pub struct V1451_2;

impl SchemaDef for V1451_2 {
    fn register_block_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = this.super_register_block_entities(schema);
        schema.register_with(&mut map, "minecraft:piston", |s| {
            DSL::optional_fields(&[("blockState", References::BLOCK_STATE.in_(s))])
        });
        map
    }
    fn namespaced(&self) -> bool {
        true
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V1451_2)
}
