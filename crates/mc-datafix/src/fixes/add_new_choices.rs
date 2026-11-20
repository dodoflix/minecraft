//! `AddNewChoices`: carries a tagged choice type (`ENTITY`, `BLOCK_ENTITY`, ...) over to an output schema that
//! registers more choices; the data is unchanged but every id must exist in the output type.

use crate::datafix::DataFix;
use crate::dsl::TypeReference;
use crate::schema::Schema;

/// `new AddNewChoices(outputSchema, name, type)`.
pub fn new(output_schema: &Schema, name: &str, ty: &TypeReference) -> DataFix {
    let (name, ty) = (name.to_string(), *ty);
    DataFix::new("AddNewChoices", output_schema, true, move |df| {
        let input_type = df.input_schema().find_choice_type(&ty);
        let output_type = df.output_schema().find_choice_type(&ty);
        if !input_type
            .key_type()
            .equals_with(output_type.key_type(), false, true)
        {
            panic!("java.lang.IllegalStateException: Could not inject: key type is not the same");
        }
        let (output_choice, name2) = (output_type.clone(), name.clone());
        df.fix_type_everywhere_new(&name, input_type.ty(), output_type.ty(), move |_, input| {
            let id = input.first().as_lossy_str();
            if !output_choice.has_type(id) {
                panic!(
                    "java.lang.IllegalArgumentException: {name2}: Unknown type {id} in '{}'",
                    ty.type_name()
                );
            }
            input
        })
    })
}
