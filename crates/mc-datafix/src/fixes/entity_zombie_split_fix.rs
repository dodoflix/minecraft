//! `EntityZombieSplitFix`: splits zombies into `ZombieVillager`, `Husk` and `Zombie` by `ZombieType`.

use crate::datafix::DataFix;
use crate::fixes::entity_rename_fix;
use crate::fixes::references::References;
use crate::schema::Schema;
use crate::typed::Typed;
use crate::util::write_and_read_typed_or_throw;

/// `new EntityZombieSplitFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    let output_schema2 = output_schema.clone();
    entity_rename_fix::new(
        "EntityZombieSplitFix",
        "EntityZombieSplitFix",
        output_schema,
        true,
        move |name, entity| {
            if name != "Zombie" {
                return (name.to_string(), entity);
            }
            let tag = entity.get_remainder();
            let ty = tag.get("ZombieType").as_int_or(0);
            let (new_name, new_entity): (&str, Typed) = match ty {
                1..=5 => {
                    // Java memoizes this lookup; it is only needed (and can only fail) here
                    let zombie_villager_type =
                        output_schema2.get_choice_type(&References::ENTITY, "ZombieVillager");
                    let entity =
                        write_and_read_typed_or_throw(&entity, &zombie_villager_type, |d| {
                            d.set("Profession", d.create_int(ty - 1))
                        });
                    ("ZombieVillager", entity)
                }
                6 => ("Husk", entity),
                _ => ("Zombie", entity),
            };
            (
                new_name.to_string(),
                new_entity.update_remainder(|e| e.remove("ZombieType")),
            )
        },
    )
}
