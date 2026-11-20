//! `net.minecraft.util.datafix.schemas.V3818`.

use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::schema::{Schema, SchemaDef, TemplateMap};

pub struct V3818;

impl SchemaDef for V3818 {
    fn register_block_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = this.super_register_block_entities(schema);
        schema.register_with(&mut map, "minecraft:beehive", |s| {
            DSL::optional_fields(&[(
                "bees",
                DSL::list(&DSL::optional_fields(&[(
                    "entity_data",
                    References::ENTITY_TREE.in_(s),
                )])),
            )])
        });
        map
    }
    fn namespaced(&self) -> bool {
        true
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V3818)
}
