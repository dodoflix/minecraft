//! `net.minecraft.util.datafix.schemas.V3439`.

use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::schema::{Schema, SchemaDef, TemplateMap};
use crate::template::Template;

pub struct V3439;

impl SchemaDef for V3439 {
    fn register_block_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = this.super_register_block_entities(schema);
        schema.register_with(&mut map, "minecraft:sign", sign);
        map
    }
    fn namespaced(&self) -> bool {
        true
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V3439)
}

/// `V3439.sign`.
pub(crate) fn sign(schema: &Schema) -> Template {
    let text = || {
        DSL::optional_fields(&[
            (
                "messages",
                DSL::list(&References::TEXT_COMPONENT.in_(schema)),
            ),
            (
                "filtered_messages",
                DSL::list(&References::TEXT_COMPONENT.in_(schema)),
            ),
        ])
    };
    DSL::optional_fields(&[("front_text", text()), ("back_text", text())])
}
