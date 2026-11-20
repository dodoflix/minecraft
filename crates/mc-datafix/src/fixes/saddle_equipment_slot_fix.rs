//! `SaddleEquipmentSlotFix`: moves the saddle of rideable entities (`SaddleItem` or the `Saddle` flag) into the `saddle`
//! equipment slot and gives it a drop chance of 2.

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::extra_data_fix_utils::{cast, patch_sub_type};
use crate::fixes::references::References;
use crate::schema::Schema;
use crate::ty::ensure_namespaced;
use crate::util::write_and_read_typed_or_throw;
use mc_dfu::Dynamic;

const ENTITIES_WITH_SADDLE_ITEM: [&str; 8] = [
    "minecraft:horse",
    "minecraft:skeleton_horse",
    "minecraft:zombie_horse",
    "minecraft:donkey",
    "minecraft:mule",
    "minecraft:camel",
    "minecraft:llama",
    "minecraft:trader_llama",
];
const ENTITIES_WITH_SADDLE_FLAG: [&str; 2] = ["minecraft:pig", "minecraft:strider"];

/// `new SaddleEquipmentSlotFix(outputSchema)` (`changesType` is `true`).
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new("SaddleEquipmentSlotFix", output_schema, true, |df| {
        let entity_id_type = df.input_schema().find_choice_type(&References::ENTITY);
        let entity_id_f = DSL::type_finder(entity_id_type.ty());
        let input_type = df.input_schema().get_type(&References::ENTITY);
        let output_type = df.output_schema().get_type(&References::ENTITY);
        let patched_input_type = patch_sub_type(&input_type, &input_type, &output_type);
        let output_type2 = output_type.clone();
        df.fix_type_everywhere_typed_new(
            "SaddleEquipmentSlotFix",
            &input_type,
            &output_type,
            move |input| {
                let entity_id = input
                    .get_optional(&entity_id_f)
                    .map(|v| ensure_namespaced(v.first().as_str()))
                    .unwrap_or_default();
                if ENTITIES_WITH_SADDLE_ITEM.contains(&entity_id.as_str()) {
                    let fixed_input = cast(&patched_input_type, input);
                    write_and_read_typed_or_throw(
                        &fixed_input,
                        &output_type2,
                        fix_entity_with_saddle_item,
                    )
                } else if ENTITIES_WITH_SADDLE_FLAG.contains(&entity_id.as_str()) {
                    let fixed_input = cast(&patched_input_type, input);
                    write_and_read_typed_or_throw(
                        &fixed_input,
                        &output_type2,
                        fix_entity_with_saddle_flag,
                    )
                } else {
                    cast(&output_type2, input)
                }
            },
        )
    })
}

fn fix_entity_with_saddle_item(input: Dynamic) -> Dynamic {
    if input.get("SaddleItem").result().is_none() {
        input
    } else {
        fix_drop_chances(input.rename_field("SaddleItem", "saddle"))
    }
}

fn fix_entity_with_saddle_flag(tag: Dynamic) -> Dynamic {
    let has_saddle = tag.get("Saddle").as_boolean_or(false);
    let tag = tag.remove("Saddle");
    if !has_saddle {
        tag
    } else {
        let saddle_item = tag
            .empty_map()
            .set("id", tag.create_string("minecraft:saddle"))
            .set("count", tag.create_int(1));
        fix_drop_chances(tag.set("saddle", saddle_item))
    }
}

fn fix_drop_chances(tag: Dynamic) -> Dynamic {
    let drop_chances = tag
        .get("drop_chances")
        .or_else_empty_map()
        .set("saddle", tag.create_float(2.0));
    tag.set("drop_chances", drop_chances)
}
