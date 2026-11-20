//! `BlockEntityBlockStateFix`: replaces the `blockId` / `blockData` of piston block entities by a `blockState`.

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::fixes::block_state_data;
use crate::fixes::named_entity_fix;
use crate::fixes::references::References;
use crate::schema::Schema;
use mc_dfu::{Dyn, Dynamic, NBT};

/// `new BlockEntityBlockStateFix(outputSchema, changesType)`.
pub fn new(output_schema: &Schema, changes_type: bool) -> DataFix {
    let output_schema2 = output_schema.clone();
    named_entity_fix::new(
        "BlockEntityBlockStateFix",
        output_schema,
        changes_type,
        "BlockEntityBlockStateFix",
        &References::BLOCK_ENTITY,
        "minecraft:piston",
        move |entity| {
            let new_type =
                output_schema2.get_choice_type(&References::BLOCK_ENTITY, "minecraft:piston");
            let block_state_type = new_type.find_field_type("blockState");
            let block_state_f = DSL::field_finder("blockState", &block_state_type);
            let tag = entity.get(&DSL::remainder_finder()).into_dynamic();
            let block = tag.get("blockId").as_int_or(0);
            let tag = tag.remove("blockId");
            let data = tag.get("blockData").as_int_or(0) & 15;
            let tag = tag.remove("blockData");
            let block_state_tag =
                Dynamic::new(NBT, Dyn::from(block_state_data::get_tag(block << 4 | data)));
            let output = new_type.point_typed(entity.get_ops()).unwrap_or_else(|| {
                panic!("java.lang.IllegalStateException: Could not create new piston block entity.")
            });
            let block_state = block_state_type
                .read_typed(&block_state_tag)
                .result()
                .unwrap_or_else(|| {
                    panic!(
                        "java.lang.IllegalStateException: Could not parse newly created block state tag."
                    )
                })
                .0;
            output
                .set_remainder(tag)
                .set_typed(&block_state_f, block_state)
        },
    )
}
