//! `OptionsFancyGraphicsToGraphicsModeFix`: renames the `fancyGraphics` option to `graphicsMode` (`"1"` when it reads
//! as `"true"`, `"0"` otherwise).

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;
use mc_dfu::Dynamic;

/// `new OptionsFancyGraphicsToGraphicsModeFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new(
        "OptionsFancyGraphicsToGraphicsModeFix",
        output_schema,
        true,
        |df| {
            df.fix_type_everywhere_typed(
                "fancyGraphics to graphicsMode",
                &df.input_schema().get_type(&References::OPTIONS),
                |input| {
                    input.update_remainder(|tag| {
                        tag.rename_and_fix_field("fancyGraphics", "graphicsMode", fix_graphics_mode)
                    })
                },
            )
        },
    )
}

fn fix_graphics_mode(field: Dynamic) -> Dynamic {
    if field.as_string_or("true") == "true" {
        field.create_string("1")
    } else {
        field.create_string("0")
    }
}
