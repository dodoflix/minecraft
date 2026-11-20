//! `PlayerEquipmentFix`: moves the armor and offhand stacks of a player's `Inventory` (slots 100-103 and -106) into an
//! `equipment` map keyed by slot name.

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;
use mc_core::java_collections::{HASH_MAP_DEFAULT_CAPACITY, hash_map_ordered};
use mc_dfu::Dynamic;

fn slot_translation(slot: i32) -> Option<&'static str> {
    match slot {
        100 => Some("feet"),
        101 => Some("legs"),
        102 => Some("chest"),
        103 => Some("head"),
        -106 => Some("offhand"),
        _ => None,
    }
}

/// `new PlayerEquipmentFix(outputSchema)` (`changesType` is `true`).
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new("PlayerEquipmentFix", output_schema, true, |df| {
        let old_player_type = df.input_schema().get_type_raw(&References::PLAYER);
        let new_player_type = df.output_schema().get_type_raw(&References::PLAYER);
        df.write_fix_and_read(
            "Player Equipment Fix",
            &old_player_type,
            &new_player_type,
            fix_player,
        )
    })
}

fn fix_player(tag: Dynamic) -> Dynamic {
    // a `HashMap<Dynamic, Dynamic>`: a repeated slot replaces the value in place
    let mut equipment: Vec<(Dynamic, Dynamic)> = Vec::new();
    let tag = tag.update("Inventory", |inventory| {
        let kept: Vec<Dynamic> = inventory
            .as_stream()
            .into_iter()
            .filter(|item| {
                let inventory_slot = item.get("Slot").as_int_or(-1);
                let equipment_slot = slot_translation(inventory_slot);
                if let Some(equipment_slot) = equipment_slot {
                    let key = inventory.create_string(equipment_slot);
                    let value = item.remove("Slot");
                    match equipment.iter_mut().find(|(k, _)| *k == key) {
                        Some(entry) => entry.1 = value,
                        None => equipment.push((key, value)),
                    }
                }
                equipment_slot.is_none()
            })
            .collect();
        inventory.create_list(kept)
    });
    let equipment = hash_map_ordered(equipment, |(k, _)| k.hash_code(), HASH_MAP_DEFAULT_CAPACITY);
    tag.set("equipment", tag.create_map(equipment))
}
