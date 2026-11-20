//! `WeaponSmithChestLootTableFix`: renames the blacksmith village chest loot table to the weaponsmith one.

use crate::datafix::DataFix;
use crate::fixes::named_entity_fix;
use crate::fixes::references::References;
use crate::schema::Schema;

/// `new WeaponSmithChestLootTableFix(outputSchema, changesType)`.
pub fn new(output_schema: &Schema, changes_type: bool) -> DataFix {
    named_entity_fix::new(
        "WeaponSmithChestLootTableFix",
        output_schema,
        changes_type,
        "WeaponSmithChestLootTableFix",
        &References::BLOCK_ENTITY,
        "minecraft:chest",
        |entity| {
            entity.update_remainder(|tag| {
                let loot_table = tag.get("LootTable").as_string_or("");
                if loot_table == "minecraft:chests/village_blacksmith" {
                    tag.set(
                        "LootTable",
                        tag.create_string("minecraft:chests/village/village_weaponsmith"),
                    )
                } else {
                    tag
                }
            })
        },
    )
}
