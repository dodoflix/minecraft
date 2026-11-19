//! `AddFlagIfNotPresentFix`: sets a boolean flag in the remainder of a type unless it is already there.

use crate::datafix::DataFix;
use crate::dsl::TypeReference;
use crate::schema::Schema;

/// `new AddFlagIfNotPresentFix(outputSchema, typeReference, flagKey, flagValue)`.
pub fn new(
    output_schema: &Schema,
    type_reference: &TypeReference,
    flag_key: &str,
    flag_value: bool,
) -> DataFix {
    let name = format!(
        "AddFlagIfNotPresentFix_{flag_key}={flag_value} for {}",
        output_schema.version_key()
    );
    let (ty, flag_key) = (*type_reference, flag_key.to_string());
    DataFix::new("AddFlagIfNotPresentFix", output_schema, true, move |df| {
        let flag_key = flag_key.clone();
        df.fix_type_everywhere_typed(&name, &df.input_schema().get_type(&ty), move |settings| {
            settings.update_remainder(|tag| {
                let flag = tag
                    .get(&flag_key)
                    .result()
                    .unwrap_or_else(|| tag.create_boolean(flag_value));
                tag.set(&flag_key, flag)
            })
        })
    })
}
