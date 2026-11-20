//! `net.minecraft.util.datafix.fixes.SpawnerDataFix`.

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;
use crate::ty::Type;
use crate::typed::Typed;
use crate::value::Value;
use mc_dfu::Dynamic;

/// `new SpawnerDataFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new("SpawnerDataFix", output_schema, true, |df| {
        let old_type = df.input_schema().get_type(&References::UNTAGGED_SPAWNER);
        let new_type = df.output_schema().get_type(&References::UNTAGGED_SPAWNER);
        let spawn_data_finder = old_type.find_field("SpawnData");
        let new_spawn_data_type = new_type.find_field("SpawnData").ty().clone();
        let spawn_potentials_finder = old_type.find_field("SpawnPotentials");
        let new_spawn_potentials_type = new_type.find_field("SpawnPotentials").ty().clone();
        df.fix_type_everywhere_typed_new(
            "Fix mob spawner data structure",
            &old_type,
            &new_type,
            move |spawner| {
                spawner
                    .update_typed_as(&spawn_data_finder, &new_spawn_data_type, |spawn_data| {
                        wrap_entity_to_spawn_data(&new_spawn_data_type, spawn_data)
                    })
                    .update_typed_as(
                        &spawn_potentials_finder,
                        &new_spawn_potentials_type,
                        |spawn_potentials| {
                            wrap_spawn_potentials_to_weighted_entries(
                                &new_spawn_potentials_type,
                                spawn_potentials,
                            )
                        },
                    )
            },
        )
    })
}

fn wrap_entity_to_spawn_data(new_type: &Type, spawn_data: Typed) -> Typed {
    let ops = spawn_data.ops;
    Typed::new(
        new_type.clone(),
        ops,
        Value::pair(spawn_data.value, Value::Dynamic(Dynamic::empty(ops))),
    )
}

fn wrap_spawn_potentials_to_weighted_entries(new_type: &Type, spawn_potentials: Typed) -> Typed {
    let ops = spawn_potentials.ops;
    let wrapped_entries = spawn_potentials
        .value
        .into_list()
        .into_iter()
        .map(|o| {
            let (first, second) = o.into_pair();
            let second = second.into_dynamic();
            let weight = second.get("Weight").as_int_or(1);
            let new_entry_remainder = Dynamic::empty(ops);
            let new_entry_remainder =
                new_entry_remainder.set("weight", new_entry_remainder.create_int(weight));
            let new_inner_remainder = second.remove("Weight").remove("Entity");
            Value::pair(
                Value::pair(first, Value::Dynamic(new_inner_remainder)),
                Value::Dynamic(new_entry_remainder),
            )
        })
        .collect();
    Typed::new(new_type.clone(), ops, Value::List(wrapped_entries))
}
