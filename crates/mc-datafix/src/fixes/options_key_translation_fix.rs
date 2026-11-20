//! `OptionsKeyTranslationFix`: converts the `key.*` values of the `key_*` options to `key.keyboard.*` names (mouse
//! buttons and scancodes are kept).

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;
use mc_dfu::Dynamic;

/// `new OptionsKeyTranslationFix(outputSchema, changesType)`.
pub fn new(output_schema: &Schema, changes_type: bool) -> DataFix {
    DataFix::new(
        "OptionsKeyTranslationFix",
        output_schema,
        changes_type,
        |df| {
            df.fix_type_everywhere_typed(
                "OptionsKeyTranslationFix",
                &df.input_schema().get_type(&References::OPTIONS),
                |input| input.update_remainder(|tag| tag.update_map_values(fix_entry)),
            )
        },
    )
}

fn fix_entry((key, value): (Dynamic, Dynamic)) -> (Dynamic, Dynamic) {
    if key.as_string_or("").as_lossy_str().starts_with("key_") {
        let old_value = value.as_string_or("");
        if !old_value.as_lossy_str().starts_with("key.mouse")
            && !old_value.as_lossy_str().starts_with("scancode.")
        {
            let new_value = value.create_string(format!(
                "key.keyboard.{}",
                substring_from_key_prefix(old_value.as_lossy_str())
            ));
            return (key, new_value);
        }
    }
    (key, value)
}

/// `s.substring("key.".length())`: drops the first four UTF-16 units, throws like Java when there are fewer.
fn substring_from_key_prefix(s: &str) -> &str {
    const BEGIN: usize = "key.".len();
    let mut units = 0;
    for (index, c) in s.char_indices() {
        if units >= BEGIN {
            return &s[index..];
        }
        units += c.len_utf16();
    }
    if units < BEGIN {
        panic!(
            "java.lang.StringIndexOutOfBoundsException: Range [{BEGIN}, {units}) out of bounds for length {units}"
        );
    }
    ""
}
