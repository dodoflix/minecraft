//! `InvalidLockComponentFix`: drops `minecraft:lock` predicates that only match an empty custom name.

use crate::datafix::DataFix;
use crate::fixes::data_component_remainder_fix;
use crate::schema::Schema;
use mc_dfu::Dynamic;

/// `new InvalidLockComponentFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    data_component_remainder_fix::new(
        "InvalidLockComponentFix",
        output_schema,
        "InvalidLockComponentPredicateFix",
        "minecraft:lock",
        fix_lock,
    )
}

/// `InvalidLockComponentFix.fixLock(input)` (`None` = Java `null`).
pub fn fix_lock(input: Dynamic) -> Option<Dynamic> {
    if is_broken_lock(&input) {
        None
    } else {
        Some(input)
    }
}

fn is_broken_lock(input: &Dynamic) -> bool {
    is_map_with_one_field(input, "components", |components| {
        is_map_with_one_field(components, "minecraft:custom_name", |custom_name| {
            custom_name.as_string_lossy().result().as_deref() == Some("\"\"")
        })
    })
}

fn is_map_with_one_field(
    input: &Dynamic,
    field_name: &str,
    predicate: impl FnOnce(&Dynamic) -> bool,
) -> bool {
    match input.get_map_values().result() {
        Some(map) if map.len() == 1 => input
            .get(field_name)
            .result()
            .is_some_and(|field| predicate(&field)),
        _ => false,
    }
}
