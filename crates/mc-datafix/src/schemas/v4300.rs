//! `net.minecraft.util.datafix.schemas.V4300`.

use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::schema::{Schema, SchemaDef, TemplateMap};
use crate::template::Template;

pub struct V4300;

impl SchemaDef for V4300 {
    fn register_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = this.super_register_entities(schema);
        schema.register_with(&mut map, "minecraft:llama", entity_with_inventory);
        schema.register_with(&mut map, "minecraft:trader_llama", entity_with_inventory);
        schema.register_with(&mut map, "minecraft:donkey", entity_with_inventory);
        schema.register_with(&mut map, "minecraft:mule", entity_with_inventory);
        schema.register_simple(&mut map, "minecraft:horse");
        schema.register_simple(&mut map, "minecraft:skeleton_horse");
        schema.register_simple(&mut map, "minecraft:zombie_horse");
        map
    }
    fn namespaced(&self) -> bool {
        true
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V4300)
}

fn entity_with_inventory(schema: &Schema) -> Template {
    DSL::optional_fields(&[("Items", DSL::list(&References::ITEM_STACK.in_(schema)))])
}
