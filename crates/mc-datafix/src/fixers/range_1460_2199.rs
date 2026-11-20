//! `DataFixers.addFixers`, versions 1460..2199 (1.13 to 1.15). The `LegacyStructureFileFix` registered between 1492
//! and 1494 is a file fix (`file_fixes`).

use crate::data_fixers::{create_renamer, create_renamer_pair};
use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::fixer::DataFixerBuilder;
use crate::fixes::references::References;
use crate::fixes::{
    add_new_choices, advancements_fix, advancements_rename_fix,
    block_entity_custom_name_to_component_fix, block_entity_keep_packed, cat_type_fix,
    chunk_light_remove_fix, chunk_status_fix, chunk_status_fix2,
    chunk_structures_template_rename_fix, chunk_to_protochunk_fix, colorless_shulker_entity_fix,
    dye_item_rename_fix, entity_cat_split_fix, entity_cod_salmon_fix, entity_painting_motive_fix,
    entity_pufferfish_rename_fix, entity_ravager_rename_fix, entity_the_renamening_fix,
    heightmap_renaming_fix, igloo_metadata_removal_fix, item_lore_fix, item_rename_fix,
    item_stack_enchantment_names_fix, leaves_fix, legacy_block_rename_fix,
    level_data_generator_options_fix, map_id_fix, named_entity_write_read_fix,
    namespaced_type_rename_fix, new_village_fix, objective_render_type_fix,
    ominous_banner_block_entity_rename_fix, ominous_banner_rename_fix,
    options_add_text_background_fix, recipes_fix, recipes_renamening_fix, remove_golem_gossip_fix,
    renamed_coral_fans_fix, renamed_coral_fix, reorganize_poi, scoreboard_display_name_fix,
    stats_rename_fix, trapped_chest_block_entity_fix, villager_data_fix,
    villager_rebuild_level_and_xp_fix, weapon_smith_chest_loot_table_fix,
    zombie_villager_rebuild_xp_fix,
};
use crate::legacy_component_data_fix_utils::create_text_component_json;
use crate::schema::Schema;
use crate::schemas::{
    v1460, v1466, v1470, v1481, v1483, v1486, v1488, v1510, v1800, v1801, v1904, v1906, v1909,
    v1920, v1928, v1929, v1931, v2100,
};
use crate::ty::ensure_namespaced;
use crate::value::Value;

/// `ImmutableMap.of("minecraft:sea_grass", "minecraft:seagrass", "minecraft:tall_sea_grass", "minecraft:tall_seagrass")`.
const SEAGRASS_RENAMES: [(&str, &str); 2] = [
    ("minecraft:sea_grass", "minecraft:seagrass"),
    ("minecraft:tall_sea_grass", "minecraft:tall_seagrass"),
];

const PRISMARINE_BRICK_RENAMES: [(&str, &str); 2] = [
    (
        "minecraft:prismarine_bricks_slab",
        "minecraft:prismarine_brick_slab",
    ),
    (
        "minecraft:prismarine_bricks_stairs",
        "minecraft:prismarine_brick_stairs",
    ),
];

fn purple_shulker_box_to_shulker_box(id: &str) -> String {
    if ensure_namespaced(id) == "minecraft:purple_shulker_box" {
        "minecraft:shulker_box".to_string()
    } else {
        id.to_string()
    }
}

/// The anonymous `DataFix` of `DataFixers.java`: the custom name of command block minecarts becomes a text component.
fn command_block_minecart_custom_name_fix(schema: &Schema) -> DataFix {
    DataFix::new("DataFixers$2", schema, false, |df| {
        let entity_type = df.input_schema().get_type(&References::ENTITY);
        let id_f = DSL::field_finder("id", &DSL::namespaced_string());
        let custom_name_f = entity_type.find_field("CustomName");
        let component_f =
            DSL::type_finder(&df.input_schema().get_type(&References::TEXT_COMPONENT));
        df.fix_type_everywhere_typed(
            "Command block minecart custom name fix",
            &entity_type,
            move |input| {
                let id = input
                    .get_optional(&id_f)
                    .map(|id| id.as_str().to_string())
                    .unwrap_or_default();
                if id != "minecraft:commandblock_minecart" {
                    return input;
                }
                input.update_typed(&custom_name_f, |custom_name| {
                    custom_name.update(&component_f, |pair| {
                        pair.map_second(|text| {
                            Value::Str(create_text_component_json(text.as_str()))
                        })
                    })
                })
            },
        )
    })
}

