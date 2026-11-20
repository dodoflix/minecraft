//! `OptionsProgrammerArtFix`: fixes the `programer_art` typo in the resource pack lists of the options.

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;
use mc_dfu::Dynamic;

fn fix_list(entry: Dynamic) -> Dynamic {
    match entry.as_string().result() {
        Some(s) => entry.create_string(
            s.as_lossy_str()
                .replace("\"programer_art\"", "\"programmer_art\""),
        ),
        None => entry,
    }
}

/// `new OptionsProgrammerArtFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new("OptionsProgrammerArtFix", output_schema, false, |df| {
        df.fix_type_everywhere_typed(
            "OptionsProgrammerArtFix",
            &df.input_schema().get_type(&References::OPTIONS),
            |input| {
                input.update_remainder(|tag| {
                    tag.update("resourcePacks", fix_list)
                        .update("incompatibleResourcePacks", fix_list)
                })
            },
        )
    })
}
