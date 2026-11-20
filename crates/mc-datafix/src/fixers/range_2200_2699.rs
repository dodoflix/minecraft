//! `DataFixers.addFixers`, versions 2200..2699.

use crate::data_fixers::{create_renamer, create_renamer_no_namespace, create_renamer_pair};
use crate::fixer::DataFixerBuilder;
use crate::fixes::references::References;
use crate::fixes::{
    add_flag_if_not_present_fix, add_new_choices, advancements_rename_fix,
    attributes_rename_legacy, biome_fix, bit_storage_align_fix, block_entity_uuid_fix,
    cauldron_rename_fix, chunk_biome_fix, entity_projectile_owner_fix, entity_shulker_rotation_fix,
    entity_uuid_fix, entity_zombified_piglin_rename_fix, force_poi_rebuild, furnace_recipe_fix,
    gossip_uuid_fix, item_rename_fix, item_stack_uuid_fix, jigsaw_properties_fix,
    legacy_block_rename_fix, legacy_dimension_id_fix, legacy_jigsaw_rotation_fix,
    level_legacy_world_gen_settings_fix, level_uuid_fix, memory_expiry_data_fix,
    missing_dimension_fix, named_entity_convert_unchecked_fix, namespaced_type_rename_fix,
    options_fancy_graphics_to_graphics_mode_fix, options_rename_field_fix, player_uuid_fix,
    poi_type_rename_fix, redstone_wire_connections_fix, saved_data_uuid_fix, strider_gravity_fix,
    structure_reference_count_fix, villager_follow_range_fix, wall_property_fix,
    world_gen_settings_fix, write_and_read_fix,
};
use crate::schema::Schema;
use crate::schemas::{
    v2501, v2502, v2505, v2509, v2511_1, v2519, v2522, v2551, v2568, v2571, v2684, v2686, v2688,
};

const COPPER_BLOCKS_TO_RENAME: [(&str, &str); 20] = [
    (
        "minecraft:weathered_copper_block",
        "minecraft:oxidized_copper_block",
    ),
    (
        "minecraft:semi_weathered_copper_block",
        "minecraft:weathered_copper_block",
    ),
    (
        "minecraft:lightly_weathered_copper_block",
        "minecraft:exposed_copper_block",
    ),
    (
        "minecraft:weathered_cut_copper",
        "minecraft:oxidized_cut_copper",
    ),
    (
        "minecraft:semi_weathered_cut_copper",
        "minecraft:weathered_cut_copper",
    ),
    (
        "minecraft:lightly_weathered_cut_copper",
        "minecraft:exposed_cut_copper",
    ),
    (
        "minecraft:weathered_cut_copper_stairs",
        "minecraft:oxidized_cut_copper_stairs",
    ),
    (
        "minecraft:semi_weathered_cut_copper_stairs",
        "minecraft:weathered_cut_copper_stairs",
    ),
    (
        "minecraft:lightly_weathered_cut_copper_stairs",
        "minecraft:exposed_cut_copper_stairs",
    ),
    (
        "minecraft:weathered_cut_copper_slab",
        "minecraft:oxidized_cut_copper_slab",
    ),
    (
        "minecraft:semi_weathered_cut_copper_slab",
        "minecraft:weathered_cut_copper_slab",
    ),
    (
        "minecraft:lightly_weathered_cut_copper_slab",
        "minecraft:exposed_cut_copper_slab",
    ),
    (
        "minecraft:waxed_semi_weathered_copper",
        "minecraft:waxed_weathered_copper",
    ),
    (
        "minecraft:waxed_lightly_weathered_copper",
        "minecraft:waxed_exposed_copper",
    ),
    (
        "minecraft:waxed_semi_weathered_cut_copper",
        "minecraft:waxed_weathered_cut_copper",
    ),
    (
        "minecraft:waxed_lightly_weathered_cut_copper",
        "minecraft:waxed_exposed_cut_copper",
    ),
    (
        "minecraft:waxed_semi_weathered_cut_copper_stairs",
        "minecraft:waxed_weathered_cut_copper_stairs",
    ),
    (
        "minecraft:waxed_lightly_weathered_cut_copper_stairs",
        "minecraft:waxed_exposed_cut_copper_stairs",
    ),
    (
        "minecraft:waxed_semi_weathered_cut_copper_slab",
        "minecraft:waxed_weathered_cut_copper_slab",
    ),
    (
        "minecraft:waxed_lightly_weathered_cut_copper_slab",
        "minecraft:waxed_exposed_cut_copper_slab",
    ),
];

