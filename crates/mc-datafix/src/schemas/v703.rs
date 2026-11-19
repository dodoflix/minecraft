//! `net.minecraft.util.datafix.schemas.V703`.

use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::schema::{Schema, SchemaDef, TemplateMap};

pub struct V703;

impl SchemaDef for V703 {
    fn register_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = this.super_register_entities(schema);
        map.remove("EntityHorse");
        schema.register_with(&mut map, "Horse", |s| {
            DSL::optional_fields(&[
                ("ArmorItem", References::ITEM_STACK.in_(s)),
                ("SaddleItem", References::ITEM_STACK.in_(s)),
            ])
        });
        schema.register_with(&mut map, "Donkey", |s| {
            DSL::optional_fields(&[
                ("Items", DSL::list(&References::ITEM_STACK.in_(s))),
                ("SaddleItem", References::ITEM_STACK.in_(s)),
            ])
        });
        schema.register_with(&mut map, "Mule", |s| {
            DSL::optional_fields(&[
                ("Items", DSL::list(&References::ITEM_STACK.in_(s))),
                ("SaddleItem", References::ITEM_STACK.in_(s)),
            ])
        });
        schema.register_with(&mut map, "ZombieHorse", |s| {
            DSL::optional_fields(&[("SaddleItem", References::ITEM_STACK.in_(s))])
        });
        schema.register_with(&mut map, "SkeletonHorse", |s| {
            DSL::optional_fields(&[("SaddleItem", References::ITEM_STACK.in_(s))])
        });
        map
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V703)
}
