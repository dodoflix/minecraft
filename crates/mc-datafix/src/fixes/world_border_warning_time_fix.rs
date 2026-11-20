//! `WorldBorderWarningTimeFix`: the world border warning time of the saved data is stored in ticks instead of seconds.

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;

/// `new WorldBorderWarningTimeFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new("WorldBorderWarningTimeFix", output_schema, false, |df| {
        df.write_fix_and_read(
            "WorldBorderWarningTimeFix",
            &df.input_schema()
                .get_type(&References::SAVED_DATA_WORLD_BORDER),
            &df.output_schema()
                .get_type(&References::SAVED_DATA_WORLD_BORDER),
            |input| {
                input.update("data", |tag| {
                    tag.update("warning_time", |warning_time| {
                        tag.create_int(warning_time.as_int_or(15).wrapping_mul(20))
                    })
                })
            },
        )
    })
}
