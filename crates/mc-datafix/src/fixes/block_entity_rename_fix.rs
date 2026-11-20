//! `BlockEntityRenameFix`: renames block entity ids (the `BLOCK_ENTITY` choice changes).

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;
use crate::value::Value;
use std::sync::Arc;

/// `BlockEntityRenameFix.create(outputSchema, name, nameChangeLookup)`.
pub fn create(
    output_schema: &Schema,
    name: &str,
    name_change_lookup: impl Fn(&str) -> String + Send + Sync + 'static,
) -> DataFix {
    let name = name.to_string();
    let lookup = Arc::new(name_change_lookup);
    DataFix::new("BlockEntityRenameFix", output_schema, true, move |df| {
        let old_type = df
            .input_schema()
            .find_choice_type(&References::BLOCK_ENTITY);
        let new_type = df
            .output_schema()
            .find_choice_type(&References::BLOCK_ENTITY);
        let lookup = lookup.clone();
        df.fix_type_everywhere_new(&name, old_type.ty(), new_type.ty(), move |_, input| {
            input.map_first(|id| Value::Str(id.as_java_string().map_lossless(|t| lookup(t))))
        })
    })
}
