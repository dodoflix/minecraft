//! `OptionsAmbientOcclusionFix`: converts the legacy `ao` option values to booleans.

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;

fn update_value(value: &str) -> &str {
    match value {
        "0" => "false",
        "1" | "2" => "true",
        _ => value,
    }
}

/// `new OptionsAmbientOcclusionFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new("OptionsAmbientOcclusionFix", output_schema, false, |df| {
        df.fix_type_everywhere_typed(
            "OptionsAmbientOcclusionFix",
            &df.input_schema().get_type(&References::OPTIONS),
            |input| {
                input.update_remainder(|tag| match tag.get("ao").as_string().result() {
                    Some(value) => tag.set(
                        "ao",
                        tag.create_string(value.map_lossless(|t| update_value(t).to_owned())),
                    ),
                    None => tag,
                })
            },
        )
    })
}
