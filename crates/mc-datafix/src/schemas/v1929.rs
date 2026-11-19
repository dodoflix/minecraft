//! `net.minecraft.util.datafix.schemas.V1929`.

use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::schema::{Schema, SchemaDef, TemplateMap};

pub struct V1929;

impl SchemaDef for V1929 {
    fn register_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = this.super_register_entities(schema);
        schema.register_with(&mut map, "minecraft:wandering_trader", |s| {
            DSL::optional_fields(&[
                ("Inventory", DSL::list(&References::ITEM_STACK.in_(s))),
                (
                    "Offers",
                    DSL::optional_fields(&[(
                        "Recipes",
                        DSL::list(&References::VILLAGER_TRADE.in_(s)),
                    )]),
                ),
            ])
        });
        schema.register_with(&mut map, "minecraft:trader_llama", |s| {
            DSL::optional_fields(&[
                ("Items", DSL::list(&References::ITEM_STACK.in_(s))),
                ("SaddleItem", References::ITEM_STACK.in_(s)),
                ("DecorItem", References::ITEM_STACK.in_(s)),
            ])
        });
        map
    }
    fn namespaced(&self) -> bool {
        true
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V1929)
}
