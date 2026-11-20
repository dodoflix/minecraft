//! `DropChancesFormatFix`: folds `ArmorDropChances`, `HandDropChances` and `body_armor_drop_chance` of entities into one
//! `drop_chances` map keyed by equipment slot, keeping only non-default chances.

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;
use mc_dfu::{Dynamic, OptionalDynamic};

const ARMOR_SLOT_NAMES: [&str; 4] = ["feet", "legs", "chest", "head"];
const HAND_SLOT_NAMES: [&str; 2] = ["mainhand", "offhand"];
const DEFAULT_CHANCE: f32 = 0.085;

/// `new DropChancesFormatFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new("DropChancesFormatFix", output_schema, false, |df| {
        df.fix_type_everywhere_typed(
            "DropChancesFormatFix",
            &df.input_schema().get_type(&References::ENTITY),
            |input| input.update_remainder(fix_remainder),
        )
    })
}

fn fix_remainder(remainder: Dynamic) -> Dynamic {
    let armor_drop_chances = parse_drop_chances(&remainder.get("ArmorDropChances"));
    let hand_drop_chances = parse_drop_chances(&remainder.get("HandDropChances"));
    let body_armor_drop_chance = remainder
        .get("body_armor_drop_chance")
        .as_float_or(DEFAULT_CHANCE);
    let remainder = remainder
        .remove("ArmorDropChances")
        .remove("HandDropChances")
        .remove("body_armor_drop_chance");
    let mut slot_chances = remainder.empty_map();
    slot_chances = add_slot_chances(slot_chances, &armor_drop_chances, &ARMOR_SLOT_NAMES);
    slot_chances = add_slot_chances(slot_chances, &hand_drop_chances, &HAND_SLOT_NAMES);
    if body_armor_drop_chance != DEFAULT_CHANCE {
        slot_chances = slot_chances.set("body", remainder.create_float(body_armor_drop_chance));
    }
    if slot_chances != remainder.empty_map() {
        remainder.set("drop_chances", slot_chances)
    } else {
        remainder
    }
}

fn add_slot_chances(mut output: Dynamic, chances: &[f32], slot_names: &[&str]) -> Dynamic {
    for (slot, &chance) in slot_names.iter().zip(chances) {
        if chance != DEFAULT_CHANCE {
            output = output.set(*slot, output.create_float(chance));
        }
    }
    output
}

fn parse_drop_chances(value: &OptionalDynamic) -> Vec<f32> {
    value
        .as_stream()
        .into_iter()
        .map(|dynamic| dynamic.as_float_or(DEFAULT_CHANCE))
        .collect()
}
