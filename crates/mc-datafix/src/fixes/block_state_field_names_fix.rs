//! `BlockStateFieldNamesFix`: block states use the lower-case field names `id` and `properties`.

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;

/// `new BlockStateFieldNamesFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new("BlockStateFieldNamesFix", output_schema, false, |df| {
        df.fix_type_everywhere_typed(
            "BlockStateFieldNamesFix",
            &df.input_schema().get_type(&References::BLOCK_STATE),
            |input| {
                input.update_remainder(|remainder| {
                    remainder
                        .rename_field("Name", "id")
                        .rename_field("Properties", "properties")
                })
            },
        )
    })
}
