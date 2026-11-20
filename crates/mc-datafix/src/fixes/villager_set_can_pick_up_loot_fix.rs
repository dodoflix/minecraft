//! `VillagerSetCanPickUpLootFix`: sets `CanPickUpLoot` to `true` on every villager.

use crate::datafix::DataFix;
use crate::fixes::named_entity_fix;
use crate::fixes::references::References;
use crate::schema::Schema;

/// `new VillagerSetCanPickUpLootFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    named_entity_fix::new(
        "VillagerSetCanPickUpLootFix",
        output_schema,
        true,
        "Villager CanPickUpLoot default value",
        &References::ENTITY,
        "Villager",
        |entity| entity.update_remainder(|tag| tag.set("CanPickUpLoot", tag.create_boolean(true))),
    )
}
