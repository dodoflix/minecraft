//! `EntityPufferfishRenameFix`: renames the `minecraft:puffer_fish` entity to `minecraft:pufferfish`.

use crate::datafix::DataFix;
use crate::fixes::simplest_entity_rename_fix;
use crate::schema::Schema;

/// `EntityPufferfishRenameFix.RENAMED_IDS`.
pub fn renamed_ids() -> &'static [(&'static str, &'static str)] {
    &[(
        "minecraft:puffer_fish_spawn_egg",
        "minecraft:pufferfish_spawn_egg",
    )]
}

/// `new EntityPufferfishRenameFix(outputSchema, changesType)`.
pub fn new(output_schema: &Schema, changes_type: bool) -> DataFix {
    simplest_entity_rename_fix::new(
        "EntityPufferfishRenameFix",
        "EntityPufferfishRenameFix",
        output_schema,
        changes_type,
        |name| {
            if name == "minecraft:puffer_fish" {
                "minecraft:pufferfish".to_string()
            } else {
                name.to_string()
            }
        },
    )
}
