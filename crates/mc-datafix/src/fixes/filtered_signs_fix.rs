//! `FilteredSignsFix`: removes the filtered text of signs.

use crate::datafix::DataFix;
use crate::fixes::named_entity_write_read_fix;
use crate::fixes::references::References;
use crate::schema::Schema;

/// `new FilteredSignsFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    named_entity_write_read_fix::new(
        "FilteredSignsFix",
        output_schema,
        false,
        "Remove filtered text from signs",
        &References::BLOCK_ENTITY,
        "minecraft:sign",
        |input| {
            input
                .remove("FilteredText1")
                .remove("FilteredText2")
                .remove("FilteredText3")
                .remove("FilteredText4")
        },
    )
}
