//! `OptionsLowerCaseLanguageFix`: lower-cases the `lang` option.

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;

/// `new OptionsLowerCaseLanguageFix(outputSchema, changesType)`.
pub fn new(output_schema: &Schema, changes_type: bool) -> DataFix {
    DataFix::new(
        "OptionsLowerCaseLanguageFix",
        output_schema,
        changes_type,
        |df| {
            df.fix_type_everywhere_typed(
                "OptionsLowerCaseLanguageFix",
                &df.input_schema().get_type(&References::OPTIONS),
                |input| {
                    input.update_remainder(|tag| {
                        let lang = tag.get("lang").as_string().result();
                        match lang {
                            Some(lang) => tag.set("lang", tag.create_string(lang.to_lowercase())),
                            None => tag,
                        }
                    })
                },
            )
        },
    )
}
