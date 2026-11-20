//! `DataFixers.addFixers`, versions 2700..3299.

use crate::data_fixers::{create_renamer, create_renamer_pair};
use crate::fixer::DataFixerBuilder;
use crate::fixes::caves_and_cliffs_renames;
use crate::fixes::references::References;
use crate::fixes::{
    abstract_arrow_pickup_fix, add_flag_if_not_present_fix, add_new_choices,
    advancements_rename_fix, blending_data_remove_from_nether_end_fix,
    chunk_delete_ignored_light_data_fix, chunk_height_and_biome_fix, chunk_proto_tick_list_fix,
    chunk_renames_fix, criteria_rename_fix, entity_fields_rename_fix,
    entity_goat_missing_state_fix, entity_variant_fix, filtered_books_fix, filtered_signs_fix,
    goat_horn_id_fix, item_rename_fix, item_stack_spawn_egg_fix, legacy_block_rename_fix,
    namespaced_type_rename_fix, options_ambient_occlusion_fix, options_programmer_art_fix,
    overreaching_tick_fix, poi_type_remove_fix, saved_data_feature_pool_element_fix,
    spawner_data_fix, stats_rename_fix, structure_settings_flatten_fix,
    structures_become_configured_fix, variant_rename_fix,
    world_gen_settings_disallow_old_custom_worlds_fix, world_gen_settings_height_and_biome_fix,
};
use crate::schema::Schema;
use crate::schemas::{
    v2704, v2707, v2831, v2832, v2842, v3076, v3078, v3081, v3082, v3083, v3202, v3203, v3204,
};

const GAME_EVENT_RENAMES_3084: [(&str, &str); 15] = [
    ("minecraft:block_press", "minecraft:block_activate"),
    ("minecraft:block_switch", "minecraft:block_activate"),
    ("minecraft:block_unpress", "minecraft:block_deactivate"),
    ("minecraft:block_unswitch", "minecraft:block_deactivate"),
    ("minecraft:drinking_finish", "minecraft:drink"),
    ("minecraft:elytra_free_fall", "minecraft:elytra_glide"),
    ("minecraft:entity_damaged", "minecraft:entity_damage"),
    ("minecraft:entity_dying", "minecraft:entity_die"),
    ("minecraft:entity_killed", "minecraft:entity_die"),
    ("minecraft:mob_interact", "minecraft:entity_interact"),
    ("minecraft:ravager_roar", "minecraft:entity_roar"),
    ("minecraft:ring_bell", "minecraft:block_change"),
    ("minecraft:shulker_close", "minecraft:container_close"),
    ("minecraft:shulker_open", "minecraft:container_open"),
    ("minecraft:wolf_shaking", "minecraft:entity_shake"),
];

const CAT_VARIANTS: [&str; 11] = [
    "minecraft:tabby",
    "minecraft:black",
    "minecraft:red",
    "minecraft:siamese",
    "minecraft:british",
    "minecraft:calico",
    "minecraft:persian",
    "minecraft:ragdoll",
    "minecraft:white",
    "minecraft:jellie",
    "minecraft:all_black",
];

const CAT_ADVANCEMENT_MIGRATION: [(&str, &str); 11] = [
    ("textures/entity/cat/tabby.png", "minecraft:tabby"),
    ("textures/entity/cat/black.png", "minecraft:black"),
    ("textures/entity/cat/red.png", "minecraft:red"),
    ("textures/entity/cat/siamese.png", "minecraft:siamese"),
    (
        "textures/entity/cat/british_shorthair.png",
        "minecraft:british",
    ),
    ("textures/entity/cat/calico.png", "minecraft:calico"),
    ("textures/entity/cat/persian.png", "minecraft:persian"),
    ("textures/entity/cat/ragdoll.png", "minecraft:ragdoll"),
    ("textures/entity/cat/white.png", "minecraft:white"),
    ("textures/entity/cat/jellie.png", "minecraft:jellie"),
    ("textures/entity/cat/all_black.png", "minecraft:all_black"),
];

const FROG_VARIANTS: [&str; 3] = ["minecraft:temperate", "minecraft:warm", "minecraft:cold"];

