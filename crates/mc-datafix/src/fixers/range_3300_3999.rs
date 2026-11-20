//! `DataFixers.addFixers`, versions 3300..3999.

use crate::data_fixers::{create_renamer, create_renamer_pair};
use crate::fixer::DataFixerBuilder;
use crate::fixes::references::References;
use crate::fixes::{
    add_new_choices, area_effect_cloud_potion_fix, attribute_modifier_id_fix,
    attributes_rename_legacy, banner_entity_custom_name_to_override_component_fix,
    banner_pattern_format_fix, beehive_field_rename_fix, block_entity_rename_fix,
    block_entity_sign_double_sided_editable_text_fix, block_pos_format_and_renames_fix,
    chested_horses_inventory_zero_indexing_fix, chunk_delete_light_fix,
    decorated_pot_field_rename_fix, drop_invalid_sign_data_fix, effect_duration_fix,
    empty_item_in_hotbar_fix, empty_item_in_villager_trade_fix,
    entity_brushable_block_fields_rename_fix, feature_flag_remove_fix, fix_projectile_stored_item,
    fix_wolf_health, horse_body_armor_item_fix, item_rename_fix, item_stack_componentization_fix,
    item_stack_custom_name_to_override_component_fix, jukebox_ticks_since_song_started_fix,
    legacy_block_rename_fix, legacy_dragon_fight_fix, lodestone_compass_component_fix,
    map_banner_block_pos_format_fix, mob_effect_id_fix, namespaced_type_rename_fix,
    options_accessibility_onboard_fix, options_menu_blurriness_fix, particle_unflattening_fix,
    player_head_block_profile_fix, primed_tnt_block_state_fixer, projectile_stored_weapon_fix,
    random_sequence_settings_fix, remap_chunk_status_fix, remove_empty_item_in_brushable_block_fix,
    rename_enchantments_fix, scoreboard_display_slot_fix, tipped_arrow_potion_to_item_fix,
    trial_spawner_config_fix, write_and_read_fix,
};
use crate::schema::Schema;
use crate::schemas::{
    v3325, v3326, v3327, v3328, v3438, v3439, v3439_1, v3448, v3682, v3683, v3685, v3689, v3799,
    v3807, v3808, v3808_1, v3808_2, v3813, v3816, v3818, v3818_3, v3818_4, v3818_5, v3825, v3938,
};

const POTTERY_SHARDS: [&str; 20] = [
    "minecraft:angler_pottery_shard",
    "minecraft:archer_pottery_shard",
    "minecraft:arms_up_pottery_shard",
    "minecraft:blade_pottery_shard",
    "minecraft:brewer_pottery_shard",
    "minecraft:burn_pottery_shard",
    "minecraft:danger_pottery_shard",
    "minecraft:explorer_pottery_shard",
    "minecraft:friend_pottery_shard",
    "minecraft:heart_pottery_shard",
    "minecraft:heartbreak_pottery_shard",
    "minecraft:howl_pottery_shard",
    "minecraft:miner_pottery_shard",
    "minecraft:mourner_pottery_shard",
    "minecraft:plenty_pottery_shard",
    "minecraft:prize_pottery_shard",
    "minecraft:sheaf_pottery_shard",
    "minecraft:shelter_pottery_shard",
    "minecraft:skull_pottery_shard",
    "minecraft:snort_pottery_shard",
];

