//! `OptionsRenameFieldFix`: renames one field of the options.

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;

/// `new OptionsRenameFieldFix(outputSchema, changesType, fixName, fieldFrom, fieldTo)`.
pub fn new(
    output_schema: &Schema,
    changes_type: bool,
    name: &str,
    field_from: &str,
    field_to: &str,
) -> DataFix {
    let (name, field_from, field_to) = (
        name.to_string(),
        field_from.to_string(),
        field_to.to_string(),
    );
    DataFix::new(
        "OptionsRenameFieldFix",
        output_schema,
        changes_type,
        move |df| {
            let (field_from, field_to) = (field_from.clone(), field_to.clone());
            df.fix_type_everywhere_typed(
                &name,
                &df.input_schema().get_type(&References::OPTIONS),
                move |input| input.update_remainder(|tag| tag.rename_field(&field_from, &field_to)),
            )
        },
    )
}
