//! `DebugProfileOverlayReferenceFix`: debug profile entries valued `inF3` become `inOverlay`.

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;

/// `new DebugProfileOverlayReferenceFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new(
        "DebugProfileOverlayReferenceFix",
        output_schema,
        false,
        |df| {
            df.fix_type_everywhere_typed(
                "DebugProfileOverlayReferenceFix",
                &df.input_schema().get_type(&References::DEBUG_PROFILE),
                |typed| {
                    typed.update_remainder(|file| {
                        file.update("custom", |custom| {
                            custom.update_map_values(|(key, value)| {
                                let value = if value.as_string_or("") == "inF3" {
                                    value.create_string("inOverlay")
                                } else {
                                    value
                                };
                                (key, value)
                            })
                        })
                    })
                },
            )
        },
    )
}
