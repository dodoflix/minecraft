//! `MobSpawnerEntityIdentifiersFix`: moves the spawner's `EntityId` into `SpawnData.id` and the `SpawnPotentials`'
//! `Type` / `Properties` into `Entity`.

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::schema::Schema;
use mc_dfu::Dynamic;

/// `new MobSpawnerEntityIdentifiersFix(outputSchema, changesType)`.
pub fn new(output_schema: &Schema, changes_type: bool) -> DataFix {
    DataFix::new(
        "MobSpawnerEntityIdentifiersFix",
        output_schema,
        changes_type,
        |df| {
            let new_type = df.output_schema().get_type(&References::UNTAGGED_SPAWNER);
            let new_type2 = new_type.clone();
            df.fix_type_everywhere_typed_new(
                "MobSpawnerEntityIdentifiersFix",
                &df.input_schema().get_type(&References::UNTAGGED_SPAWNER),
                &new_type,
                move |input| {
                    let tag = input.get(&DSL::remainder_finder()).into_dynamic();
                    let tag = tag.set("id", tag.create_string("MobSpawner"));
                    match new_type2.read_typed(&fix(tag)).result() {
                        Some((fixed, _)) => fixed,
                        None => input,
                    }
                },
            )
        },
    )
}

fn fix(input: Dynamic) -> Dynamic {
    if input.get("id").as_string_or("") != "MobSpawner" {
        return input;
    }
    let mut input = input;
    if let Some(entity_id) = input.get("EntityId").as_string().result() {
        let spawn_data = input
            .get("SpawnData")
            .result()
            .unwrap_or_else(|| input.empty_map());
        let spawn_data = spawn_data.set(
            "id",
            spawn_data.create_string(if entity_id.as_lossy_str().is_empty() {
                "Pig"
            } else {
                entity_id.as_lossy_str()
            }),
        );
        input = input.set("SpawnData", spawn_data);
        input = input.remove("EntityId");
    }
    if let Some(spawn_potentials) = input.get("SpawnPotentials").as_stream_opt().result() {
        let list = input.create_list(spawn_potentials.into_iter().map(|spawn_potential| {
            match spawn_potential.get("Type").as_string().result() {
                Some(ty) => {
                    let spawn_data = spawn_potential
                        .get("Properties")
                        .result()
                        .unwrap_or_else(|| spawn_potential.empty_map());
                    let spawn_data = spawn_data.set("id", spawn_potential.create_string(ty));
                    spawn_potential
                        .set("Entity", spawn_data)
                        .remove("Type")
                        .remove("Properties")
                }
                None => spawn_potential,
            }
        }));
        input = input.set("SpawnPotentials", list);
    }
    input
}
