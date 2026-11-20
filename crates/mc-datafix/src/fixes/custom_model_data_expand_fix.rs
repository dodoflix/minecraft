//! `CustomModelDataExpandFix`: `minecraft:custom_model_data` changes from a number to a map with a `floats` list.

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;

/// `new CustomModelDataExpandFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new("CustomModelDataExpandFix", output_schema, false, |df| {
        df.fix_type_everywhere_typed(
            "Custom Model Data expansion",
            &df.input_schema().get_type(&References::DATA_COMPONENTS),
            |component| {
                component.update_remainder(|tag| {
                    tag.update("minecraft:custom_model_data", |cmd| {
                        let current_value = cmd.as_float_or(0.0);
                        cmd.create_map([(
                            cmd.create_string("floats"),
                            cmd.create_list([cmd.create_float(current_value)]),
                        )])
                    })
                })
            },
        )
    })
}