const WAXED_COPPER_RENAME: [(&str, &str); 4] = [
    ("minecraft:waxed_copper", "minecraft:waxed_copper_block"),
    (
        "minecraft:oxidized_copper_block",
        "minecraft:oxidized_copper",
    ),
    (
        "minecraft:weathered_copper_block",
        "minecraft:weathered_copper",
    ),
    ("minecraft:exposed_copper_block", "minecraft:exposed_copper"),
];

const GRIMSTONE_BLOCKS_TO_RENAME: [(&str, &str); 17] = [
    ("minecraft:grimstone", "minecraft:deepslate"),
    (
        "minecraft:grimstone_slab",
        "minecraft:cobbled_deepslate_slab",
    ),
    (
        "minecraft:grimstone_stairs",
        "minecraft:cobbled_deepslate_stairs",
    ),
    (
        "minecraft:grimstone_wall",
        "minecraft:cobbled_deepslate_wall",
    ),
    (
        "minecraft:polished_grimstone",
        "minecraft:polished_deepslate",
    ),
    (
        "minecraft:polished_grimstone_slab",
        "minecraft:polished_deepslate_slab",
    ),
    (
        "minecraft:polished_grimstone_stairs",
        "minecraft:polished_deepslate_stairs",
    ),
    (
        "minecraft:polished_grimstone_wall",
        "minecraft:polished_deepslate_wall",
    ),
    ("minecraft:grimstone_tiles", "minecraft:deepslate_tiles"),
    (
        "minecraft:grimstone_tile_slab",
        "minecraft:deepslate_tile_slab",
    ),
    (
        "minecraft:grimstone_tile_stairs",
        "minecraft:deepslate_tile_stairs",
    ),
    (
        "minecraft:grimstone_tile_wall",
        "minecraft:deepslate_tile_wall",
    ),
    ("minecraft:grimstone_bricks", "minecraft:deepslate_bricks"),
    (
        "minecraft:grimstone_brick_slab",
        "minecraft:deepslate_brick_slab",
    ),
    (
        "minecraft:grimstone_brick_stairs",
        "minecraft:deepslate_brick_stairs",
    ),
    (
        "minecraft:grimstone_brick_wall",
        "minecraft:deepslate_brick_wall",
    ),
    (
        "minecraft:chiseled_grimstone",
        "minecraft:chiseled_deepslate",
    ),
];

const ATTRIBUTE_RENAMES: [(&str, &str); 18] = [
    ("generic.maxHealth", "minecraft:generic.max_health"),
    ("Max Health", "minecraft:generic.max_health"),
    (
        "zombie.spawnReinforcements",
        "minecraft:zombie.spawn_reinforcements",
    ),
    (
        "Spawn Reinforcements Chance",
        "minecraft:zombie.spawn_reinforcements",
    ),
    ("horse.jumpStrength", "minecraft:horse.jump_strength"),
    ("Jump Strength", "minecraft:horse.jump_strength"),
    ("generic.followRange", "minecraft:generic.follow_range"),
    ("Follow Range", "minecraft:generic.follow_range"),
    (
        "generic.knockbackResistance",
        "minecraft:generic.knockback_resistance",
    ),
    (
        "Knockback Resistance",
        "minecraft:generic.knockback_resistance",
    ),
    ("generic.movementSpeed", "minecraft:generic.movement_speed"),
    ("Movement Speed", "minecraft:generic.movement_speed"),
    ("generic.flyingSpeed", "minecraft:generic.flying_speed"),
    ("Flying Speed", "minecraft:generic.flying_speed"),
    ("generic.attackDamage", "minecraft:generic.attack_damage"),
    (
        "generic.attackKnockback",
        "minecraft:generic.attack_knockback",
    ),
    ("generic.attackSpeed", "minecraft:generic.attack_speed"),
    (
        "generic.armorToughness",
        "minecraft:generic.armor_toughness",
    ),
];

