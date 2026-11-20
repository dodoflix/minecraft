//! `DataFixers.addFixers`, versions 99..165: the schemas and fixes.

use crate::fixer::DataFixerBuilder;
use crate::fixes::{
    entity_armor_stand_silent_fix, entity_equipment_to_armor_and_hand_fix, entity_health_fix,
    entity_horse_saddle_fix, entity_minecart_identifiers_fix,
    entity_painting_item_frame_direction_fix, entity_redundant_chance_tags_fix,
    entity_riding_to_passengers_fix, entity_string_uuid_fix, entity_tipped_arrow_fix, item_id_fix,
    item_potion_fix, item_spawn_egg_fix, mob_spawner_entity_identifiers_fix,
    sign_text_strict_json_fix, villager_set_can_pick_up_loot_fix,
    written_book_pages_strict_json_fix,
};
use crate::schema::Schema;
use crate::schemas::{v99, v100, v102, v106, v107, v135, v143};

pub fn add(b: &mut DataFixerBuilder) {
    b.add_schema(99, v99::new);
    let v100 = b.add_schema(100, v100::new);
    b.add_fixer(entity_equipment_to_armor_and_hand_fix::new(&v100));
    let v101 = b.add_schema(101, Schema::same);
    b.add_fixer(villager_set_can_pick_up_loot_fix::new(&v101));
    let v102 = b.add_schema(102, v102::new);
    b.add_fixer(item_id_fix::new(&v102, true));
    b.add_fixer(item_potion_fix::new(&v102, false));
    let v105 = b.add_schema(105, Schema::same);
    b.add_fixer(item_spawn_egg_fix::new(&v105, true));
    let v106 = b.add_schema(106, v106::new);
    b.add_fixer(mob_spawner_entity_identifiers_fix::new(&v106, true));
    let v107 = b.add_schema(107, v107::new);
    b.add_fixer(entity_minecart_identifiers_fix::new(&v107));
    let v108 = b.add_schema(108, Schema::same);
    b.add_fixer(entity_string_uuid_fix::new(&v108, true));
    let v109 = b.add_schema(109, Schema::same);
    b.add_fixer(entity_health_fix::new(&v109, true));
    let v110 = b.add_schema(110, Schema::same);
    b.add_fixer(entity_horse_saddle_fix::new(&v110, true));
    let v111 = b.add_schema(111, Schema::same);
    b.add_fixer(entity_painting_item_frame_direction_fix::new(&v111, true));
    let v113 = b.add_schema(113, Schema::same);
    b.add_fixer(entity_redundant_chance_tags_fix::new(&v113, true));
    let v135 = b.add_schema(135, v135::new);
    b.add_fixer(entity_riding_to_passengers_fix::new(&v135, true));
    let v143 = b.add_schema(143, v143::new);
    b.add_fixer(entity_tipped_arrow_fix::new(&v143, true));
    let v147 = b.add_schema(147, Schema::same);
    b.add_fixer(entity_armor_stand_silent_fix::new(&v147, true));
    let v165 = b.add_schema(165, Schema::same);
    b.add_fixer(sign_text_strict_json_fix::new(&v165));
    b.add_fixer(written_book_pages_strict_json_fix::new(&v165));
}
