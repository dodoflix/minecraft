//! `net.minecraft.util.datafix.schemas.V135`.

use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::schema::{Schema, SchemaDef, TemplateMap};

pub struct V135;

impl SchemaDef for V135 {
    fn register_types(
        &self,
        this: &Schema,
        schema: &Schema,
        entity_types: &TemplateMap,
        block_entity_types: &TemplateMap,
    ) {
        this.super_register_types(schema, entity_types, block_entity_types);
        schema.register_type(false, &References::PLAYER, |s| {
            DSL::optional_fields(&[
                (
                    "RootVehicle",
                    DSL::optional_fields(&[("Entity", References::ENTITY_TREE.in_(s))]),
                ),
                ("ender_pearls", DSL::list(&References::ENTITY_TREE.in_(s))),
                ("Inventory", DSL::list(&References::ITEM_STACK.in_(s))),
                ("EnderItems", DSL::list(&References::ITEM_STACK.in_(s))),
            ])
        });
        schema.register_type(true, &References::ENTITY_TREE, |s| {
            DSL::optional_fields_with_rest(
                &[("Passengers", DSL::list(&References::ENTITY_TREE.in_(s)))],
                &References::ENTITY.in_(s),
            )
        });
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V135)
}
