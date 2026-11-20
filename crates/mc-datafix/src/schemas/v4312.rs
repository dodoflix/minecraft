//! `net.minecraft.util.datafix.schemas.V4312`.

use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::schema::{Schema, SchemaDef, TemplateMap};

pub struct V4312;

impl SchemaDef for V4312 {
    fn register_types(
        &self,
        this: &Schema,
        schema: &Schema,
        entity_types: &TemplateMap,
        block_entity_types: &TemplateMap,
    ) {
        this.super_register_types(schema, entity_types, block_entity_types);
        schema.register_type(false, &References::PLAYER, |s| {
            DSL::and(
                &References::ENTITY_EQUIPMENT.in_(s),
                &DSL::optional_fields(&[
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
                ]),
            )
        });
    }
    fn namespaced(&self) -> bool {
        true
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V4312)
}
