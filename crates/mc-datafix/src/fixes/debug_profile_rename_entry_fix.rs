//! `DebugProfileRenameEntryFix`: renames one entry of the `custom` map of a debug profile.

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;

/// `new DebugProfileRenameEntryFix(outputSchema, oldName, newName)`.
pub fn new(output_schema: &Schema, old_name: &str, new_name: &str) -> DataFix {
    let (old_name, new_name) = (old_name.to_string(), new_name.to_string());
    DataFix::new(
        "DebugProfileRenameEntryFix",
        output_schema,
        false,
        move |df| {
            let (old_name, new_name) = (old_name.clone(), new_name.clone());
            df.fix_type_everywhere_typed(
                "DebugProfilePostEffectsFix",
                &df.input_schema().get_type(&References::DEBUG_PROFILE),
                move |input| {
                    input.update_remainder(|remainder| {
                        remainder
                            .update("custom", |custom| custom.rename_field(&old_name, &new_name))
                    })
                },
            )
        },
    )
}
