//! `net.minecraft.util.datafix.schemas.V3685`.

use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::schema::{Schema, SchemaDef, TemplateMap};
use crate::template::Template;

pub struct V3685;

impl SchemaDef for V3685 {
    fn register_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = this.super_register_entities(schema);
        schema.register_with(&mut map, "minecraft:trident", abstract_arrow);
        schema.register_with(&mut map, "minecraft:spectral_arrow", abstract_arrow);
        schema.register_with(&mut map, "minecraft:arrow", abstract_arrow);
        map
    }
    fn namespaced(&self) -> bool {
        true
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V3685)
}

fn abstract_arrow(schema: &Schema) -> Template {
    DSL::optional_fields(&[
        ("inBlockState", References::BLOCK_STATE.in_(schema)),
        ("item", References::ITEM_STACK.in_(schema)),
    ])
}
