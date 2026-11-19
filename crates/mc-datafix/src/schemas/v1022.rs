//! `net.minecraft.util.datafix.schemas.V1022`.

use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::schema::{Schema, SchemaDef, TemplateMap};

pub struct V1022;

impl SchemaDef for V1022 {
    fn register_types(
        &self,
        this: &Schema,
        schema: &Schema,
        entity_types: &TemplateMap,
        block_entity_types: &TemplateMap,
    ) {
        this.super_register_types(schema, entity_types, block_entity_types);
        schema.register_type(false, &References::RECIPE, |_| {
            DSL::const_type(&DSL::namespaced_string())
        });
        schema.register_type(false, &References::PLAYER, |s| {
            DSL::optional_fields(&[
                (
                    "RootVehicle",
                    DSL::optional_fields(&[("Entity", References::ENTITY_TREE.in_(s))]),
                ),
                ("ender_pearls", DSL::list(&References::ENTITY_TREE.in_(s))),
                ("Inventory", DSL::list(&References::ITEM_STACK.in_(s))),
                ("EnderItems", DSL::list(&References::ITEM_STACK.in_(s))),
                ("ShoulderEntityLeft", References::ENTITY_TREE.in_(s)),
                ("ShoulderEntityRight", References::ENTITY_TREE.in_(s)),
                (
                    "recipeBook",
                    DSL::optional_fields(&[
                        ("recipes", DSL::list(&References::RECIPE.in_(s))),
                        ("toBeDisplayed", DSL::list(&References::RECIPE.in_(s))),
                    ]),
                ),
            ])
        });
        schema.register_type(false, &References::HOTBAR, |s| {
            DSL::compound_list(&DSL::list(&References::ITEM_STACK.in_(s)))
        });
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V1022)
}
