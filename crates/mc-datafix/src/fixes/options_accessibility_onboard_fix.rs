//! `OptionsAccessibilityOnboardFix`: marks the accessibility onboarding of an options file as not done.

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;

/// `new OptionsAccessibilityOnboardFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new(
        "OptionsAccessibilityOnboardFix",
        output_schema,
        false,
        |df| {
            df.fix_type_everywhere_typed(
                "OptionsAccessibilityOnboardFix",
                &df.input_schema().get_type(&References::OPTIONS),
                |typed| {
                    typed.update_remainder(|value| {
                        value.set("onboardAccessibility", value.create_string("false"))
                    })
                },
            )
        },
    )
}
