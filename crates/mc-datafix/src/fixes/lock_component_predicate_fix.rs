//! `LockComponentPredicateFix`: the string `minecraft:lock` component becomes an item predicate matching the custom
//! name.

use crate::datafix::DataFix;
use crate::fixes::data_component_remainder_fix;
use crate::schema::Schema;
use mc_dfu::Dynamic;

/// `LockComponentPredicateFix.ESCAPER.escape(name)`: escapes `"` and `\`.
fn escape(name: &str) -> String {
    let mut escaped = String::with_capacity(name.len());
    for c in name.chars() {
        if c == '"' || c == '\\' {
            escaped.push('\\');
        }
        escaped.push(c);
    }
    escaped
}

/// `new LockComponentPredicateFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    data_component_remainder_fix::new(
        "LockComponentPredicateFix",
        output_schema,
        "LockComponentPredicateFix",
        "minecraft:lock",
        fix_lock,
    )
}

/// `LockComponentPredicateFix.fixLock(input)` (`None` = Java `null`).
pub fn fix_lock(input: Dynamic) -> Option<Dynamic> {
    let name = input.as_string().result()?;
    if name.as_lossy_str().is_empty() {
        return None;
    }
    let name_component = input.create_string(format!("\"{}\"", escape(name.as_lossy_str())));
    let components = input
        .empty_map()
        .set("minecraft:custom_name", name_component);
    Some(input.empty_map().set("components", components))
}
