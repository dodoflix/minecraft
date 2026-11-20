//! `ZombieVillagerSetVillagerDataFinalized`: sets `VillagerDataFinalized` to `true` on zombie villagers.

use crate::datafix::DataFix;
use crate::fixes::named_entity_fix;
use crate::fixes::references::References;
use crate::schema::Schema;

/// `new ZombieVillagerSetVillagerDataFinalized(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    named_entity_fix::new(
        "ZombieVillagerSetVillagerDataFinalized",
        output_schema,
        true,
        "Zombie Villager VillagerDataFinalized default value",
        &References::ENTITY,
        "minecraft:zombie_villager",
        |entity| {
            entity
                .update_remainder(|tag| tag.set("VillagerDataFinalized", tag.create_boolean(true)))
        },
    )
}
