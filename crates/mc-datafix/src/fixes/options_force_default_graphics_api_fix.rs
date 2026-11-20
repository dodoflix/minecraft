//! `OptionsForceDefaultGraphicsApiFix`: sets `preferredGraphicsBackend` to `"default"`.

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;

/// `new OptionsForceDefaultGraphicsApiFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new(
        "OptionsForceDefaultGraphicsApiFix",
        output_schema,
        false,
        |df| {
            df.fix_type_everywhere_typed(
                "OptionsForceDefaultGraphicsApiFix",
                &df.input_schema().get_type(&References::OPTIONS),
                |input| {
                    input.update_remainder(|tag| {
                        tag.set("preferredGraphicsBackend", tag.create_string("default"))
                    })
                },
            )
        },
    )
}
