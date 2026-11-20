//! `BlendingDataNoValueHeightFix`: the heights of `blending_data` become floats, `Double.MAX_VALUE` the float maximum.

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;
use mc_dfu::Dynamic;

/// `new BlendingDataNoValueHeightFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new("BlendingDataNoValueHeightFix", output_schema, false, |df| {
        df.fix_type_everywhere_typed(
            "BlendingDataNoValueHeightFix",
            &df.input_schema().get_type(&References::CHUNK),
            |chunk| chunk.update_remainder(|chunk| fix(&chunk)),
        )
    })
}

fn fix(chunk: &Dynamic) -> Dynamic {
    chunk.update("blending_data", |blending_data| {
        blending_data.update("heights", |heights| {
            heights.create_list(heights.as_stream().iter().map(fix_height_value))
        })
    })
}

fn fix_height_value(height: &Dynamic) -> Dynamic {
    let value = height.as_double_or(f64::MAX);
    if value == f64::MAX {
        height.create_float(f32::MAX)
    } else {
        height.create_float(value as f32)
    }
}
