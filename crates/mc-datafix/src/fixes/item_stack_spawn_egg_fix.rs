//! `ItemStackSpawnEggFix`: gives spawn eggs of one item id the id of the spawn egg of their `EntityTag`.

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::schema::Schema;
use crate::value::Value;

const MAP: [(&str, &str); 49] = [
    ("minecraft:bat", "minecraft:bat_spawn_egg"),
    ("minecraft:blaze", "minecraft:blaze_spawn_egg"),
    ("minecraft:cave_spider", "minecraft:cave_spider_spawn_egg"),
    ("minecraft:chicken", "minecraft:chicken_spawn_egg"),
    ("minecraft:cow", "minecraft:cow_spawn_egg"),
    ("minecraft:creeper", "minecraft:creeper_spawn_egg"),
    ("minecraft:donkey", "minecraft:donkey_spawn_egg"),
    (
        "minecraft:elder_guardian",
        "minecraft:elder_guardian_spawn_egg",
    ),
    ("minecraft:ender_dragon", "minecraft:ender_dragon_spawn_egg"),
    ("minecraft:enderman", "minecraft:enderman_spawn_egg"),
    ("minecraft:endermite", "minecraft:endermite_spawn_egg"),
    (
        "minecraft:evocation_illager",
        "minecraft:evocation_illager_spawn_egg",
    ),
    ("minecraft:ghast", "minecraft:ghast_spawn_egg"),
    ("minecraft:guardian", "minecraft:guardian_spawn_egg"),
    ("minecraft:horse", "minecraft:horse_spawn_egg"),
    ("minecraft:husk", "minecraft:husk_spawn_egg"),
    ("minecraft:iron_golem", "minecraft:iron_golem_spawn_egg"),
    ("minecraft:llama", "minecraft:llama_spawn_egg"),
    ("minecraft:magma_cube", "minecraft:magma_cube_spawn_egg"),
    ("minecraft:mooshroom", "minecraft:mooshroom_spawn_egg"),
    ("minecraft:mule", "minecraft:mule_spawn_egg"),
    ("minecraft:ocelot", "minecraft:ocelot_spawn_egg"),
    ("minecraft:pufferfish", "minecraft:pufferfish_spawn_egg"),
    ("minecraft:parrot", "minecraft:parrot_spawn_egg"),
    ("minecraft:pig", "minecraft:pig_spawn_egg"),
    ("minecraft:polar_bear", "minecraft:polar_bear_spawn_egg"),
    ("minecraft:rabbit", "minecraft:rabbit_spawn_egg"),
    ("minecraft:sheep", "minecraft:sheep_spawn_egg"),
    ("minecraft:shulker", "minecraft:shulker_spawn_egg"),
    ("minecraft:silverfish", "minecraft:silverfish_spawn_egg"),
    ("minecraft:skeleton", "minecraft:skeleton_spawn_egg"),
    (
        "minecraft:skeleton_horse",
        "minecraft:skeleton_horse_spawn_egg",
    ),
    ("minecraft:slime", "minecraft:slime_spawn_egg"),
    ("minecraft:snow_golem", "minecraft:snow_golem_spawn_egg"),
    ("minecraft:spider", "minecraft:spider_spawn_egg"),
    ("minecraft:squid", "minecraft:squid_spawn_egg"),
    ("minecraft:stray", "minecraft:stray_spawn_egg"),
    ("minecraft:turtle", "minecraft:turtle_spawn_egg"),
    ("minecraft:vex", "minecraft:vex_spawn_egg"),
    ("minecraft:villager", "minecraft:villager_spawn_egg"),
    (
        "minecraft:vindication_illager",
        "minecraft:vindication_illager_spawn_egg",
    ),
    ("minecraft:witch", "minecraft:witch_spawn_egg"),
    ("minecraft:wither", "minecraft:wither_spawn_egg"),
    (
        "minecraft:wither_skeleton",
        "minecraft:wither_skeleton_spawn_egg",
    ),
    ("minecraft:wolf", "minecraft:wolf_spawn_egg"),
    ("minecraft:zombie", "minecraft:zombie_spawn_egg"),
    ("minecraft:zombie_horse", "minecraft:zombie_horse_spawn_egg"),
    (
        "minecraft:zombie_pigman",
        "minecraft:zombie_pigman_spawn_egg",
    ),
    (
        "minecraft:zombie_villager",
        "minecraft:zombie_villager_spawn_egg",
    ),
];

/// `new ItemStackSpawnEggFix(outputSchema, changesType, itemType)`.
pub fn new(output_schema: &Schema, changes_type: bool, item_type: &str) -> DataFix {
    let item_type = item_type.to_string();
    DataFix::new(
        "ItemStackSpawnEggFix",
        output_schema,
        changes_type,
        move |df| {
            let item_stack_type = df.input_schema().get_type(&References::ITEM_STACK);
            let id_f = DSL::field_finder(
                "id",
                &DSL::named_type(References::ITEM_NAME.type_name(), &DSL::namespaced_string()),
            );
            let entity_id_f = DSL::field_finder("id", &DSL::namespaced_string());
            let tag_f = item_stack_type.find_field("tag");
            let entity_f = tag_f.ty().find_field("EntityTag");
            let item_type = item_type.clone();
            df.fix_type_everywhere_typed(
                &format!(
                    "ItemInstanceSpawnEggFix{}",
                    df.output_schema().version_key()
                ),
                &item_stack_type,
                move |input| {
                    if let Some(id) = input.get_optional(&id_f)
                        && id.second().as_str() == item_type
                    {
                        let tag = input.get_or_create_typed(&tag_f);
                        let entity = tag.get_or_create_typed(&entity_f);
                        if let Some(entity_id) = entity.get_optional(&entity_id_f) {
                            let egg = MAP
                                .iter()
                                .find(|(key, _)| *key == entity_id.as_str())
                                .map_or("minecraft:pig_spawn_egg", |(_, egg)| *egg);
                            return input.set(
                                &id_f,
                                Value::named(References::ITEM_NAME.type_name(), Value::str(egg)),
                            );
                        }
                    }
                    input
                },
            )
        },
    )
}
