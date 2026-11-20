//! `BoatSplitFix`: splits `minecraft:boat` / `minecraft:chest_boat` into one entity per wood type by `Type`.

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::extra_data_fix_utils::cast;
use crate::fixes::references::References;
use crate::schema::Schema;
use crate::value::Value;

fn is_normal_boat(id: &str) -> bool {
    id == "minecraft:boat"
}

fn is_chest_boat(id: &str) -> bool {
    id == "minecraft:chest_boat"
}

fn is_any_boat(id: &str) -> bool {
    is_normal_boat(id) || is_chest_boat(id)
}

fn map_variant_to_normal_boat(id: &str) -> &'static str {
    match id {
        "spruce" => "minecraft:spruce_boat",
        "birch" => "minecraft:birch_boat",
        "jungle" => "minecraft:jungle_boat",
        "acacia" => "minecraft:acacia_boat",
        "cherry" => "minecraft:cherry_boat",
        "dark_oak" => "minecraft:dark_oak_boat",
        "mangrove" => "minecraft:mangrove_boat",
        "bamboo" => "minecraft:bamboo_raft",
        _ => "minecraft:oak_boat",
    }
}

fn map_variant_to_chest_boat(id: &str) -> &'static str {
    match id {
        "spruce" => "minecraft:spruce_chest_boat",
        "birch" => "minecraft:birch_chest_boat",
        "jungle" => "minecraft:jungle_chest_boat",
        "acacia" => "minecraft:acacia_chest_boat",
        "cherry" => "minecraft:cherry_chest_boat",
        "dark_oak" => "minecraft:dark_oak_chest_boat",
        "mangrove" => "minecraft:mangrove_chest_boat",
        "bamboo" => "minecraft:bamboo_chest_raft",
        _ => "minecraft:oak_chest_boat",
    }
}

/// `new BoatSplitFix(outputSchema)` (`changesType` is `true`).
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new("BoatSplitFix", output_schema, true, |df| {
        let id_f = DSL::field_finder("id", &DSL::namespaced_string());
        let old_type = df.input_schema().get_type(&References::ENTITY);
        let new_type = df.output_schema().get_type(&References::ENTITY);
        let new_type2 = new_type.clone();
        df.fix_type_everywhere_typed_new("BoatSplitFix", &old_type, &new_type, move |input| {
            let id = input.get_optional(&id_f).map(Value::into_string);
            match id {
                Some(id) if is_any_boat(&id) => {
                    let tag = input.get_remainder();
                    let maybe_boat_id = tag.get("Type").as_string().result();
                    let new_id = if is_chest_boat(&id) {
                        maybe_boat_id
                            .map(|v| map_variant_to_chest_boat(&v))
                            .unwrap_or("minecraft:oak_chest_boat")
                    } else {
                        maybe_boat_id
                            .map(|v| map_variant_to_normal_boat(&v))
                            .unwrap_or("minecraft:oak_boat")
                    };
                    cast(&new_type2, input)
                        .update_remainder(|remainder| remainder.remove("Type"))
                        .set(&id_f, Value::str(new_id))
                }
                _ => cast(&new_type2, input),
            }
        })
    })
}
