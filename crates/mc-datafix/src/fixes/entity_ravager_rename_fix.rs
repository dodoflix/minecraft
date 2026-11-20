//! `EntityRavagerRenameFix`: renames the `minecraft:illager_beast` entity to `minecraft:ravager`.

use crate::datafix::DataFix;
use crate::fixes::simplest_entity_rename_fix;
use crate::schema::Schema;

/// `EntityRavagerRenameFix.RENAMED_IDS`.
pub fn renamed_ids() -> &'static [(&'static str, &'static str)] {
    &[(
        "minecraft:illager_beast_spawn_egg",
        "minecraft:ravager_spawn_egg",
    )]
}

/// `new EntityRavagerRenameFix(outputSchema, changesType)`.
pub fn new(output_schema: &Schema, changes_type: bool) -> DataFix {
    simplest_entity_rename_fix::new(
        "EntityRavagerRenameFix",
        "EntityRavagerRenameFix",
        output_schema,
        changes_type,
        |name| {
            if name == "minecraft:illager_beast" {
                "minecraft:ravager".to_string()
            } else {
                name.to_string()
            }
        },
    )
}
