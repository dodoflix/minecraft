//! `net.minecraft.util.datafix.schemas.V1470`.

use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::schema::{Schema, SchemaDef, TemplateMap};

fn register_mob(schema: &Schema, map: &mut TemplateMap, name: &str) {
    schema.register_simple(map, name);
}

pub struct V1470;

impl SchemaDef for V1470 {
    fn register_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = this.super_register_entities(schema);
        register_mob(schema, &mut map, "minecraft:turtle");
        register_mob(schema, &mut map, "minecraft:cod_mob");
        register_mob(schema, &mut map, "minecraft:tropical_fish");
        register_mob(schema, &mut map, "minecraft:salmon_mob");
        register_mob(schema, &mut map, "minecraft:puffer_fish");
        register_mob(schema, &mut map, "minecraft:phantom");
        register_mob(schema, &mut map, "minecraft:dolphin");
        register_mob(schema, &mut map, "minecraft:drowned");
        schema.register_with(&mut map, "minecraft:trident", |s| {
            DSL::optional_fields(&[
                ("inBlockState", References::BLOCK_STATE.in_(s)),
                ("Trident", References::ITEM_STACK.in_(s)),
            ])
        });
        map
    }
    fn namespaced(&self) -> bool {
        true
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V1470)
}
