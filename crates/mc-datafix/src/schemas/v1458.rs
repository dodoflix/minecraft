//! `net.minecraft.util.datafix.schemas.V1458`.

use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::schema::{Schema, SchemaDef, TemplateMap};
use crate::template::Template;

pub struct V1458;

impl SchemaDef for V1458 {
    fn register_block_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = this.super_register_block_entities(schema);
        schema.register_with(&mut map, "minecraft:beacon", nameable);
        schema.register_with(&mut map, "minecraft:banner", nameable);
        schema.register_with(&mut map, "minecraft:brewing_stand", nameable_inventory);
        schema.register_with(&mut map, "minecraft:chest", nameable_inventory);
        schema.register_with(&mut map, "minecraft:trapped_chest", nameable_inventory);
        schema.register_with(&mut map, "minecraft:dispenser", nameable_inventory);
        schema.register_with(&mut map, "minecraft:dropper", nameable_inventory);
        schema.register_with(&mut map, "minecraft:enchanting_table", nameable);
        schema.register_with(&mut map, "minecraft:furnace", nameable_inventory);
        schema.register_with(&mut map, "minecraft:hopper", nameable_inventory);
        schema.register_with(&mut map, "minecraft:shulker_box", nameable_inventory);
        map
    }
    fn register_types(
        &self,
        this: &Schema,
        schema: &Schema,
        entity_types: &TemplateMap,
        block_entity_types: &TemplateMap,
    ) {
        this.super_register_types(schema, entity_types, block_entity_types);
        {
            let entity_types = entity_types.clone();
            schema.register_type(true, &References::ENTITY, move |s| {
                DSL::and(
                    &References::ENTITY_EQUIPMENT.in_(s),
                    &DSL::optional_fields_with_rest(
                        &[("CustomName", References::TEXT_COMPONENT.in_(s))],
                        &DSL::tagged_choice_lazy("id", &DSL::namespaced_string(), &entity_types, s),
                    ),
                )
            });
        }
    }
    fn namespaced(&self) -> bool {
        true
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V1458)
}

/// `V1458.nameableInventory`.
pub(crate) fn nameable_inventory(schema: &Schema) -> Template {
    DSL::optional_fields(&[
        ("Items", DSL::list(&References::ITEM_STACK.in_(schema))),
        ("CustomName", References::TEXT_COMPONENT.in_(schema)),
    ])
}

/// `V1458.nameable`.
pub(crate) fn nameable(schema: &Schema) -> Template {
    DSL::optional_fields(&[("CustomName", References::TEXT_COMPONENT.in_(schema))])
}
