//! `EntityCodSalmonFix`: renames the `minecraft:salmon_mob` and `minecraft:cod_mob` entities.

use crate::datafix::DataFix;
use crate::fixes::simplest_entity_rename_fix;
use crate::schema::Schema;

/// `EntityCodSalmonFix.RENAMED_IDS`.
pub fn renamed_ids() -> &'static [(&'static str, &'static str)] {
    &[
        ("minecraft:salmon_mob", "minecraft:salmon"),
        ("minecraft:cod_mob", "minecraft:cod"),
    ]
}

/// `EntityCodSalmonFix.RENAMED_EGG_IDS`.
pub fn renamed_egg_ids() -> &'static [(&'static str, &'static str)] {
    &[
        (
            "minecraft:salmon_mob_spawn_egg",
            "minecraft:salmon_spawn_egg",
        ),
        ("minecraft:cod_mob_spawn_egg", "minecraft:cod_spawn_egg"),
    ]
}

/// `new EntityCodSalmonFix(schema, changesType)`.
pub fn new(schema: &Schema, changes_type: bool) -> DataFix {
    simplest_entity_rename_fix::new(
        "EntityCodSalmonFix",
        "EntityCodSalmonFix",
        schema,
        changes_type,
        |name| {
            renamed_ids()
                .iter()
                .find(|(old, _)| *old == name)
                .map_or(name, |(_, new)| new)
                .to_string()
        },
    )
}
