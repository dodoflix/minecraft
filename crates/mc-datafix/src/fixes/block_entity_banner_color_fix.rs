//! `BlockEntityBannerColorFix`: inverts the dye colors (`15 - color`) of banner base and patterns.

use crate::datafix::DataFix;
use crate::fixes::named_entity_fix;
use crate::fixes::references::References;
use crate::schema::Schema;
use mc_dfu::Dynamic;

/// `new BlockEntityBannerColorFix(outputSchema, changesType)`.
pub fn new(output_schema: &Schema, changes_type: bool) -> DataFix {
    named_entity_fix::new(
        "BlockEntityBannerColorFix",
        output_schema,
        changes_type,
        "BlockEntityBannerColorFix",
        &References::BLOCK_ENTITY,
        "minecraft:banner",
        |entity| entity.update_remainder(fix_tag),
    )
}

/// `fixTag(input)`.
pub fn fix_tag(input: Dynamic) -> Dynamic {
    let input = input.update("Base", |base| {
        base.create_int(15i32.wrapping_sub(base.as_int_or(0)))
    });
    input.update("Patterns", |list| match list.as_stream_opt().result() {
        Some(stream) => list.create_list(stream.into_iter().map(|pattern| {
            pattern.update("Color", |color| {
                color.create_int(15i32.wrapping_sub(color.as_int_or(0)))
            })
        })),
        None => list,
    })
}
