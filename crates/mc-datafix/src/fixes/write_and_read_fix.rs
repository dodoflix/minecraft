//! `WriteAndReadFix`: re-reads a whole type through its `Dynamic` form (the type changes).

use crate::datafix::DataFix;
use crate::dsl::TypeReference;
use crate::schema::Schema;

/// `new WriteAndReadFix(outputSchema, name, type)`.
pub fn new(output_schema: &Schema, name: &str, ty: &TypeReference) -> DataFix {
    let (name, ty) = (name.to_string(), *ty);
    DataFix::new("WriteAndReadFix", output_schema, true, move |df| {
        df.write_and_read(
            &name,
            &df.input_schema().get_type(&ty),
            &df.output_schema().get_type(&ty),
        )
    })
}
