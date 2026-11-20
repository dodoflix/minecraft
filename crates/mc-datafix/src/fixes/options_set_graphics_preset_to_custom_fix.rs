//! `OptionsSetGraphicsPresetToCustomFix`: sets the `graphicsPreset` option to `"custom"`.

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;

/// `new OptionsSetGraphicsPresetToCustomFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new(
        "OptionsSetGraphicsPresetToCustomFix",
        output_schema,
        true,
        |df| {
            df.fix_type_everywhere_typed(
                "graphicsPreset set to \"custom\"",
                &df.input_schema().get_type(&References::OPTIONS),
                |input| {
                    input.update_remainder(|tag| {
                        tag.set("graphicsPreset", tag.create_string("custom"))
                    })
                },
            )
        },
    )
}
