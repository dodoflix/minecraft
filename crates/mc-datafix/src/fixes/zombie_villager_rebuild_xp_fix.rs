//! `ZombieVillagerRebuildXpFix`: gives zombie villagers without `Xp` the minimum xp of their level.

use crate::datafix::DataFix;
use crate::fixes::named_entity_fix;
use crate::fixes::references::References;
use crate::fixes::villager_rebuild_level_and_xp_fix::get_min_xp_per_level;
use crate::schema::Schema;

/// `new ZombieVillagerRebuildXpFix(outputSchema, changesType)`.
pub fn new(output_schema: &Schema, changes_type: bool) -> DataFix {
    named_entity_fix::new(
        "ZombieVillagerRebuildXpFix",
        output_schema,
        changes_type,
        "Zombie Villager XP rebuild",
        &References::ENTITY,
        "minecraft:zombie_villager",
        |entity| {
            entity.update_remainder(|remainder| {
                if remainder.get("Xp").as_number().result().is_none() {
                    let level = remainder.get("VillagerData").get("level").as_int_or(1);
                    remainder.set("Xp", remainder.create_int(get_min_xp_per_level(level)))
                } else {
                    remainder
                }
            })
        },
    )
}
