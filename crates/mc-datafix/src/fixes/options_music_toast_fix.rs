//! `OptionsMusicToastFix`: renames `showNowPlayingToast` to `musicToast` (`"never"` for `"false"`, else
//! `"pause_and_toast"`).

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;

/// `new OptionsMusicToastFix(outputSchema, changesType)`.
pub fn new(output_schema: &Schema, changes_type: bool) -> DataFix {
    DataFix::new("OptionsMusicToastFix", output_schema, changes_type, |df| {
        df.fix_type_everywhere_typed(
            "OptionsMusicToastFix",
            &df.input_schema().get_type(&References::OPTIONS),
            |input| {
                input.update_remainder(|tag| {
                    tag.rename_and_fix_field("showNowPlayingToast", "musicToast", |old| {
                        tag.create_string(if old.as_string_or("false") == "false" {
                            "never"
                        } else {
                            "pause_and_toast"
                        })
                    })
                })
            },
        )
    })
}
