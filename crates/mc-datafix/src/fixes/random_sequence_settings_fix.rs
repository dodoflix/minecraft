//! `RandomSequenceSettingsFix`: wraps the random sequences of the saved data in a `sequences` map.

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;

/// `new RandomSequenceSettingsFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new("RandomSequenceSettingsFix", output_schema, false, |df| {
        df.fix_type_everywhere_typed(
            "RandomSequenceSettingsFix",
            &df.input_schema()
                .get_type(&References::SAVED_DATA_RANDOM_SEQUENCES),
            |input| {
                input.update_remainder(|container| {
                    container.update("data", |tag| tag.empty_map().set("sequences", tag))
                })
            },
        )
    })
}
