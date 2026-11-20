//! `net.minecraft.util.datafix.schemas.V4996_1`.

use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::schema::{Schema, SchemaDef, TemplateMap};

pub struct V4996_1;

impl SchemaDef for V4996_1 {
    fn register_block_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = this.super_register_block_entities(schema);
        schema.register_with(&mut map, "minecraft:decorated_pot", |s| {
            DSL::optional_fields(&[
                (
                    "sherds",
                    DSL::optional_fields(&[
                        ("back", References::ITEM_STACK.in_(s)),
                        ("left", References::ITEM_STACK.in_(s)),
                        ("right", References::ITEM_STACK.in_(s)),
                        ("front", References::ITEM_STACK.in_(s)),
                    ]),
                ),
                ("item", References::ITEM_STACK.in_(s)),
            ])
        });
        map
    }
    fn namespaced(&self) -> bool {
        true
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V4996_1)
}
