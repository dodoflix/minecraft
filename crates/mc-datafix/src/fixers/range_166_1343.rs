//! `DataFixers.addFixers`, versions 166..1343: the schemas and fixes.

use crate::data_fixers::create_renamer_pair;
use crate::fixer::DataFixerBuilder;
use crate::fixes::references::References;
use crate::fixes::{
    add_new_choices, bed_item_color_fix, block_entity_id_fix, block_entity_shulker_box_color_fix,
    chunk_bed_block_entity_injecter_fix, entity_elder_guardian_split_fix, entity_horse_split_fix,
    entity_id_fix, entity_shulker_color_fix, entity_skeleton_split_fix, entity_zombie_split_fix,
    entity_zombie_villager_type_fix, item_banner_color_fix, item_rename_fix,
    item_shulker_box_color_fix, item_water_potion_fix, options_force_vbo_fix,
    options_lower_case_language_fix, write_and_read_fix,
};
use crate::schema::Schema;
use crate::schemas::{v501, v700, v701, v702, v703, v704, v705, v808, v1022, v1125};
use crate::ty::ensure_namespaced;

pub fn add(b: &mut DataFixerBuilder) {
    let v501 = b.add_schema(501, v501::new);
    b.add_fixer(add_new_choices::new(
        &v501,
        "Add 1.10 entities fix",
        &References::ENTITY,
    ));
    let v502 = b.add_schema(502, Schema::same);
    b.add_fixer(item_rename_fix::create(
        &v502,
        "cooked_fished item renamer",
        |item| {
            if ensure_namespaced(item) == "minecraft:cooked_fished" {
                "minecraft:cooked_fish".to_string()
            } else {
                item.to_string()
            }
        },
    ));
    b.add_fixer(entity_zombie_villager_type_fix::new(&v502, false));
    let v505 = b.add_schema(505, Schema::same);
    b.add_fixer(options_force_vbo_fix::new(&v505, false));
    let v700 = b.add_schema(700, v700::new);
    b.add_fixer(entity_elder_guardian_split_fix::new(&v700, true));
    let v701 = b.add_schema(701, v701::new);
    b.add_fixer(entity_skeleton_split_fix::new(&v701, true));
    let v702 = b.add_schema(702, v702::new);
    b.add_fixer(entity_zombie_split_fix::new(&v702));
    let v703 = b.add_schema(703, v703::new);
    b.add_fixer(entity_horse_split_fix::new(&v703, true));
    let v704 = b.add_schema(704, v704::new);
    b.add_fixer(block_entity_id_fix::new(&v704, true));
    let v705 = b.add_schema(705, v705::new);
    b.add_fixer(entity_id_fix::new(&v705, true));
    let v804 = b.add_schema(804, Schema::same_namespaced);
    b.add_fixer(item_banner_color_fix::new(&v804, true));
    let v806 = b.add_schema(806, Schema::same_namespaced);
    b.add_fixer(item_water_potion_fix::new(&v806, false));
    let v808 = b.add_schema(808, v808::new);
    b.add_fixer(add_new_choices::new(
        &v808,
        "added shulker box",
        &References::BLOCK_ENTITY,
    ));
    let v808_1 = b.add_schema_sub(808, 1, Schema::same_namespaced);
    b.add_fixer(entity_shulker_color_fix::new(&v808_1, false));
    let v813 = b.add_schema(813, Schema::same_namespaced);
    b.add_fixer(item_shulker_box_color_fix::new(&v813, false));
    b.add_fixer(block_entity_shulker_box_color_fix::new(&v813, false));
    let v816 = b.add_schema(816, Schema::same_namespaced);
    b.add_fixer(options_lower_case_language_fix::new(&v816, false));
    let v820 = b.add_schema(820, Schema::same_namespaced);
    b.add_fixer(item_rename_fix::create(
        &v820,
        "totem item renamer",
        create_renamer_pair("minecraft:totem", "minecraft:totem_of_undying"),
    ));
    let v1022 = b.add_schema(1022, v1022::new);
    b.add_fixer(write_and_read_fix::new(
        &v1022,
        "added shoulder entities to players",
        &References::PLAYER,
    ));
    let v1125 = b.add_schema(1125, v1125::new);
    b.add_fixer(chunk_bed_block_entity_injecter_fix::new(&v1125, true));
    b.add_fixer(bed_item_color_fix::new(&v1125, false));
}
