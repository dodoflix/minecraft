//! `net.minecraft.util.datafix.schemas.V3938`.

use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::schema::{Schema, SchemaDef, TemplateMap};
use crate::template::Template;

pub struct V3938;

impl SchemaDef for V3938 {
    fn register_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = this.super_register_entities(schema);
        schema.register_with(&mut map, "minecraft:spectral_arrow", abstract_arrow);
        schema.register_with(&mut map, "minecraft:arrow", abstract_arrow);
        map
    }
    fn namespaced(&self) -> bool {
        true
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V3938)
}

fn abstract_arrow(schema: &Schema) -> Template {
    DSL::optional_fields(&[
        ("inBlockState", References::BLOCK_STATE.in_(schema)),
        ("item", References::ITEM_STACK.in_(schema)),
        ("weapon", References::ITEM_STACK.in_(schema)),
    ])
}
