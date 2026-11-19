//! `net.minecraft.util.datafix.schemas.V2501`.

use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::schema::{Schema, SchemaDef, TemplateMap};

fn register_furnace(schema: &Schema, map: &mut TemplateMap, name: &str) {
    schema.register_with(map, name, |s| {
        DSL::optional_fields(&[
            ("Items", DSL::list(&References::ITEM_STACK.in_(s))),
            ("CustomName", References::TEXT_COMPONENT.in_(s)),
            (
                "RecipesUsed",
                DSL::compound_list_kv(
                    &References::RECIPE.in_(s),
                    &DSL::const_type(&DSL::int_type()),
                ),
            ),
        ])
    });
}

pub struct V2501;

impl SchemaDef for V2501 {
    fn register_block_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = this.super_register_block_entities(schema);
        register_furnace(schema, &mut map, "minecraft:furnace");
        register_furnace(schema, &mut map, "minecraft:smoker");
        register_furnace(schema, &mut map, "minecraft:blast_furnace");
        map
    }
    fn namespaced(&self) -> bool {
        true
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V2501)
}
