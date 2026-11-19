//! `net.minecraft.util.datafix.schemas.V1800`.

use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::schema::{Schema, SchemaDef, TemplateMap};

pub struct V1800;

impl SchemaDef for V1800 {
    fn register_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = this.super_register_entities(schema);
        schema.register_simple(&mut map, "minecraft:panda");
        schema.register_with(&mut map, "minecraft:pillager", |s| {
            DSL::optional_fields(&[("Inventory", DSL::list(&References::ITEM_STACK.in_(s)))])
        });
        map
    }
    fn namespaced(&self) -> bool {
        true
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V1800)
}
