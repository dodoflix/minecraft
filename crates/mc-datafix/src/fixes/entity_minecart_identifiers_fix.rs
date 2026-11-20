//! `EntityMinecartIdentifiersFix`: splits the `Minecart` entity into `MinecartRideable`, `MinecartChest` and
//! `MinecartFurnace` by its `Type`.

use crate::datafix::DataFix;
use crate::fixes::entity_rename_fix;
use crate::fixes::references::References;
use crate::schema::Schema;
use crate::util::write_and_read_typed_or_throw;

/// `new EntityMinecartIdentifiersFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    let output_schema2 = output_schema.clone();
    entity_rename_fix::new(
        "EntityMinecartIdentifiersFix",
        "EntityMinecartIdentifiersFix",
        output_schema,
        true,
        move |name, entity| {
            if name != "Minecart" {
                return (name.to_string(), entity);
            }
            let id = entity.get_remainder().get("Type").as_int_or(0);
            let new_name = match id {
                1 => "MinecartChest",
                2 => "MinecartFurnace",
                _ => "MinecartRideable",
            };
            let choice_type = output_schema2.find_choice_type(&References::ENTITY);
            let Some(new_type) = choice_type.get(new_name) else {
                panic!(
                    "java.lang.NullPointerException: Cannot invoke \"com.mojang.datafixers.types.Type.readTyped(com.mojang.serialization.Dynamic)\" because \"type\" is null"
                )
            };
            (
                new_name.to_string(),
                write_and_read_typed_or_throw(&entity, new_type, |dynamic| dynamic.remove("Type")),
            )
        },
    )
}
