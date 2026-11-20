//! `EntityZombifiedPiglinRenameFix`: renames the `minecraft:zombie_pigman` entity to `minecraft:zombified_piglin`.

use crate::datafix::DataFix;
use crate::fixes::simplest_entity_rename_fix;
use crate::schema::Schema;

/// `EntityZombifiedPiglinRenameFix.RENAMED_IDS`.
pub fn renamed_ids() -> &'static [(&'static str, &'static str)] {
    &[(
        "minecraft:zombie_pigman_spawn_egg",
        "minecraft:zombified_piglin_spawn_egg",
    )]
}

/// `new EntityZombifiedPiglinRenameFix(outputSchema)`.
pub fn new(schema: &Schema) -> DataFix {
    simplest_entity_rename_fix::new(
        "EntityZombifiedPiglinRenameFix",
        "EntityZombifiedPiglinRenameFix",
        schema,
        true,
        |name| {
            if name == "minecraft:zombie_pigman" {
                "minecraft:zombified_piglin".to_string()
            } else {
                name.to_string()
            }
        },
    )
}
