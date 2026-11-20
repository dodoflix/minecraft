//! `OptionsForceVBOFix`: forces the `useVbo` option to `"true"`.

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;

/// `new OptionsForceVBOFix(outputSchema, changesType)`.
pub fn new(output_schema: &Schema, changes_type: bool) -> DataFix {
    DataFix::new("OptionsForceVBOFix", output_schema, changes_type, |df| {
        df.fix_type_everywhere_typed(
            "OptionsForceVBOFix",
            &df.input_schema().get_type(&References::OPTIONS),
            |input| input.update_remainder(|tag| tag.set("useVbo", tag.create_string("true"))),
        )
    })
}
