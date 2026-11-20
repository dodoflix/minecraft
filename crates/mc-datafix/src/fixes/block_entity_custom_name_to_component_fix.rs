//! `BlockEntityCustomNameToComponentFix`: converts the plain string `CustomName` of nameable block entities to a text
//! component.

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::extra_data_fix_utils::{cast, patch_sub_type};
use crate::fixes::references::References;
use crate::legacy_component_data_fix_utils::create_plain_text_component;
use crate::schema::Schema;
use crate::util::write_and_read_typed_or_throw;
use mc_dfu::Dynamic;

const NAMEABLE_BLOCK_ENTITIES: [&str; 11] = [
    "minecraft:beacon",
    "minecraft:banner",
    "minecraft:brewing_stand",
    "minecraft:chest",
    "minecraft:trapped_chest",
    "minecraft:dispenser",
    "minecraft:dropper",
    "minecraft:enchanting_table",
    "minecraft:furnace",
    "minecraft:hopper",
    "minecraft:shulker_box",
];

/// `new BlockEntityCustomNameToComponentFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new(
        "BlockEntityCustomNameToComponentFix",
        output_schema,
        true,
        |df| {
            let id_finder = DSL::field_finder("id", &DSL::namespaced_string());
            let input_type = df.input_schema().get_type(&References::BLOCK_ENTITY);
            let output_type = df.output_schema().get_type(&References::BLOCK_ENTITY);
            let patched_input_type = patch_sub_type(&input_type, &input_type, &output_type);
            let new_type = output_type.clone();
            df.fix_type_everywhere_typed_new(
                "BlockEntityCustomNameToComponentFix",
                &input_type,
                &new_type,
                move |input| {
                    let id = input.get_optional(&id_finder);
                    if id.is_some_and(|id| !NAMEABLE_BLOCK_ENTITIES.contains(&id.as_lossy_str())) {
                        cast(&output_type, input)
                    } else {
                        write_and_read_typed_or_throw(
                            &cast(&patched_input_type, input),
                            &output_type,
                            fix_tag_custom_name,
                        )
                    }
                },
            )
        },
    )
}

/// `fixTagCustomName(tag)`: removes an empty `CustomName`, turns any other into a plain text component.
pub fn fix_tag_custom_name(tag: Dynamic) -> Dynamic {
    let name = tag.get("CustomName").as_string_or("");
    if name.as_lossy_str().is_empty() {
        tag.remove("CustomName")
    } else {
        tag.set("CustomName", create_plain_text_component(tag.ops(), name))
    }
}
