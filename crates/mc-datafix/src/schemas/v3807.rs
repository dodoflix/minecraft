//! `net.minecraft.util.datafix.schemas.V3807`.

use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::schema::{Schema, SchemaDef, TemplateMap};

pub struct V3807;

impl SchemaDef for V3807 {
    fn register_block_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = this.super_register_block_entities(schema);
        schema.register_with(&mut map, "minecraft:vault", |s| {
            DSL::optional_fields(&[
                (
                    "config",
                    DSL::optional_fields(&[("key_item", References::ITEM_STACK.in_(s))]),
                ),
                (
                    "server_data",
                    DSL::optional_fields(&[(
                        "items_to_eject",
                        DSL::list(&References::ITEM_STACK.in_(s)),
                    )]),
                ),
                (
                    "shared_data",
                    DSL::optional_fields(&[("display_item", References::ITEM_STACK.in_(s))]),
                ),
            ])
        });
        map
    }
    fn namespaced(&self) -> bool {
        true
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V3807)
}
