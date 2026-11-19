//! `net.minecraft.util.datafix.schemas.V702`.

use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::schema::{Schema, SchemaDef, TemplateMap};

pub struct V702;

impl SchemaDef for V702 {
    fn register_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = this.super_register_entities(schema);
        schema.register_with(&mut map, "ZombieVillager", |s| {
            DSL::optional_fields(&[(
                "Offers",
                DSL::optional_fields(&[("Recipes", DSL::list(&References::VILLAGER_TRADE.in_(s)))]),
            )])
        });
        schema.register_simple(&mut map, "Husk");
        map
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V702)
}
