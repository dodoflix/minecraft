//! `SwingAnimationComponentSplitFix`: `minecraft:swing_animation` becomes the attack and interact animation components.

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;

/// `new SwingAnimationComponentSplitFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new(
        "SwingAnimationComponentSplitFix",
        output_schema,
        false,
        |df| {
            df.fix_type_everywhere_typed(
                "SwingAnimationComponentSplitFix",
                &df.input_schema().get_type(&References::DATA_COMPONENTS),
                |input| {
                    input.update_remainder(|tag| {
                        match tag.get("minecraft:swing_animation").result() {
                            Some(swing_animation) => tag
                                .remove("minecraft:swing_animation")
                                .set("minecraft:attack_animation", swing_animation.clone())
                                .set("minecraft:interact_animation", swing_animation),
                            None => tag,
                        }
                    })
                },
            )
        },
    )
}
