//! `LegacyBlockPropertyRenameAndFix`: renames one property of one block's states and rewrites its value.

use crate::datafix::DataFix;
use crate::fixes::legacy_abstract_block_property_fix;
use crate::schema::Schema;

/// `new LegacyBlockPropertyRenameAndFix(outputSchema, name, blockId, oldPropertyName, newPropertyName, valueFixer)`.
pub fn new(
    output_schema: &Schema,
    name: &str,
    block_id: &str,
    old_property_name: &str,
    new_property_name: &str,
    value_fixer: impl Fn(String) -> String + Send + Sync + 'static,
) -> DataFix {
    let block_id = block_id.to_string();
    let (old_property_name, new_property_name) =
        (old_property_name.to_string(), new_property_name.to_string());
    legacy_abstract_block_property_fix::new(
        "LegacyBlockPropertyRenameAndFix",
        output_schema,
        name,
        move |id| id == block_id,
        move |_, properties| {
            properties.rename_and_fix_field(&old_property_name, &new_property_name, |dynamic| {
                let value = value_fixer(dynamic.as_string_or_lossy(""));
                dynamic.create_string(value)
            })
        },
    )
}
