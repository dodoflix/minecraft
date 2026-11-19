//! `net.minecraft.util.datafix.schemas.V1510`.

use crate::schema::{Schema, SchemaDef, TemplateMap};

fn move_entry(map: &mut TemplateMap, old: &str, new: &str) {
    let supplier = map
        .remove(old)
        .unwrap_or_else(|| panic!("Didn't find {old} in schema"));
    map.put_supplier(new, supplier);
}

pub struct V1510;

impl SchemaDef for V1510 {
    fn register_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = this.super_register_entities(schema);
        move_entry(
            &mut map,
            "minecraft:commandblock_minecart",
            "minecraft:command_block_minecart",
        );
        move_entry(&mut map, "minecraft:ender_crystal", "minecraft:end_crystal");
        move_entry(&mut map, "minecraft:snowman", "minecraft:snow_golem");
        move_entry(&mut map, "minecraft:evocation_illager", "minecraft:evoker");
        move_entry(
            &mut map,
            "minecraft:evocation_fangs",
            "minecraft:evoker_fangs",
        );
        move_entry(
            &mut map,
            "minecraft:illusion_illager",
            "minecraft:illusioner",
        );
        move_entry(
            &mut map,
            "minecraft:vindication_illager",
            "minecraft:vindicator",
        );
        move_entry(&mut map, "minecraft:villager_golem", "minecraft:iron_golem");
        move_entry(&mut map, "minecraft:xp_orb", "minecraft:experience_orb");
        move_entry(
            &mut map,
            "minecraft:xp_bottle",
            "minecraft:experience_bottle",
        );
        move_entry(
            &mut map,
            "minecraft:eye_of_ender_signal",
            "minecraft:eye_of_ender",
        );
        move_entry(
            &mut map,
            "minecraft:fireworks_rocket",
            "minecraft:firework_rocket",
        );
        map
    }
    fn namespaced(&self) -> bool {
        true
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V1510)
}
