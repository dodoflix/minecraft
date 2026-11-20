//! `EntityBrushableBlockFieldsRenameFix`: renames the loot table fields of a brushable block.

use crate::datafix::DataFix;
use crate::fixes::named_entity_fix;
use crate::fixes::references::References;
use crate::schema::Schema;

/// `new EntityBrushableBlockFieldsRenameFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    named_entity_fix::new(
        "EntityBrushableBlockFieldsRenameFix",
        output_schema,
        false,
        "EntityBrushableBlockFieldsRenameFix",
        &References::BLOCK_ENTITY,
        "minecraft:brushable_block",
        |entity| {
            entity.update_remainder(|input| {
                input
                    .rename_field("loot_table", "LootTable")
                    .rename_field("loot_table_seed", "LootTableSeed")
            })
        },
    )
}
