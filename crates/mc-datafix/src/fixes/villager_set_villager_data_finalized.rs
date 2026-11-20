//! `VillagerSetVillagerDataFinalized`: sets `VillagerDataFinalized` to `true` on villagers.

use crate::datafix::DataFix;
use crate::fixes::named_entity_fix;
use crate::fixes::references::References;
use crate::schema::Schema;

/// `new VillagerSetVillagerDataFinalized(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    named_entity_fix::new(
        "VillagerSetVillagerDataFinalized",
        output_schema,
        true,
        "Villager VillagerDataFinalized default value",
        &References::ENTITY,
        "minecraft:villager",
        |entity| {
            entity
                .update_remainder(|tag| tag.set("VillagerDataFinalized", tag.create_boolean(true)))
        },
    )
}