pub fn add(b: &mut DataFixerBuilder) {
    let v2202 = b.add_schema(2202, Schema::same_namespaced);
    b.add_fixer(chunk_biome_fix::new(&v2202, false));
    let v2209 = b.add_schema(2209, Schema::same_namespaced);
    let beehive_renamer = create_renamer_pair("minecraft:bee_hive", "minecraft:beehive");
    b.add_fixer(item_rename_fix::create(
        &v2209,
        "Rename bee_hive item to beehive",
        beehive_renamer.clone(),
    ));
    b.add_fixer(poi_type_rename_fix::new(
        &v2209,
        "Rename bee_hive poi to beehive",
        beehive_renamer.clone(),
    ));
    b.add_fixer(legacy_block_rename_fix::create(
        &v2209,
        "Rename bee_hive block to beehive",
        beehive_renamer,
    ));
    let v2211 = b.add_schema(2211, Schema::same_namespaced);
    b.add_fixer(structure_reference_count_fix::new(&v2211, false));
    let v2218 = b.add_schema(2218, Schema::same_namespaced);
    b.add_fixer(force_poi_rebuild::new(&v2218, false));
    let v2501 = b.add_schema(2501, v2501::new);
    b.add_fixer(furnace_recipe_fix::new(&v2501, true));
    let v2502 = b.add_schema(2502, v2502::new);
    b.add_fixer(add_new_choices::new(
        &v2502,
        "Added Hoglin",
        &References::ENTITY,
    ));
    let v2503 = b.add_schema(2503, Schema::same_namespaced);
    b.add_fixer(wall_property_fix::new(&v2503, false));
    b.add_fixer(advancements_rename_fix::new(
        "AdvancementsRenameFix",
        &v2503,
        false,
        "Composter category change",
        create_renamer_pair(
            "minecraft:recipes/misc/composter",
            "minecraft:recipes/decorations/composter",
        ),
    ));
    let v2505 = b.add_schema(2505, v2505::new);
    b.add_fixer(add_new_choices::new(
        &v2505,
        "Added Piglin",
        &References::ENTITY,
    ));
    b.add_fixer(memory_expiry_data_fix::new(&v2505, "minecraft:villager"));
    let v2508 = b.add_schema(2508, Schema::same_namespaced);
    let fungi = [
        ("minecraft:warped_fungi", "minecraft:warped_fungus"),
        ("minecraft:crimson_fungi", "minecraft:crimson_fungus"),
    ];
    b.add_fixer(item_rename_fix::create(
        &v2508,
        "Renamed fungi items to fungus",
        create_renamer(&fungi),
    ));
    b.add_fixer(legacy_block_rename_fix::create(
        &v2508,
        "Renamed fungi blocks to fungus",
        create_renamer(&fungi),
    ));
    let v2509 = b.add_schema(2509, v2509::new);
    b.add_fixer(entity_zombified_piglin_rename_fix::new(&v2509));
    b.add_fixer(item_rename_fix::create(
        &v2509,
        "Rename zombie pigman egg item",
        create_renamer(entity_zombified_piglin_rename_fix::renamed_ids()),
    ));
    let v2511 = b.add_schema(2511, Schema::same_namespaced);
    b.add_fixer(entity_projectile_owner_fix::new(&v2511));
    let v2511_1 = b.add_schema_sub(2511, 1, v2511_1::new);
    b.add_fixer(named_entity_convert_unchecked_fix::new(
        &v2511_1,
        "SplashPotionItemFieldRenameFix",
        &References::ENTITY,
        "minecraft:potion",
    ));
    let v2514 = b.add_schema(2514, Schema::same_namespaced);
    b.add_fixer(entity_uuid_fix::new(&v2514));
    b.add_fixer(block_entity_uuid_fix::new(&v2514));
    b.add_fixer(player_uuid_fix::new(&v2514));
    b.add_fixer(level_uuid_fix::new(&v2514));
    b.add_fixer(saved_data_uuid_fix::new(&v2514));
    b.add_fixer(item_stack_uuid_fix::new(&v2514));
    let v2516 = b.add_schema(2516, Schema::same_namespaced);
    b.add_fixer(gossip_uuid_fix::new(&v2516, "minecraft:villager"));
    b.add_fixer(gossip_uuid_fix::new(&v2516, "minecraft:zombie_villager"));
    let v2518 = b.add_schema(2518, Schema::same_namespaced);
    b.add_fixer(jigsaw_properties_fix::new(&v2518, false));
    b.add_fixer(legacy_jigsaw_rotation_fix::new(&v2518));
    let v2519 = b.add_schema(2519, v2519::new);
    b.add_fixer(add_new_choices::new(
        &v2519,
        "Added Strider",
        &References::ENTITY,
    ));
    let v2522 = b.add_schema(2522, v2522::new);
    b.add_fixer(add_new_choices::new(
        &v2522,
        "Added Zoglin",
        &References::ENTITY,
    ));
    let v2523 = b.add_schema(2523, Schema::same_namespaced);
    b.add_fixer(attributes_rename_legacy::new(
        &v2523,
        "Attribute renames",
        create_renamer_no_namespace(&ATTRIBUTE_RENAMES),
    ));
    let v2527 = b.add_schema(2527, Schema::same_namespaced);
    b.add_fixer(bit_storage_align_fix::new(&v2527));
    let v2528 = b.add_schema(2528, Schema::same_namespaced);
    b.add_fixer(item_rename_fix::create(
        &v2528,
        "Rename soul fire torch and soul fire lantern",
        create_renamer(&[
            ("minecraft:soul_fire_torch", "minecraft:soul_torch"),
            ("minecraft:soul_fire_lantern", "minecraft:soul_lantern"),
        ]),
    ));
    b.add_fixer(legacy_block_rename_fix::create(
        &v2528,
        "Rename soul fire torch and soul fire lantern",
        create_renamer(&[
            ("minecraft:soul_fire_torch", "minecraft:soul_torch"),
            (
                "minecraft:soul_fire_wall_torch",
                "minecraft:soul_wall_torch",
            ),
            ("minecraft:soul_fire_lantern", "minecraft:soul_lantern"),
        ]),
    ));
    let v2529 = b.add_schema(2529, Schema::same_namespaced);
    b.add_fixer(strider_gravity_fix::new(&v2529, false));
    let v2531 = b.add_schema(2531, Schema::same_namespaced);
    b.add_fixer(redstone_wire_connections_fix::new(&v2531));
    let v2533 = b.add_schema(2533, Schema::same_namespaced);
    b.add_fixer(villager_follow_range_fix::new(&v2533));
    let v2535 = b.add_schema(2535, Schema::same_namespaced);
    b.add_fixer(entity_shulker_rotation_fix::new(&v2535));
    let v2537 = b.add_schema(2537, Schema::same_namespaced);
    b.add_fixer(legacy_dimension_id_fix::new(&v2537));
    let v2538 = b.add_schema(2538, Schema::same_namespaced);
    b.add_fixer(level_legacy_world_gen_settings_fix::new(&v2538));
    let v2550 = b.add_schema(2550, Schema::same_namespaced);
    b.add_fixer(world_gen_settings_fix::new(&v2550));
    let v2551 = b.add_schema(2551, v2551::new);
    b.add_fixer(write_and_read_fix::new(
        &v2551,
        "add types to WorldGenData",
        &References::WORLD_GEN_SETTINGS,
    ));
    let v2552 = b.add_schema(2552, Schema::same_namespaced);
    b.add_fixer(namespaced_type_rename_fix::new(
        &v2552,
        "Nether biome rename",
        &References::BIOME,
        create_renamer_pair("minecraft:nether", "minecraft:nether_wastes"),
    ));
    let v2553 = b.add_schema(2553, Schema::same_namespaced);
    b.add_fixer(namespaced_type_rename_fix::new(
        &v2553,
        "Biomes fix",
        &References::BIOME,
        create_renamer(biome_fix::biomes()),
    ));
    let v2556 = b.add_schema(2556, Schema::same_namespaced);
    b.add_fixer(options_fancy_graphics_to_graphics_mode_fix::new(&v2556));
    let v2558 = b.add_schema(2558, Schema::same_namespaced);
    b.add_fixer(missing_dimension_fix::new(&v2558, false));
    b.add_fixer(options_rename_field_fix::new(
        &v2558,
        false,
        "Rename swapHands setting",
        "key_key.swapHands",
        "key_key.swapOffhand",
    ));
    let v2568 = b.add_schema(2568, v2568::new);
    b.add_fixer(add_new_choices::new(
        &v2568,
        "Added Piglin Brute",
        &References::ENTITY,
    ));
    let v2571 = b.add_schema(2571, v2571::new);
    b.add_fixer(add_new_choices::new(
        &v2571,
        "Added Goat",
        &References::ENTITY,
    ));
    let v2679 = b.add_schema(2679, Schema::same_namespaced);
    b.add_fixer(cauldron_rename_fix::new(&v2679, false));
    let v2680 = b.add_schema(2680, Schema::same_namespaced);
    b.add_fixer(item_rename_fix::create(
        &v2680,
        "Renamed grass path item to dirt path",
        create_renamer_pair("minecraft:grass_path", "minecraft:dirt_path"),
    ));
    b.add_fixer(legacy_block_rename_fix::create(
        &v2680,
        "Renamed grass path block to dirt path",
        create_renamer_pair("minecraft:grass_path", "minecraft:dirt_path"),
    ));
    let v2684 = b.add_schema(2684, v2684::new);
    b.add_fixer(add_new_choices::new(
        &v2684,
        "Added Sculk Sensor",
        &References::BLOCK_ENTITY,
    ));
    let v2686 = b.add_schema(2686, v2686::new);
    b.add_fixer(add_new_choices::new(
        &v2686,
        "Added Axolotl",
        &References::ENTITY,
    ));
    let v2688 = b.add_schema(2688, v2688::new);
    b.add_fixer(add_new_choices::new(
        &v2688,
        "Added Glow Squid",
        &References::ENTITY,
    ));
    b.add_fixer(add_new_choices::new(
        &v2688,
        "Added Glow Item Frame",
        &References::ENTITY,
    ));
    let v2690 = b.add_schema(2690, Schema::same_namespaced);
    b.add_fixer(item_rename_fix::create(
        &v2690,
        "Renamed copper block items to new oxidized terms",
        create_renamer(&COPPER_BLOCKS_TO_RENAME),
    ));
    b.add_fixer(legacy_block_rename_fix::create(
        &v2690,
        "Renamed copper blocks to new oxidized terms",
        create_renamer(&COPPER_BLOCKS_TO_RENAME),
    ));
    let v2691 = b.add_schema(2691, Schema::same_namespaced);
    b.add_fixer(item_rename_fix::create(
        &v2691,
        "Rename copper item suffixes",
        create_renamer(&WAXED_COPPER_RENAME),
    ));
    b.add_fixer(legacy_block_rename_fix::create(
        &v2691,
        "Rename copper blocks suffixes",
        create_renamer(&WAXED_COPPER_RENAME),
    ));
    let v2693 = b.add_schema(2693, Schema::same_namespaced);
    b.add_fixer(add_flag_if_not_present_fix::new(
        &v2693,
        &References::WORLD_GEN_SETTINGS,
        "has_increased_height_already",
        false,
    ));
    let v2696 = b.add_schema(2696, Schema::same_namespaced);
    b.add_fixer(item_rename_fix::create(
        &v2696,
        "Renamed grimstone block items to deepslate",
        create_renamer(&GRIMSTONE_BLOCKS_TO_RENAME),
    ));
    b.add_fixer(legacy_block_rename_fix::create(
        &v2696,
        "Renamed grimstone blocks to deepslate",
        create_renamer(&GRIMSTONE_BLOCKS_TO_RENAME),
    ));
}
