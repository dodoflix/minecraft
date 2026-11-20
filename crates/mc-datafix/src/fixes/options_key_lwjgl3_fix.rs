//! `OptionsKeyLwjgl3Fix`: converts the numeric LWJGL 2 key codes of the `key_*` options to the LWJGL 3 key names.

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::fixes::tables::{OrderedMap, table};
use crate::schema::Schema;
use mc_core::math::java::parse_int;
use mc_dfu::Dynamic;
use std::sync::LazyLock;

pub const KEY_UNKNOWN: &str = "key.unknown";

static MAP: LazyLock<OrderedMap<i32, String>> = LazyLock::new(|| {
    table("options_key_lwjgl3_fix")
        .field("MAP")
        .int_string_map()
});

/// `new OptionsKeyLwjgl3Fix(outputSchema, changesType)`.
pub fn new(output_schema: &Schema, changes_type: bool) -> DataFix {
    DataFix::new("OptionsKeyLwjgl3Fix", output_schema, changes_type, |df| {
        df.fix_type_everywhere_typed(
            "OptionsKeyLwjgl3Fix",
            &df.input_schema().get_type(&References::OPTIONS),
            |input| input.update_remainder(|tag| tag.update_map_values(fix_entry)),
        )
    })
}

fn fix_entry((key, value): (Dynamic, Dynamic)) -> (Dynamic, Dynamic) {
    if !key.as_string_or("").starts_with("key_") {
        return (key, value);
    }
    let text = value.as_string_or("");
    let old_value = parse_int(&text)
        .unwrap_or_else(|| panic!("java.lang.NumberFormatException: For input string: \"{text}\""));
    let name = if old_value < 0 {
        let button = old_value + 100;
        match button {
            0 => "key.mouse.left".to_string(),
            1 => "key.mouse.right".to_string(),
            2 => "key.mouse.middle".to_string(),
            _ => format!("key.mouse.{}", button + 1),
        }
    } else {
        MAP.get(&old_value)
            .map_or(KEY_UNKNOWN, String::as_str)
            .to_string()
    };
    let value = value.create_string(name);
    (key, value)
}
