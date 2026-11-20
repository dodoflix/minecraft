//! `EntityHorseSplitFix`: splits `EntityHorse` into donkey, mule, zombie horse, skeleton horse and horse by `Type`.

use crate::datafix::DataFix;
use crate::finder::remainder_finder;
use crate::fixes::entity_rename_fix;
use crate::fixes::references::References;
use crate::schema::Schema;
use crate::util::write_and_read_typed_or_throw;

/// `new EntityHorseSplitFix(outputSchema, changesType)`.
pub fn new(output_schema: &Schema, changes_type: bool) -> DataFix {
    let output_schema2 = output_schema.clone();
    entity_rename_fix::new(
        "EntityHorseSplitFix",
        "EntityHorseSplitFix",
        output_schema,
        changes_type,
        move |name, entity| {
            if name != "EntityHorse" {
                return (name.to_string(), entity);
            }
            let new_name = match entity
                .get(&remainder_finder())
                .into_dynamic()
                .get("Type")
                .as_int_or(0)
            {
                1 => "Donkey",
                2 => "Mule",
                3 => "ZombieHorse",
                4 => "SkeletonHorse",
                _ => "Horse",
            };
            let choices = output_schema2.find_choice_type(&References::ENTITY);
            let new_type = choices.get(new_name).unwrap_or_else(|| {
                panic!("java.lang.NullPointerException: Cannot invoke \"com.mojang.datafixers.types.Type.readTyped(com.mojang.serialization.Dynamic)\" because \"type\" is null")
            });
            (
                new_name.to_string(),
                write_and_read_typed_or_throw(&entity, new_type, |d| d.remove("Type")),
            )
        },
    )
}
