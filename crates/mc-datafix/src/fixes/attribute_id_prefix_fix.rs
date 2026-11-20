//! `AttributeIdPrefixFix`: strips the `generic.`, `horse.`, `player.` and `zombie.` prefixes of attribute ids.

use crate::datafix::DataFix;
use crate::fixes::attributes_rename_fix;
use crate::schema::Schema;
use crate::ty::ensure_namespaced;

const PREFIXES: [&str; 4] = ["generic.", "horse.", "player.", "zombie."];

/// `new AttributeIdPrefixFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    attributes_rename_fix::new_with_format(
        "AttributeIdPrefixFix",
        output_schema,
        "AttributeIdPrefixFix",
        replace_id,
        true,
    )
}

fn replace_id(id: &str) -> String {
    let namespaced_id = ensure_namespaced(id);
    for prefix in PREFIXES {
        let namespaced_prefix = ensure_namespaced(prefix);
        if let Some(rest) = namespaced_id.strip_prefix(namespaced_prefix.as_str()) {
            return format!("minecraft:{rest}");
        }
    }
    id.to_string()
}
