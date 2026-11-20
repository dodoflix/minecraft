//! `EntityRedundantChanceTagsFix`: removes all-zero `HandDropChances` and `ArmorDropChances` lists.

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;
use mc_dfu::{Codec, FLOAT, OptionalDynamic};
use std::sync::LazyLock;

static FLOAT_LIST_CODEC: LazyLock<Codec<Vec<f32>>> = LazyLock::new(|| FLOAT.list_of());

/// `new EntityRedundantChanceTagsFix(outputSchema, changesType)`.
pub fn new(output_schema: &Schema, changes_type: bool) -> DataFix {
    DataFix::new(
        "EntityRedundantChanceTagsFix",
        output_schema,
        changes_type,
        |df| {
            df.fix_type_everywhere_typed(
                "EntityRedundantChanceTagsFix",
                &df.input_schema().get_type(&References::ENTITY),
                |input| {
                    input.update_remainder(|mut tag| {
                        if is_zero_list(tag.get("HandDropChances"), 2) {
                            tag = tag.remove("HandDropChances");
                        }
                        if is_zero_list(tag.get("ArmorDropChances"), 4) {
                            tag = tag.remove("ArmorDropChances");
                        }
                        tag
                    })
                },
            )
        },
    )
}

fn is_zero_list(element: OptionalDynamic, size: usize) -> bool {
    element
        .flat_map(|d| d.read(&FLOAT_LIST_CODEC))
        .map(|floats| floats.len() == size && floats.iter().all(|f| *f == 0.0))
        .result()
        .unwrap_or(false)
}