pub fn add(b: &mut DataFixerBuilder) {
    let v3319 = b.add_schema(3319, Schema::same_namespaced);
    b.add_fixer(options_accessibility_onboard_fix::new(&v3319));
    let v3322 = b.add_schema(3322, Schema::same_namespaced);
    b.add_fixer(effect_duration_fix::new(&v3322));
    let v3325 = b.add_schema(3325, v3325::new);
    b.add_fixer(add_new_choices::new(
        &v3325,
        "Added displays",
        &References::ENTITY,
    ));
    let v3326 = b.add_schema(3326, v3326::new);
    b.add_fixer(add_new_choices::new(
        &v3326,
        "Added Sniffer",
        &References::ENTITY,
    ));
    let v3327 = b.add_schema(3327, v3327::new);
    b.add_fixer(add_new_choices::new(
        &v3327,
        "Archaeology",
        &References::BLOCK_ENTITY,
    ));
    let v3328 = b.add_schema(3328, v3328::new);
    b.add_fixer(add_new_choices::new(
        &v3328,
        "Added interaction",
        &References::ENTITY,
    ));
    let v3438 = b.add_schema(3438, v3438::new);
    b.add_fixer(block_entity_rename_fix::create(
        &v3438,
        "Rename Suspicious Sand to Brushable Block",
        create_renamer_pair("minecraft:suspicious_sand", "minecraft:brushable_block"),
    ));
    b.add_fixer(entity_brushable_block_fields_rename_fix::new(&v3438));
    b.add_fixer(item_rename_fix::create(
        &v3438,
        "Pottery shard renaming",
        create_renamer(&[
            (
                "minecraft:pottery_shard_archer",
                "minecraft:archer_pottery_shard",
            ),
            (
                "minecraft:pottery_shard_prize",
                "minecraft:prize_pottery_shard",
            ),
            (
                "minecraft:pottery_shard_arms_up",
                "minecraft:arms_up_pottery_shard",
            ),
            (
                "minecraft:pottery_shard_skull",
                "minecraft:skull_pottery_shard",
            ),
        ]),
    ));
    b.add_fixer(add_new_choices::new(
        &v3438,
        "Added calibrated sculk sensor",
        &References::BLOCK_ENTITY,
    ));
    let v3439 = b.add_schema(3439, v3439::new);
    b.add_fixer(block_entity_sign_double_sided_editable_text_fix::new(
        &v3439,
        "Updated sign text format for Signs",
        "minecraft:sign",
    ));
    let v3439_1 = b.add_schema_sub(3439, 1, v3439_1::new);
    b.add_fixer(block_entity_sign_double_sided_editable_text_fix::new(
        &v3439_1,
        "Updated sign text format for Hanging Signs",
        "minecraft:hanging_sign",
    ));
    let v3440 = b.add_schema(3440, Schema::same_namespaced);
    b.add_fixer(namespaced_type_rename_fix::new(
        &v3440,
        "Replace experimental 1.20 overworld",
        &References::MULTI_NOISE_BIOME_SOURCE_PARAMETER_LIST,
        create_renamer_pair("minecraft:overworld_update_1_20", "minecraft:overworld"),
    ));
    b.add_fixer(feature_flag_remove_fix::new(
        &v3440,
        "Remove 1.20 feature toggle",
        &["minecraft:update_1_20"],
    ));
    let v3447 = b.add_schema(3447, Schema::same_namespaced);
    let sherd_renames: Vec<(&str, String)> = POTTERY_SHARDS
        .iter()
        .map(|key| (*key, key.replace("_pottery_shard", "_pottery_sherd")))
        .collect();
    let sherd_renames: Vec<(&str, &str)> = sherd_renames
        .iter()
        .map(|(from, to)| (*from, to.as_str()))
        .collect();
    b.add_fixer(item_rename_fix::create(
        &v3447,
        "Pottery shard item renaming to Pottery sherd",
        create_renamer(&sherd_renames),
    ));
    let v3448 = b.add_schema(3448, v3448::new);
    b.add_fixer(decorated_pot_field_rename_fix::new(&v3448));
    let v3450 = b.add_schema(3450, Schema::same_namespaced);
    b.add_fixer(remap_chunk_status_fix::new(
        &v3450,
        "Remove liquid_carvers and heightmap chunk statuses",
        create_renamer(&[
            ("minecraft:liquid_carvers", "minecraft:carvers"),
            ("minecraft:heightmaps", "minecraft:spawn"),
        ]),
    ));
    let v3451 = b.add_schema(3451, Schema::same_namespaced);
    b.add_fixer(chunk_delete_light_fix::new(&v3451));
    let v3459 = b.add_schema(3459, Schema::same_namespaced);
    b.add_fixer(legacy_dragon_fight_fix::new(&v3459));
    let v3564 = b.add_schema(3564, Schema::same_namespaced);
    b.add_fixer(drop_invalid_sign_data_fix::new(&v3564, "minecraft:sign"));
    let v3564_1 = b.add_schema_sub(3564, 1, Schema::same_namespaced);
    b.add_fixer(drop_invalid_sign_data_fix::new(
        &v3564_1,
        "minecraft:hanging_sign",
    ));
    let v3565 = b.add_schema(3565, Schema::same_namespaced);
    b.add_fixer(random_sequence_settings_fix::new(&v3565));
    let v3566 = b.add_schema(3566, Schema::same_namespaced);
    b.add_fixer(scoreboard_display_slot_fix::new(&v3566));
    let v3568 = b.add_schema(3568, Schema::same_namespaced);
    b.add_fixer(mob_effect_id_fix::new(&v3568));
    let v3682 = b.add_schema(3682, v3682::new);
    b.add_fixer(add_new_choices::new(
        &v3682,
        "Added Crafter",
        &References::BLOCK_ENTITY,
    ));
    let v3683 = b.add_schema(3683, v3683::new);
    b.add_fixer(primed_tnt_block_state_fixer::new(&v3683));
    let v3685 = b.add_schema(3685, v3685::new);
    b.add_fixer(fix_projectile_stored_item::new(&v3685));
    let v3689 = b.add_schema(3689, v3689::new);
    b.add_fixer(add_new_choices::new(
        &v3689,
        "Added Breeze",
        &References::ENTITY,
    ));
    b.add_fixer(add_new_choices::new(
        &v3689,
        "Added Trial Spawner",
        &References::BLOCK_ENTITY,
    ));
    let v3692 = b.add_schema(3692, Schema::same_namespaced);
    let grass_renamer = create_renamer(&[("minecraft:grass", "minecraft:short_grass")]);
    b.add_fixer(legacy_block_rename_fix::create(
        &v3692,
        "Rename grass block to short_grass",
        grass_renamer.clone(),
    ));
    b.add_fixer(item_rename_fix::create(
        &v3692,
        "Rename grass item to short_grass",
        grass_renamer,
    ));
    let v3799 = b.add_schema(3799, v3799::new);
    b.add_fixer(add_new_choices::new(
        &v3799,
        "Added Armadillo",
        &References::ENTITY,
    ));
    let v3800 = b.add_schema(3800, Schema::same_namespaced);
    b.add_fixer(item_rename_fix::create(
        &v3800,
        "Rename scute item to turtle_scute",
        create_renamer(&[("minecraft:scute", "minecraft:turtle_scute")]),
    ));
    let v3803 = b.add_schema(3803, Schema::same_namespaced);
    b.add_fixer(rename_enchantments_fix::new(
        &v3803,
        "Rename sweeping enchant to sweeping_edge",
        &[("minecraft:sweeping", "minecraft:sweeping_edge")],
    ));
    let v3807 = b.add_schema(3807, v3807::new);
    b.add_fixer(add_new_choices::new(
        &v3807,
        "Added Vault",
        &References::BLOCK_ENTITY,
    ));
    let v3807_1 = b.add_schema_sub(3807, 1, Schema::same_namespaced);
    b.add_fixer(map_banner_block_pos_format_fix::new(&v3807_1));
    let v3808 = b.add_schema(3808, v3808::new);
    b.add_fixer(horse_body_armor_item_fix::new(
        &v3808,
        "minecraft:horse",
        "ArmorItem",
        true,
    ));
    let v3808_1 = b.add_schema_sub(3808, 1, v3808_1::new);
    b.add_fixer(horse_body_armor_item_fix::new(
        &v3808_1,
        "minecraft:llama",
        "DecorItem",
        false,
    ));
    let v3808_2 = b.add_schema_sub(3808, 2, v3808_2::new);
    b.add_fixer(horse_body_armor_item_fix::new(
        &v3808_2,
        "minecraft:trader_llama",
        "DecorItem",
        false,
    ));
    let v3809 = b.add_schema(3809, Schema::same_namespaced);
    b.add_fixer(chested_horses_inventory_zero_indexing_fix::new(&v3809));
    let v3812 = b.add_schema(3812, Schema::same_namespaced);
    b.add_fixer(fix_wolf_health::new(&v3812));
    let v3813 = b.add_schema(3813, v3813::new);
    b.add_fixer(block_pos_format_and_renames_fix::new(&v3813));
    let v3814 = b.add_schema(3814, Schema::same_namespaced);
    b.add_fixer(attributes_rename_legacy::new(
        &v3814,
        "Rename jump strength attribute",
        create_renamer_pair(
            "minecraft:horse.jump_strength",
            "minecraft:generic.jump_strength",
        ),
    ));
    let v3816 = b.add_schema(3816, v3816::new);
    b.add_fixer(add_new_choices::new(
        &v3816,
        "Added Bogged",
        &References::ENTITY,
    ));
    let v3818 = b.add_schema(3818, v3818::new);
    b.add_fixer(beehive_field_rename_fix::new(&v3818));
    b.add_fixer(empty_item_in_hotbar_fix::new(&v3818));
    let v3818_1 = b.add_schema_sub(3818, 1, Schema::same_namespaced);
    b.add_fixer(banner_pattern_format_fix::new(&v3818_1));
    let v3818_2 = b.add_schema_sub(3818, 2, Schema::same_namespaced);
    b.add_fixer(tipped_arrow_potion_to_item_fix::new(&v3818_2));
    let v3818_3 = b.add_schema_sub(3818, 3, v3818_3::new);
    b.add_fixer(write_and_read_fix::new(
        &v3818_3,
        "Inject data component types",
        &References::DATA_COMPONENTS,
    ));
    let v3818_4 = b.add_schema_sub(3818, 4, v3818_4::new);
    b.add_fixer(particle_unflattening_fix::new(&v3818_4));
    let v3818_5 = b.add_schema_sub(3818, 5, v3818_5::new);
    b.add_fixer(item_stack_componentization_fix::new(&v3818_5));
    let v3818_6 = b.add_schema_sub(3818, 6, Schema::same_namespaced);
    b.add_fixer(area_effect_cloud_potion_fix::new(&v3818_6));
    let v3820 = b.add_schema(3820, Schema::same_namespaced);
    b.add_fixer(player_head_block_profile_fix::new(&v3820));
    b.add_fixer(lodestone_compass_component_fix::new(&v3820));
    let v3825 = b.add_schema(3825, v3825::new);
    b.add_fixer(item_stack_custom_name_to_override_component_fix::new(
        &v3825,
    ));
    b.add_fixer(banner_entity_custom_name_to_override_component_fix::new(
        &v3825,
    ));
    b.add_fixer(trial_spawner_config_fix::new(&v3825));
    b.add_fixer(add_new_choices::new(
        &v3825,
        "Added Ominous Item Spawner",
        &References::ENTITY,
    ));
    let v3828 = b.add_schema(3828, Schema::same_namespaced);
    b.add_fixer(empty_item_in_villager_trade_fix::new(&v3828));
    let v3833 = b.add_schema(3833, Schema::same_namespaced);
    b.add_fixer(remove_empty_item_in_brushable_block_fix::new(&v3833));
    let v3938 = b.add_schema(3938, v3938::new);
    b.add_fixer(projectile_stored_weapon_fix::new(&v3938));
    let v3939 = b.add_schema(3939, Schema::same_namespaced);
    b.add_fixer(feature_flag_remove_fix::new(
        &v3939,
        "Remove 1.21 feature toggle",
        &["minecraft:update_1_21"],
    ));
    let v3943 = b.add_schema(3943, Schema::same_namespaced);
    b.add_fixer(options_menu_blurriness_fix::new(&v3943));
    let v3945 = b.add_schema(3945, Schema::same_namespaced);
    b.add_fixer(attribute_modifier_id_fix::new(&v3945));
    b.add_fixer(jukebox_ticks_since_song_started_fix::new(&v3945));
}
