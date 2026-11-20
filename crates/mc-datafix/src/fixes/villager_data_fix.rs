//! `VillagerDataFix`: replaces `Profession`/`Career`/`CareerLevel` by a `VillagerData` compound.

use crate::datafix::DataFix;
use crate::finder::remainder_finder;
use crate::fixes::named_entity_fix;
use crate::fixes::references::References;
use crate::schema::Schema;

/// `new VillagerDataFix(schema, entityType)`.
pub fn new(schema: &Schema, entity_type: &str) -> DataFix {
    named_entity_fix::new(
        "VillagerDataFix",
        schema,
        false,
        &format!("Villager profession data fix ({entity_type})"),
        &References::ENTITY,
        entity_type,
        |entity| {
            let remainder = entity.get(&remainder_finder()).into_dynamic();
            let villager_data = remainder.create_map([
                (
                    remainder.create_string("type"),
                    remainder.create_string("minecraft:plains"),
                ),
                (
                    remainder.create_string("profession"),
                    remainder.create_string(upgrade_data(
                        remainder.get("Profession").as_int_or(0),
                        remainder.get("Career").as_int_or(0),
                    )),
                ),
                (
                    remainder.create_string("level"),
                    remainder
                        .get("CareerLevel")
                        .result()
                        .unwrap_or_else(|| remainder.create_int(1)),
                ),
            ]);
            entity.set_remainder(
                remainder
                    .remove("Profession")
                    .remove("Career")
                    .remove("CareerLevel")
                    .set("VillagerData", villager_data),
            )
        },
    )
}

/// `VillagerDataFix.upgradeData(profession, career)`.
fn upgrade_data(profession: i32, career: i32) -> &'static str {
    match profession {
        0 => match career {
            2 => "minecraft:fisherman",
            3 => "minecraft:shepherd",
            4 => "minecraft:fletcher",
            _ => "minecraft:farmer",
        },
        1 => {
            if career == 2 {
                "minecraft:cartographer"
            } else {
                "minecraft:librarian"
            }
        }
        2 => "minecraft:cleric",
        3 => match career {
            2 => "minecraft:weaponsmith",
            3 => "minecraft:toolsmith",
            _ => "minecraft:armorer",
        },
        4 => {
            if career == 2 {
                "minecraft:leatherworker"
            } else {
                "minecraft:butcher"
            }
        }
        5 => "minecraft:nitwit",
        _ => "minecraft:none",
    }
}
