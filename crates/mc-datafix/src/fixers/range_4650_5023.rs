//! `DataFixers.addFixers`, versions 4650..5023: the schemas and the fixes of versions 4996 and up (the fixes of
//! 4650..4892 are not registered yet).

use crate::data_fixers::create_renamer;
use crate::fixer::DataFixerBuilder;
use crate::fixes::references::References;
use crate::fixes::{
    add_new_choices, blending_data_fix, blending_data_no_value_height_fix,
    block_state_field_names_fix, debug_profile_rename_entry_fix, explorer_map_item_fix,
    item_rename_fix, map_color_component_removal_fix, merge_terrain_chunk_status_fix,
    pot_decorations_block_entity_unflattening_fix, pot_decorations_component_unflattening_fix,
    remove_block_transformer_component_fix, signs_op_flag_fix, swing_animation_component_split_fix,
    unfilled_buried_treasure_map_fix,
};
use crate::schema::Schema;
use crate::schemas::{v4656, v4771, v4881, v4885, v4996, v4996_1, v4997, v5000};

pub fn add(b: &mut DataFixerBuilder) {
    b.add_schema(4650, Schema::same_namespaced);
    b.add_schema(4651, Schema::same_namespaced);
    b.add_schema(4656, v4656::new);
    b.add_schema(4657, Schema::same_namespaced);
    b.add_schema(4658, Schema::same_namespaced);
    b.add_schema(4661, Schema::same_namespaced);
    b.add_schema(4763, Schema::same_namespaced);
    b.add_schema(4766, Schema::same_namespaced);
    b.add_schema(4769, Schema::same_namespaced);
    b.add_schema(4771, v4771::new);
    b.add_schema(4881, v4881::new);
    b.add_schema(4885, v4885::new);
    b.add_schema(4888, Schema::same_namespaced);
    b.add_schema(4892, Schema::same_namespaced);
    let v4996 = b.add_schema(4996, v4996::new);
    b.add_fixer(pot_decorations_component_unflattening_fix::new(&v4996));
    let v4996_1 = b.add_schema_sub(4996, 1, v4996_1::new);
    b.add_fixer(pot_decorations_block_entity_unflattening_fix::new(&v4996_1));
    let blending_schema = b.add_schema(4997, Schema::same_namespaced);
    b.add_fixer(blending_data_fix::new(&blending_schema));
    let v4997 = b.add_schema(4997, v4997::new);
    b.add_fixer(add_new_choices::new(
        &v4997,
        "Added Poplar Boat and Poplar Chest Boat",
        &References::ENTITY,
    ));
    let v5000 = b.add_schema(5000, v5000::new);
    b.add_fixer(add_new_choices::new(
        &v5000,
        "Added Cushion",
        &References::ENTITY,
    ));
    let v5001 = b.add_schema(5001, Schema::same_namespaced);
    b.add_fixer(debug_profile_rename_entry_fix::new(
        &v5001,
        "minecraft:post_effect",
        "minecraft:post_effects",
    ));
    let v5002 = b.add_schema(5002, Schema::same_namespaced);
    b.add_fixer(signs_op_flag_fix::new(&v5002));
    let v5006 = b.add_schema(5006, Schema::same_namespaced);
    b.add_fixer(block_state_field_names_fix::new(&v5006));
    let v5007 = b.add_schema(5007, Schema::same_namespaced);
    b.add_fixer(swing_animation_component_split_fix::new(&v5007));
    let v5008 = b.add_schema(5008, Schema::same_namespaced);
    b.add_fixer(explorer_map_item_fix::new(&v5008));
    b.add_fixer(map_color_component_removal_fix::new(&v5008));
    let v5009 = b.add_schema(5009, Schema::same_namespaced);
    b.add_fixer(blending_data_no_value_height_fix::new(&v5009));
    let v5012 = b.add_schema(5012, Schema::same_namespaced);
    b.add_fixer(item_rename_fix::create(
        &v5012,
        "Rename explorer map items",
        create_renamer(&[
            (
                "minecraft:ocean_explorer_map",
                "minecraft:ocean_monument_map",
            ),
            ("minecraft:swamp_explorer_map", "minecraft:swamp_hut_map"),
            (
                "minecraft:trial_explorer_map",
                "minecraft:buried_trial_chambers_map",
            ),
            (
                "minecraft:woodland_explorer_map",
                "minecraft:woodland_mansion_map",
            ),
            (
                "minecraft:jungle_explorer_map",
                "minecraft:jungle_pyramid_map",
            ),
        ]),
    ));
    let v5013 = b.add_schema(5013, Schema::same_namespaced);
    b.add_fixer(merge_terrain_chunk_status_fix::new(&v5013));
    let v5014 = b.add_schema(5014, Schema::same_namespaced);
    b.add_fixer(remove_block_transformer_component_fix::new(&v5014));
    let v5016 = b.add_schema(5016, Schema::same_namespaced);
    b.add_fixer(unfilled_buried_treasure_map_fix::new(&v5016));
}