pub fn add(b: &mut DataFixerBuilder) {
    let v1460 = b.add_schema(1460, v1460::new);
    b.add_fixer(entity_painting_motive_fix::new(&v1460, false));
    let v1466 = b.add_schema(1466, v1466::new);
    b.add_fixer(add_new_choices::new(
        &v1466,
        "Add DUMMY block entity",
        &References::BLOCK_ENTITY,
    ));
    b.add_fixer(chunk_to_protochunk_fix::new(&v1466, true));
    let v1470 = b.add_schema(1470, v1470::new);
    b.add_fixer(add_new_choices::new(
        &v1470,
        "Add 1.13 entities fix",
        &References::ENTITY,
    ));
    let v1474 = b.add_schema(1474, Schema::same_namespaced);
    b.add_fixer(colorless_shulker_entity_fix::new(&v1474, false));
    b.add_fixer(legacy_block_rename_fix::create(
        &v1474,
        "Colorless shulker block fixer",
        purple_shulker_box_to_shulker_box,
    ));
    b.add_fixer(item_rename_fix::create(
        &v1474,
        "Colorless shulker item fixer",
        purple_shulker_box_to_shulker_box,
    ));
    let v1475 = b.add_schema(1475, Schema::same_namespaced);
    b.add_fixer(legacy_block_rename_fix::create(
        &v1475,
        "Flowing fixer",
        create_renamer(&[
            ("minecraft:flowing_water", "minecraft:water"),
            ("minecraft:flowing_lava", "minecraft:lava"),
        ]),
    ));
    let v1480 = b.add_schema(1480, Schema::same_namespaced);
    b.add_fixer(legacy_block_rename_fix::create(
        &v1480,
        "Rename coral blocks",
        create_renamer(renamed_coral_fix::renamed_ids()),
    ));
    b.add_fixer(item_rename_fix::create(
        &v1480,
        "Rename coral items",
        create_renamer(renamed_coral_fix::renamed_ids()),
    ));
    let v1481 = b.add_schema(1481, v1481::new);
    b.add_fixer(add_new_choices::new(
        &v1481,
        "Add conduit",
        &References::BLOCK_ENTITY,
    ));
    let v1483 = b.add_schema(1483, v1483::new);
    b.add_fixer(entity_pufferfish_rename_fix::new(&v1483, true));
    b.add_fixer(item_rename_fix::create(
        &v1483,
        "Rename pufferfish egg item",
        create_renamer(entity_pufferfish_rename_fix::renamed_ids()),
    ));
    let v1484 = b.add_schema(1484, Schema::same_namespaced);
    b.add_fixer(item_rename_fix::create(
        &v1484,
        "Rename seagrass items",
        create_renamer(&SEAGRASS_RENAMES),
    ));
    b.add_fixer(legacy_block_rename_fix::create(
        &v1484,
        "Rename seagrass blocks",
        create_renamer(&SEAGRASS_RENAMES),
    ));
    b.add_fixer(heightmap_renaming_fix::new(&v1484, false));
    let v1486 = b.add_schema(1486, v1486::new);
    b.add_fixer(entity_cod_salmon_fix::new(&v1486, true));
    b.add_fixer(item_rename_fix::create(
        &v1486,
        "Rename cod/salmon egg items",
        create_renamer(entity_cod_salmon_fix::renamed_egg_ids()),
    ));
    let v1487 = b.add_schema(1487, Schema::same_namespaced);
    b.add_fixer(item_rename_fix::create(
        &v1487,
        "Rename prismarine_brick(s)_* blocks",
        create_renamer(&PRISMARINE_BRICK_RENAMES),
    ));
    b.add_fixer(legacy_block_rename_fix::create(
        &v1487,
        "Rename prismarine_brick(s)_* items",
        create_renamer(&PRISMARINE_BRICK_RENAMES),
    ));
    let v1488 = b.add_schema(1488, v1488::new);
    b.add_fixer(legacy_block_rename_fix::create(
        &v1488,
        "Rename kelp/kelptop",
        create_renamer(&[
            ("minecraft:kelp_top", "minecraft:kelp"),
            ("minecraft:kelp", "minecraft:kelp_plant"),
        ]),
    ));
    b.add_fixer(item_rename_fix::create(
        &v1488,
        "Rename kelptop",
        create_renamer_pair("minecraft:kelp_top", "minecraft:kelp"),
    ));
    b.add_fixer(named_entity_write_read_fix::new(
        "DataFixers$1",
        &v1488,
        true,
        "Command block block entity custom name fix",
        &References::BLOCK_ENTITY,
        "minecraft:command_block",
        block_entity_custom_name_to_component_fix::fix_tag_custom_name,
    ));
    b.add_fixer(command_block_minecart_custom_name_fix(&v1488));
    b.add_fixer(igloo_metadata_removal_fix::new(&v1488, false));
    let v1490 = b.add_schema(1490, Schema::same_namespaced);
    b.add_fixer(legacy_block_rename_fix::create(
        &v1490,
        "Rename melon_block",
        create_renamer_pair("minecraft:melon_block", "minecraft:melon"),
    ));
    b.add_fixer(item_rename_fix::create(
        &v1490,
        "Rename melon_block/melon/speckled_melon",
        create_renamer(&[
            ("minecraft:melon_block", "minecraft:melon"),
            ("minecraft:melon", "minecraft:melon_slice"),
            (
                "minecraft:speckled_melon",
                "minecraft:glistering_melon_slice",
            ),
        ]),
    ));
    let v1492 = b.add_schema(1492, Schema::same_namespaced);
    b.add_fixer(chunk_structures_template_rename_fix::new(&v1492, false));
    let v1494 = b.add_schema(1494, Schema::same_namespaced);
    b.add_fixer(item_stack_enchantment_names_fix::new(&v1494, false));
    let v1496 = b.add_schema(1496, Schema::same_namespaced);
    b.add_fixer(leaves_fix::new(&v1496, false));
    let v1500 = b.add_schema(1500, Schema::same_namespaced);
    b.add_fixer(block_entity_keep_packed::new(&v1500, false));
    let v1501 = b.add_schema(1501, Schema::same_namespaced);
    b.add_fixer(advancements_fix::new(&v1501, false));
    let v1502 = b.add_schema(1502, Schema::same_namespaced);
    b.add_fixer(namespaced_type_rename_fix::new(
        &v1502,
        "Recipes fix",
        &References::RECIPE,
        create_renamer(recipes_fix::recipes()),
    ));
    let v1506 = b.add_schema(1506, Schema::same_namespaced);
    b.add_fixer(level_data_generator_options_fix::new(&v1506, false));
    let v1510 = b.add_schema(1510, v1510::new);
    b.add_fixer(legacy_block_rename_fix::create(
        &v1510,
        "Block renamening fix",
        create_renamer(entity_the_renamening_fix::renamed_blocks()),
    ));
    b.add_fixer(item_rename_fix::create(
        &v1510,
        "Item renamening fix",
        create_renamer(entity_the_renamening_fix::renamed_items()),
    ));
    b.add_fixer(namespaced_type_rename_fix::new(
        &v1510,
        "Recipes renamening fix",
        &References::RECIPE,
        create_renamer(recipes_renamening_fix::recipes()),
    ));
    b.add_fixer(entity_the_renamening_fix::new(&v1510, true));
    b.add_fixer(stats_rename_fix::new(
        &v1510,
        "SwimStatsRenameFix",
        &[
            ("minecraft:swim_one_cm", "minecraft:walk_on_water_one_cm"),
            ("minecraft:dive_one_cm", "minecraft:walk_under_water_one_cm"),
        ],
    ));
    let v1514 = b.add_schema(1514, Schema::same_namespaced);
    b.add_fixer(scoreboard_display_name_fix::new(
        &v1514,
        "ObjectiveDisplayNameFix",
        &References::OBJECTIVE,
    ));
    b.add_fixer(scoreboard_display_name_fix::new(
        &v1514,
        "TeamDisplayNameFix",
        &References::TEAM,
    ));
    b.add_fixer(objective_render_type_fix::new(&v1514));
    let v1515 = b.add_schema(1515, Schema::same_namespaced);
    b.add_fixer(legacy_block_rename_fix::create(
        &v1515,
        "Rename coral fan blocks",
        create_renamer(renamed_coral_fans_fix::renamed_ids()),
    ));
    let v1624 = b.add_schema(1624, Schema::same_namespaced);
    b.add_fixer(trapped_chest_block_entity_fix::new(&v1624, false));
    let v1800 = b.add_schema(1800, v1800::new);
    b.add_fixer(add_new_choices::new(
        &v1800,
        "Added 1.14 mobs fix",
        &References::ENTITY,
    ));
    b.add_fixer(item_rename_fix::create(
        &v1800,
        "Rename dye items",
        create_renamer(dye_item_rename_fix::renamed_ids()),
    ));
    let v1801 = b.add_schema(1801, v1801::new);
    b.add_fixer(add_new_choices::new(
        &v1801,
        "Added Illager Beast",
        &References::ENTITY,
    ));
    let v1802 = b.add_schema(1802, Schema::same_namespaced);
    b.add_fixer(legacy_block_rename_fix::create(
        &v1802,
        "Rename sign blocks & stone slabs",
        create_renamer(&[
            ("minecraft:stone_slab", "minecraft:smooth_stone_slab"),
            ("minecraft:sign", "minecraft:oak_sign"),
            ("minecraft:wall_sign", "minecraft:oak_wall_sign"),
        ]),
    ));
    b.add_fixer(item_rename_fix::create(
        &v1802,
        "Rename sign item & stone slabs",
        create_renamer(&[
            ("minecraft:stone_slab", "minecraft:smooth_stone_slab"),
            ("minecraft:sign", "minecraft:oak_sign"),
        ]),
    ));
    let v1803 = b.add_schema(1803, Schema::same_namespaced);
    b.add_fixer(item_lore_fix::new(&v1803));
    let v1904 = b.add_schema(1904, v1904::new);
    b.add_fixer(add_new_choices::new(
        &v1904,
        "Added Cats",
        &References::ENTITY,
    ));
    b.add_fixer(entity_cat_split_fix::new(&v1904, false));
    let v1905 = b.add_schema(1905, Schema::same_namespaced);
    b.add_fixer(chunk_status_fix::new(&v1905, false));
    let v1906 = b.add_schema(1906, v1906::new);
    b.add_fixer(add_new_choices::new(
        &v1906,
        "Add POI Blocks",
        &References::BLOCK_ENTITY,
    ));
    let v1909 = b.add_schema(1909, v1909::new);
    b.add_fixer(add_new_choices::new(
        &v1909,
        "Add jigsaw",
        &References::BLOCK_ENTITY,
    ));
    let v1911 = b.add_schema(1911, Schema::same_namespaced);
    b.add_fixer(chunk_status_fix2::new(&v1911, false));
    let v1914 = b.add_schema(1914, Schema::same_namespaced);
    b.add_fixer(weapon_smith_chest_loot_table_fix::new(&v1914, false));
    let v1917 = b.add_schema(1917, Schema::same_namespaced);
    b.add_fixer(cat_type_fix::new(&v1917, false));
    let v1918 = b.add_schema(1918, Schema::same_namespaced);
    b.add_fixer(villager_data_fix::new(&v1918, "minecraft:villager"));
    b.add_fixer(villager_data_fix::new(&v1918, "minecraft:zombie_villager"));
    let v1920 = b.add_schema(1920, v1920::new);
    b.add_fixer(new_village_fix::new(&v1920, false));
    b.add_fixer(add_new_choices::new(
        &v1920,
        "Add campfire",
        &References::BLOCK_ENTITY,
    ));
    let v1925 = b.add_schema(1925, Schema::same_namespaced);
    b.add_fixer(map_id_fix::new(&v1925));
    let v1928 = b.add_schema(1928, v1928::new);
    b.add_fixer(entity_ravager_rename_fix::new(&v1928, true));
    b.add_fixer(item_rename_fix::create(
        &v1928,
        "Rename ravager egg item",
        create_renamer(entity_ravager_rename_fix::renamed_ids()),
    ));
    let v1929 = b.add_schema(1929, v1929::new);
    b.add_fixer(add_new_choices::new(
        &v1929,
        "Add Wandering Trader and Trader Llama",
        &References::ENTITY,
    ));
    let v1931 = b.add_schema(1931, v1931::new);
    b.add_fixer(add_new_choices::new(
        &v1931,
        "Added Fox",
        &References::ENTITY,
    ));
    let v1936 = b.add_schema(1936, Schema::same_namespaced);
    b.add_fixer(options_add_text_background_fix::new(&v1936, false));
    let v1946 = b.add_schema(1946, Schema::same_namespaced);
    b.add_fixer(reorganize_poi::new(&v1946, false));
    let v1948 = b.add_schema(1948, Schema::same_namespaced);
    b.add_fixer(ominous_banner_rename_fix::new(&v1948));
    let v1953 = b.add_schema(1953, Schema::same_namespaced);
    b.add_fixer(ominous_banner_block_entity_rename_fix::new(&v1953, false));
    let v1955 = b.add_schema(1955, Schema::same_namespaced);
    b.add_fixer(villager_rebuild_level_and_xp_fix::new(&v1955, false));
    b.add_fixer(zombie_villager_rebuild_xp_fix::new(&v1955, false));
    let v1961 = b.add_schema(1961, Schema::same_namespaced);
    b.add_fixer(chunk_light_remove_fix::new(&v1961, false));
    let v1963 = b.add_schema(1963, Schema::same_namespaced);
    b.add_fixer(remove_golem_gossip_fix::new(&v1963, false));
    let v2100 = b.add_schema(2100, v2100::new);
    b.add_fixer(add_new_choices::new(
        &v2100,
        "Added Bee and Bee Stinger",
        &References::ENTITY,
    ));
    b.add_fixer(add_new_choices::new(
        &v2100,
        "Add beehive",
        &References::BLOCK_ENTITY,
    ));
    b.add_fixer(namespaced_type_rename_fix::new(
        &v2100,
        "Rename sugar recipe",
        &References::RECIPE,
        create_renamer_pair("minecraft:sugar", "minecraft:sugar_from_sugar_cane"),
    ));
    b.add_fixer(advancements_rename_fix::new(
        "AdvancementsRenameFix",
        &v2100,
        false,
        "Rename sugar recipe advancement",
        create_renamer_pair(
            "minecraft:recipes/misc/sugar",
            "minecraft:recipes/misc/sugar_from_sugar_cane",
        ),
    ));
}
