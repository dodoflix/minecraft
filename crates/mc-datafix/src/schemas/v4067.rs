//! `net.minecraft.util.datafix.schemas.V4067`.

use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::schema::{Schema, SchemaDef, TemplateMap};

pub struct V4067;

impl SchemaDef for V4067 {
    fn register_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = this.super_register_entities(schema);
        map.remove("minecraft:boat");
        map.remove("minecraft:chest_boat");
        schema.register_simple(&mut map, "minecraft:oak_boat");
        schema.register_simple(&mut map, "minecraft:spruce_boat");
        schema.register_simple(&mut map, "minecraft:birch_boat");
        schema.register_simple(&mut map, "minecraft:jungle_boat");
        schema.register_simple(&mut map, "minecraft:acacia_boat");
        schema.register_simple(&mut map, "minecraft:cherry_boat");
        schema.register_simple(&mut map, "minecraft:dark_oak_boat");
        schema.register_simple(&mut map, "minecraft:mangrove_boat");
        schema.register_simple(&mut map, "minecraft:bamboo_raft");
        register_chest_boat(this, &mut map, "minecraft:oak_chest_boat");
        register_chest_boat(this, &mut map, "minecraft:spruce_chest_boat");
        register_chest_boat(this, &mut map, "minecraft:birch_chest_boat");
        register_chest_boat(this, &mut map, "minecraft:jungle_chest_boat");
        register_chest_boat(this, &mut map, "minecraft:acacia_chest_boat");
        register_chest_boat(this, &mut map, "minecraft:cherry_chest_boat");
        register_chest_boat(this, &mut map, "minecraft:dark_oak_chest_boat");
        register_chest_boat(this, &mut map, "minecraft:mangrove_chest_boat");
        register_chest_boat(this, &mut map, "minecraft:bamboo_chest_raft");
        map
    }
    fn namespaced(&self) -> bool {
        true
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V4067)
}

/// `V4067.registerChestBoat`: the item stack type is looked up in this schema, not in the one being built.
fn register_chest_boat(this: &Schema, map: &mut TemplateMap, id: &str) {
    let this_schema = this.clone();
    this.register_with(map, id, move |_| {
        DSL::optional_fields(&[(
            "Items",
            DSL::list(&References::ITEM_STACK.in_(&this_schema)),
        )])
    });
}
