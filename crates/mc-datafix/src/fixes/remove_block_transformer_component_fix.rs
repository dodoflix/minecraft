//! `RemoveBlockTransformerComponentFix`: removes the inline `minecraft:block_transformer` component.

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;

/// `new RemoveBlockTransformerComponentFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new(
        "RemoveBlockTransformerComponentFix",
        output_schema,
        false,
        |df| {
            df.fix_type_everywhere_typed(
                "Remove inline block transformer component",
                &df.input_schema().get_type(&References::DATA_COMPONENTS),
                |components| {
                    components.update_remainder(|remainder| {
                        remainder.remove("minecraft:block_transformer")
                    })
                },
            )
        },
    )
}
