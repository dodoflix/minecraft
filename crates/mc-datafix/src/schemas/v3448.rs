//! `net.minecraft.util.datafix.schemas.V3448`.

use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::schema::{Schema, SchemaDef, TemplateMap};

pub struct V3448;

impl SchemaDef for V3448 {
    fn register_block_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = this.super_register_block_entities(schema);
        schema.register_with(&mut map, "minecraft:decorated_pot", |s| {
            DSL::optional_fields(&[
                ("sherds", DSL::list(&References::ITEM_NAME.in_(s))),
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
    Schema::new(key, parent, V3448)
}
