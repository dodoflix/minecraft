//! `DataFixers.addFixers`, versions 4000..4649.

use crate::data_fixers::create_renamer;
use crate::fixer::DataFixerBuilder;
use crate::fixes::references::References;
use crate::fixes::{
    add_field_fix, add_new_choices, advancements_rename_fix, area_effect_cloud_duration_scale_fix,
    attribute_id_prefix_fix, block_entity_furnace_burn_time_fix, boat_split_fix,
    chunk_delete_light_fix, chunk_ticket_unpack_pos_fix, container_block_entity_lock_predicate_fix,
    copper_golem_weather_state_fix, custom_model_data_expand_fix, drop_chances_format_fix,
    entity_attribute_base_value_fix, entity_fall_distance_float_to_double_fix,
    entity_fields_rename_fix, entity_salmon_size_fix, entity_spawner_item_variant_component_fix,
    equipment_format_fix, equippable_asset_rename_fix, feature_flag_remove_fix,
    fire_resistant_to_damage_resistant_component_fix, food_to_consumable_fix,
    forced_chunk_to_ticket_fix, inline_block_pos_format_fix, invalid_block_entity_lock_fix,
    invalid_lock_component_fix, item_rename_fix, legacy_block_property_rename_and_fix,
    legacy_block_rename_fix, legacy_hover_event_fix, legacy_world_border_fix,
    lock_component_predicate_fix, named_entity_convert_unchecked_fix, ominous_banner_rarity_fix,
    player_equipment_fix, player_respawn_data_fix, raid_renames_data_fix,
    saddle_equipment_slot_fix, text_component_hover_and_click_event_fix,
    text_component_stringified_flags_fix, thrown_potion_split_fix, tooltip_display_component_fix,
    trial_spawner_config_in_registry_fix, trident_animation_fix, unflatten_text_component_fix,
    world_spawn_data_fix,
};
use crate::schema::Schema;
use crate::schemas::{
    v4059, v4067, v4070, v4071, v4290, v4292, v4300, v4301, v4302, v4306, v4307, v4312, v4420,
    v4421, v4531, v4532, v4533, v4543, v4648,
};