pub fn add(b: &mut DataFixerBuilder) {
    let v2700 = b.add_schema(2700, Schema::same_namespaced);
    b.add_fixer(legacy_block_rename_fix::create(
        &v2700,
        "Renamed cave vines blocks",
        create_renamer(&[
            ("minecraft:cave_vines_head", "minecraft:cave_vines"),
            ("minecraft:cave_vines_body", "minecraft:cave_vines_plant"),
        ]),
    ));
    let v2701 = b.add_schema(2701, Schema::same_namespaced);
    b.add_fixer(saved_data_feature_pool_element_fix::new(&v2701));
    let v2702 = b.add_schema(2702, Schema::same_namespaced);
    b.add_fixer(abstract_arrow_pickup_fix::new(&v2702));
    let v2704 = b.add_schema(2704, v2704::new);
    b.add_fixer(add_new_choices::new(
        &v2704,
        "Added Goat",
        &References::ENTITY,
    ));
    let v2707 = b.add_schema(2707, v2707::new);
    b.add_fixer(add_new_choices::new(
        &v2707,
        "Added Marker",
        &References::ENTITY,
    ));
    b.add_fixer(add_flag_if_not_present_fix::new(
        &v2707,
        &References::WORLD_GEN_SETTINGS,
        "has_increased_height_already",
        true,
    ));
    let v2710 = b.add_schema(2710, Schema::same_namespaced);
    b.add_fixer(stats_rename_fix::new(
        &v2710,
        "Renamed play_one_minute stat to play_time",
        &[("minecraft:play_one_minute", "minecraft:play_time")],
    ));
    let v2717 = b.add_schema(2717, Schema::same_namespaced);
    b.add_fixer(item_rename_fix::create(
        &v2717,
        "Rename azalea_leaves_flowers",
        create_renamer_pair(
            "minecraft:azalea_leaves_flowers",
            "minecraft:flowering_azalea_leaves",
        ),
    ));
    b.add_fixer(legacy_block_rename_fix::create(
        &v2717,
        "Rename azalea_leaves_flowers items",
        create_renamer_pair(
            "minecraft:azalea_leaves_flowers",
            "minecraft:flowering_azalea_leaves",
        ),
    ));
    let v2825 = b.add_schema(2825, Schema::same_namespaced);
    b.add_fixer(add_flag_if_not_present_fix::new(
        &v2825,
        &References::WORLD_GEN_SETTINGS,
        "has_increased_height_already",
        false,
    ));
    let v2831 = b.add_schema(2831, v2831::new);
    b.add_fixer(spawner_data_fix::new(&v2831));
    let v2832 = b.add_schema(2832, v2832::new);
    b.add_fixer(world_gen_settings_height_and_biome_fix::new(&v2832));
    b.add_fixer(chunk_height_and_biome_fix::new(&v2832));
    let v2833 = b.add_schema(2833, Schema::same_namespaced);
    b.add_fixer(world_gen_settings_disallow_old_custom_worlds_fix::new(
        &v2833,
    ));
    let v2838 = b.add_schema(2838, Schema::same_namespaced);
    b.add_fixer(namespaced_type_rename_fix::new(
        &v2838,
        "Caves and Cliffs biome renames",
        &References::BIOME,
        create_renamer(&caves_and_cliffs_renames::RENAMES),
    ));
    let v2841 = b.add_schema(2841, Schema::same_namespaced);
    b.add_fixer(chunk_proto_tick_list_fix::new(&v2841));
    let v2842 = b.add_schema(2842, v2842::new);
    b.add_fixer(chunk_renames_fix::new(&v2842));
    let v2843 = b.add_schema(2843, Schema::same_namespaced);
    b.add_fixer(overreaching_tick_fix::new(&v2843));
    b.add_fixer(namespaced_type_rename_fix::new(
        &v2843,
        "Remove Deep Warm Ocean",
        &References::BIOME,
        create_renamer_pair("minecraft:deep_warm_ocean", "minecraft:warm_ocean"),
    ));
    let v2846 = b.add_schema(2846, Schema::same_namespaced);
    b.add_fixer(advancements_rename_fix::new(
        "AdvancementsRenameFix",
        &v2846,
        false,
        "Rename some C&C part 2 advancements",
        create_renamer(&[
            (
                "minecraft:husbandry/play_jukebox_in_meadows",
                "minecraft:adventure/play_jukebox_in_meadows",
            ),
            (
                "minecraft:adventure/caves_and_cliff",
                "minecraft:adventure/fall_from_world_height",
            ),
            (
                "minecraft:adventure/ride_strider_in_overworld_lava",
                "minecraft:nether/ride_strider_in_overworld_lava",
            ),
        ]),
    ));
    let v2852 = b.add_schema(2852, Schema::same_namespaced);
    b.add_fixer(world_gen_settings_disallow_old_custom_worlds_fix::new(
        &v2852,
    ));
    let v2967 = b.add_schema(2967, Schema::same_namespaced);
    b.add_fixer(structure_settings_flatten_fix::new(&v2967));
    let v2970 = b.add_schema(2970, Schema::same_namespaced);
    b.add_fixer(structures_become_configured_fix::new(&v2970));
    let v3076 = b.add_schema(3076, v3076::new);
    b.add_fixer(add_new_choices::new(
        &v3076,
        "Added Sculk Catalyst",
        &References::BLOCK_ENTITY,
    ));
    let v3077 = b.add_schema(3077, Schema::same_namespaced);
    b.add_fixer(chunk_delete_ignored_light_data_fix::new(&v3077));
    let v3078 = b.add_schema(3078, v3078::new);
    b.add_fixer(add_new_choices::new(
        &v3078,
        "Added Frog",
        &References::ENTITY,
    ));
    b.add_fixer(add_new_choices::new(
        &v3078,
        "Added Tadpole",
        &References::ENTITY,
    ));
    b.add_fixer(add_new_choices::new(
        &v3078,
        "Added Sculk Shrieker",
        &References::BLOCK_ENTITY,
    ));
    let v3081 = b.add_schema(3081, v3081::new);
    b.add_fixer(add_new_choices::new(
        &v3081,
        "Added Warden",
        &References::ENTITY,
    ));
    let v3082 = b.add_schema(3082, v3082::new);
    b.add_fixer(add_new_choices::new(
        &v3082,
        "Added Chest Boat",
        &References::ENTITY,
    ));
    let v3083 = b.add_schema(3083, v3083::new);
    b.add_fixer(add_new_choices::new(
        &v3083,
        "Added Allay",
        &References::ENTITY,
    ));
    let v3084 = b.add_schema(3084, Schema::same_namespaced);
    b.add_fixer(namespaced_type_rename_fix::new(
        &v3084,
        "game_event_renames_3084",
        &References::GAME_EVENT_NAME,
        create_renamer(&GAME_EVENT_RENAMES_3084),
    ));
    let v3086 = b.add_schema(3086, Schema::same_namespaced);
    b.add_fixer(entity_variant_fix::new(
        &v3086,
        "Change cat variant type",
        &References::ENTITY,
        "minecraft:cat",
        "CatType",
        |id| {
            Some(
                usize::try_from(id)
                    .ok()
                    .and_then(|i| CAT_VARIANTS.get(i))
                    .unwrap_or(&CAT_VARIANTS[0])
                    .to_string(),
            )
        },
    ));
    b.add_fixer(criteria_rename_fix::new(
        &v3086,
        "Migrate cat variant advancement",
        "minecraft:husbandry/complete_catalogue",
        |s| {
            CAT_ADVANCEMENT_MIGRATION
                .iter()
                .find(|(k, _)| *k == s)
                .map_or(s, |(_, v)| v)
                .to_string()
        },
    ));
    let v3087 = b.add_schema(3087, Schema::same_namespaced);
    b.add_fixer(entity_variant_fix::new(
        &v3087,
        "Change frog variant type",
        &References::ENTITY,
        "minecraft:frog",
        "Variant",
        |id| {
            usize::try_from(id)
                .ok()
                .and_then(|i| FROG_VARIANTS.get(i))
                .map(|s| s.to_string())
        },
    ));
    let v3090 = b.add_schema(3090, Schema::same_namespaced);
    b.add_fixer(entity_fields_rename_fix::new(
        &v3090,
        "EntityPaintingFieldsRenameFix",
        "minecraft:painting",
        &[("Motive", "variant"), ("Facing", "facing")],
    ));
    let v3093 = b.add_schema(3093, Schema::same_namespaced);
    b.add_fixer(entity_goat_missing_state_fix::new(&v3093));
    let v3094 = b.add_schema(3094, Schema::same_namespaced);
    b.add_fixer(goat_horn_id_fix::new(&v3094));
    let v3097 = b.add_schema(3097, Schema::same_namespaced);
    b.add_fixer(filtered_books_fix::new(&v3097));
    b.add_fixer(filtered_signs_fix::new(&v3097));
    b.add_fixer(variant_rename_fix::new(
        &v3097,
        "Rename british shorthair",
        &References::ENTITY,
        "minecraft:cat",
        &[("minecraft:british", "minecraft:british_shorthair")],
    ));
    b.add_fixer(criteria_rename_fix::new(
        &v3097,
        "Migrate cat variant advancement for british shorthair",
        "minecraft:husbandry/complete_catalogue",
        |s| {
            if s == "minecraft:british" {
                "minecraft:british_shorthair".to_string()
            } else {
                s.to_string()
            }
        },
    ));
    b.add_fixer(poi_type_remove_fix::new(
        &v3097,
        "Remove unpopulated villager PoI types",
        |t| t == "minecraft:unemployed" || t == "minecraft:nitwit",
    ));
    let v3108 = b.add_schema(3108, Schema::same_namespaced);
    b.add_fixer(blending_data_remove_from_nether_end_fix::new(&v3108));
    let v3201 = b.add_schema(3201, Schema::same_namespaced);
    b.add_fixer(options_programmer_art_fix::new(&v3201));
    let v3202 = b.add_schema(3202, v3202::new);
    b.add_fixer(add_new_choices::new(
        &v3202,
        "Added Hanging Sign",
        &References::BLOCK_ENTITY,
    ));
    let v3203 = b.add_schema(3203, v3203::new);
    b.add_fixer(add_new_choices::new(
        &v3203,
        "Added Camel",
        &References::ENTITY,
    ));
    let v3204 = b.add_schema(3204, v3204::new);
    b.add_fixer(add_new_choices::new(
        &v3204,
        "Added Chiseled Bookshelf",
        &References::BLOCK_ENTITY,
    ));
    let v3209 = b.add_schema(3209, Schema::same_namespaced);
    b.add_fixer(item_stack_spawn_egg_fix::new(
        &v3209,
        false,
        "minecraft:pig_spawn_egg",
    ));
    let v3214 = b.add_schema(3214, Schema::same_namespaced);
    b.add_fixer(options_ambient_occlusion_fix::new(&v3214));
}
