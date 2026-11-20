//! `DataFixers.addFixers`, versions 1344..1459: the flattening.

use crate::fixer::DataFixerBuilder;
use crate::fixes::references::References;
use crate::fixes::{
    add_new_choices, block_entity_banner_color_fix, block_entity_block_state_fix,
    block_entity_custom_name_to_component_fix, block_entity_jukebox_fix, block_name_flattening_fix,
    block_state_structure_template_fix, chunk_paletted_storage_fix, entity_block_state_fix,
    entity_custom_name_to_component_fix, entity_item_frame_direction_fix, entity_wolf_color_fix,
    item_custom_name_to_component_fix, item_stack_map_id_fix, item_stack_spawn_egg_fix,
    item_stack_the_flattening_fix, level_flat_generator_info_fix, options_key_lwjgl3_fix,
    options_key_translation_fix, remove_block_entity_tag_fix, stats_counter_fix,
    villager_trade_fix,
};
use crate::schema::Schema;
use crate::schemas::{v1451, v1451_1, v1451_2, v1451_3, v1451_4, v1451_5, v1451_6, v1458};

pub fn add(b: &mut DataFixerBuilder) {
    let v1344 = b.add_schema(1344, Schema::same_namespaced);
    b.add_fixer(options_key_lwjgl3_fix::new(&v1344, false));
    let v1446 = b.add_schema(1446, Schema::same_namespaced);
    b.add_fixer(options_key_translation_fix::new(&v1446, false));
    let v1450 = b.add_schema(1450, Schema::same_namespaced);
    b.add_fixer(block_state_structure_template_fix::new(&v1450, false));
    let v1451 = b.add_schema(1451, v1451::new);
    b.add_fixer(add_new_choices::new(
        &v1451,
        "AddTrappedChestFix",
        &References::BLOCK_ENTITY,
    ));
    let v1451_1 = b.add_schema_sub(1451, 1, v1451_1::new);
    b.add_fixer(chunk_paletted_storage_fix::new(&v1451_1, true));
    let v1451_2 = b.add_schema_sub(1451, 2, v1451_2::new);
    b.add_fixer(block_entity_block_state_fix::new(&v1451_2, true));
    let v1451_3 = b.add_schema_sub(1451, 3, v1451_3::new);
    b.add_fixer(entity_block_state_fix::new(&v1451_3, true));
    b.add_fixer(item_stack_map_id_fix::new(&v1451_3, false));
    let v1451_4 = b.add_schema_sub(1451, 4, v1451_4::new);
    b.add_fixer(block_name_flattening_fix::new(&v1451_4, true));
    b.add_fixer(item_stack_the_flattening_fix::new(&v1451_4, false));
    let v1451_5 = b.add_schema_sub(1451, 5, v1451_5::new);
    b.add_fixer(remove_block_entity_tag_fix::new_with_legacy_data_structure(
        &v1451_5,
        true,
        &["minecraft:noteblock", "minecraft:flower_pot"],
    ));
    b.add_fixer(item_stack_spawn_egg_fix::new(
        &v1451_5,
        false,
        "minecraft:spawn_egg",
    ));
    b.add_fixer(entity_wolf_color_fix::new(&v1451_5, false));
    b.add_fixer(block_entity_banner_color_fix::new(&v1451_5, false));
    b.add_fixer(level_flat_generator_info_fix::new(&v1451_5, false));
    let v1451_6 = b.add_schema_sub(1451, 6, v1451_6::new);
    b.add_fixer(stats_counter_fix::new(&v1451_6, true));
    b.add_fixer(block_entity_jukebox_fix::new(&v1451_6, false));
    let v1451_8 = b.add_schema_sub(1451, 7, Schema::same_namespaced);
    b.add_fixer(villager_trade_fix::new(&v1451_8));
    let v1456 = b.add_schema(1456, Schema::same_namespaced);
    b.add_fixer(entity_item_frame_direction_fix::new(&v1456, false));
    let v1458 = b.add_schema(1458, v1458::new);
    b.add_fixer(entity_custom_name_to_component_fix::new(&v1458));
    b.add_fixer(item_custom_name_to_component_fix::new(&v1458));
    b.add_fixer(block_entity_custom_name_to_component_fix::new(&v1458));
}