pub fn add(b: &mut DataFixerBuilder) {
    let v4054 = b.add_schema(4054, Schema::same_namespaced);
    b.add_fixer(ominous_banner_rarity_fix::new(&v4054));
    let v4055 = b.add_schema(4055, Schema::same_namespaced);
    b.add_fixer(attribute_id_prefix_fix::new(&v4055));
    let v4059 = b.add_schema(4059, v4059::new);
    b.add_fixer(food_to_consumable_fix::new(&v4059));
    let v4061 = b.add_schema(4061, Schema::same_namespaced);
    b.add_fixer(trial_spawner_config_in_registry_fix::new(&v4061));
    let v4064 = b.add_schema(4064, Schema::same_namespaced);
    b.add_fixer(fire_resistant_to_damage_resistant_component_fix::new(
        &v4064,
    ));
    let v4067 = b.add_schema(4067, v4067::new);
    b.add_fixer(boat_split_fix::new(&v4067));
    b.add_fixer(feature_flag_remove_fix::new(
        &v4067,
        "Remove Bundle experimental feature flag",
        &["minecraft:bundle"],
    ));
    let v4068 = b.add_schema(4068, Schema::same_namespaced);
    b.add_fixer(lock_component_predicate_fix::new(&v4068));
    b.add_fixer(container_block_entity_lock_predicate_fix::new(&v4068));
    let v4070 = b.add_schema(4070, v4070::new);
    b.add_fixer(add_new_choices::new(
        &v4070,
        "Added Pale Oak Boat and Pale Oak Chest Boat",
        &References::ENTITY,
    ));
    let v4071 = b.add_schema(4071, v4071::new);
    b.add_fixer(add_new_choices::new(
        &v4071,
        "Added Creaking",
        &References::ENTITY,
    ));
    b.add_fixer(add_new_choices::new(
        &v4071,
        "Added Creaking Heart",
        &References::BLOCK_ENTITY,
    ));
    let v4081 = b.add_schema(4081, Schema::same_namespaced);
    b.add_fixer(entity_salmon_size_fix::new(&v4081));
    let v4173 = b.add_schema(4173, Schema::same_namespaced);
    b.add_fixer(entity_fields_rename_fix::new(
        &v4173,
        "Rename TNT Minecart fuse",
        "minecraft:tnt_minecart",
        &[("TNTFuse", "fuse")],
    ));
    let v4175 = b.add_schema(4175, Schema::same_namespaced);
    b.add_fixer(equippable_asset_rename_fix::new(&v4175));
    b.add_fixer(custom_model_data_expand_fix::new(&v4175));
    let v4176 = b.add_schema(4176, Schema::same_namespaced);
    b.add_fixer(invalid_block_entity_lock_fix::new(&v4176));
    b.add_fixer(invalid_lock_component_fix::new(&v4176));
    let v4180 = b.add_schema(4180, Schema::same_namespaced);
    b.add_fixer(feature_flag_remove_fix::new(
        &v4180,
        "Remove Winter Drop toggle",
        &["minecraft:winter_drop"],
    ));
    let v4181 = b.add_schema(4181, Schema::same_namespaced);
    b.add_fixer(block_entity_furnace_burn_time_fix::new(
        &v4181,
        "minecraft:furnace",
    ));
    b.add_fixer(block_entity_furnace_burn_time_fix::new(
        &v4181,
        "minecraft:smoker",
    ));
    b.add_fixer(block_entity_furnace_burn_time_fix::new(
        &v4181,
        "minecraft:blast_furnace",
    ));
    let v4187 = b.add_schema(4187, Schema::same_namespaced);
    b.add_fixer(entity_attribute_base_value_fix::new(
        &v4187,
        "Villager follow range fix undo",
        "minecraft:villager",
        "minecraft:follow_range",
        |value| if value == 48.0 { 16.0 } else { value },
    ));
    b.add_fixer(entity_attribute_base_value_fix::new(
        &v4187,
        "Bee follow range fix",
        "minecraft:bee",
        "minecraft:follow_range",
        |value| if value == 48.0 { 16.0 } else { value },
    ));
    b.add_fixer(entity_attribute_base_value_fix::new(
        &v4187,
        "Allay follow range fix",
        "minecraft:allay",
        "minecraft:follow_range",
        |value| if value == 48.0 { 16.0 } else { value },
    ));
    b.add_fixer(entity_attribute_base_value_fix::new(
        &v4187,
        "Llama follow range fix",
        "minecraft:llama",
        "minecraft:follow_range",
        |value| if value == 40.0 { 16.0 } else { value },
    ));
    b.add_fixer(entity_attribute_base_value_fix::new(
        &v4187,
        "Piglin Brute follow range fix",
        "minecraft:piglin_brute",
        "minecraft:follow_range",
        |value| if value == 16.0 { 12.0 } else { value },
    ));
    b.add_fixer(entity_attribute_base_value_fix::new(
        &v4187,
        "Warden follow range fix",
        "minecraft:warden",
        "minecraft:follow_range",
        |value| if value == 16.0 { 24.0 } else { value },
    ));
    let v4290 = b.add_schema(4290, v4290::new);
    b.add_fixer(unflatten_text_component_fix::new(&v4290));
    let v4291 = b.add_schema(4291, Schema::same_namespaced);
    b.add_fixer(legacy_hover_event_fix::new(&v4291));
    b.add_fixer(text_component_stringified_flags_fix::new(&v4291));
    let v4292 = b.add_schema(4292, v4292::new);
    b.add_fixer(text_component_hover_and_click_event_fix::new(&v4292));
    let v4293 = b.add_schema(4293, Schema::same_namespaced);
    b.add_fixer(drop_chances_format_fix::new(&v4293));
    let v4294 = b.add_schema(4294, Schema::same_namespaced);
    b.add_fixer(legacy_block_property_rename_and_fix::new(
        &v4294,
        "CreakingHeartBlockStateFix",
        "minecraft:creaking_heart",
        "active",
        "creaking_heart_state",
        |value| if value == "true" { "awake" } else { "uprooted" }.to_string(),
    ));
    let v4296 = b.add_schema(4296, Schema::same_namespaced);
    b.add_fixer(area_effect_cloud_duration_scale_fix::new(&v4296));
    let v4297 = b.add_schema(4297, Schema::same_namespaced);
    b.add_fixer(forced_chunk_to_ticket_fix::new(&v4297));
    let v4299 = b.add_schema(4299, Schema::same_namespaced);
    b.add_fixer(entity_spawner_item_variant_component_fix::new(&v4299));
    let v4300 = b.add_schema(4300, v4300::new);
    b.add_fixer(saddle_equipment_slot_fix::new(&v4300));
    let v4301 = b.add_schema(4301, v4301::new);
    b.add_fixer(equipment_format_fix::new(&v4301));
    let v4302 = b.add_schema(4302, v4302::new);
    b.add_fixer(add_new_choices::new(
        &v4302,
        "Added Test and Test Instance Block Entities",
        &References::BLOCK_ENTITY,
    ));
    let v4303 = b.add_schema(4303, Schema::same_namespaced);
    b.add_fixer(entity_fall_distance_float_to_double_fix::new(
        &v4303,
        &References::ENTITY,
    ));
    b.add_fixer(entity_fall_distance_float_to_double_fix::new(
        &v4303,
        &References::PLAYER,
    ));
    let v4305 = b.add_schema(4305, Schema::same_namespaced);
    b.add_fixer(legacy_block_property_rename_and_fix::new(
        &v4305,
        "rename test block mode",
        "minecraft:test_block",
        "test_block_mode",
        "mode",
        |a| a.to_string(),
    ));
    let v4306 = b.add_schema(4306, v4306::new);
    b.add_fixer(thrown_potion_split_fix::new(&v4306));
    let v4307 = b.add_schema(4307, v4307::new);
    b.add_fixer(tooltip_display_component_fix::new(&v4307));
    let v4309 = b.add_schema(4309, Schema::same_namespaced);
    b.add_fixer(raid_renames_data_fix::new(&v4309));
    b.add_fixer(chunk_ticket_unpack_pos_fix::new(&v4309));
    let v4311 = b.add_schema(4311, Schema::same_namespaced);
    b.add_fixer(advancements_rename_fix::new(
        "AdvancementsRenameFix",
        &v4311,
        false,
        "Use lodestone category change",
        create_renamer(&[(
            "minecraft:nether/use_lodestone",
            "minecraft:adventure/use_lodestone",
        )]),
    ));
    let v4312 = b.add_schema(4312, v4312::new);
    b.add_fixer(player_equipment_fix::new(&v4312));
    let v4314 = b.add_schema(4314, Schema::same_namespaced);
    b.add_fixer(inline_block_pos_format_fix::new(&v4314));
    let v4420 = b.add_schema(4420, v4420::new);
    b.add_fixer(named_entity_convert_unchecked_fix::new(
        &v4420,
        "AreaEffectCloudCustomParticleFix",
        &References::ENTITY,
        "minecraft:area_effect_cloud",
    ));
    let v4421 = b.add_schema(4421, v4421::new);
    b.add_fixer(add_new_choices::new(
        &v4421,
        "Added Happy Ghast",
        &References::ENTITY,
    ));
    let v4424 = b.add_schema(4424, Schema::same_namespaced);
    b.add_fixer(feature_flag_remove_fix::new(
        &v4424,
        "Remove Locator Bar experimental feature flag",
        &["minecraft:locator_bar"],
    ));
    b.add_fixer(add_field_fix::new(
        &v4424,
        &References::PLAYER,
        "style",
        |field| field.create_string("minecraft:default"),
        &["locator_bar_icon"],
    ));
    b.add_fixer(add_field_fix::new(
        &v4424,
        &References::ENTITY,
        "style",
        |field| field.create_string("minecraft:default"),
        &["locator_bar_icon"],
    ));
    let v4531 = b.add_schema(4531, v4531::new);
    b.add_fixer(add_new_choices::new(
        &v4531,
        "Added Copper Golem",
        &References::ENTITY,
    ));
    let v4532 = b.add_schema(4532, v4532::new);
    b.add_fixer(add_new_choices::new(
        &v4532,
        "Added Copper Golem Statue Block Entity",
        &References::BLOCK_ENTITY,
    ));
    let v4533 = b.add_schema(4533, v4533::new);
    b.add_fixer(add_new_choices::new(
        &v4533,
        "Added Shelf",
        &References::BLOCK_ENTITY,
    ));
    let v4535 = b.add_schema(4535, Schema::same_namespaced);
    b.add_fixer(copper_golem_weather_state_fix::new(&v4535));
    let v4537 = b.add_schema(4537, Schema::same_namespaced);
    b.add_fixer(chunk_delete_light_fix::new(&v4537));
    let v4541 = b.add_schema(4541, Schema::same_namespaced);
    b.add_fixer(legacy_block_rename_fix::create(
        &v4541,
        "Rename chain to iron_chain",
        create_renamer(&[("minecraft:chain", "minecraft:iron_chain")]),
    ));
    b.add_fixer(item_rename_fix::create(
        &v4541,
        "Rename chain to iron_chain",
        create_renamer(&[("minecraft:chain", "minecraft:iron_chain")]),
    ));
    let v4543 = b.add_schema(4543, v4543::new);
    b.add_fixer(add_new_choices::new(
        &v4543,
        "Added Mannequin",
        &References::ENTITY,
    ));
    let v4544 = b.add_schema(4544, Schema::same_namespaced);
    b.add_fixer(legacy_world_border_fix::new(&v4544));
    let v4548 = b.add_schema(4548, Schema::same_namespaced);
    b.add_fixer(world_spawn_data_fix::new(&v4548));
    b.add_fixer(player_respawn_data_fix::new(&v4548));
    let v4648 = b.add_schema(4648, v4648::new);
    b.add_fixer(add_new_choices::new(
        &v4648,
        "Added Nautilus and Zombie Nautilus",
        &References::ENTITY,
    ));
    let v4649 = b.add_schema(4649, Schema::same_namespaced);
    b.add_fixer(trident_animation_fix::new(&v4649));
}
