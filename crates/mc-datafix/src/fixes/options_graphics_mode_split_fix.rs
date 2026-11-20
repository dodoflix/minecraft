//! `OptionsGraphicsModeSplitFix`: derives a new option from `graphicsMode` (fast `"0"`, fabulous `"2"`, anything else
//! fancy; fancy too when the option is absent).

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;

/// `new OptionsGraphicsModeSplitFix(outputSchema, newFieldName, valueIfFast, valueIfFancy, valueIfFabulous)`.
pub fn new(
    output_schema: &Schema,
    new_field_name: &str,
    value_if_fast: &str,
    value_if_fancy: &str,
    value_if_fabulous: &str,
) -> DataFix {
    let (new_field_name, value_if_fast, value_if_fancy, value_if_fabulous) = (
        new_field_name.to_string(),
        value_if_fast.to_string(),
        value_if_fancy.to_string(),
        value_if_fabulous.to_string(),
    );
    DataFix::new(
        "OptionsGraphicsModeSplitFix",
        output_schema,
        true,
        move |df| {
            let (new_field_name, value_if_fast, value_if_fancy, value_if_fabulous) = (
                new_field_name.clone(),
                value_if_fast.clone(),
                value_if_fancy.clone(),
                value_if_fabulous.clone(),
            );
            df.fix_type_everywhere_typed(
                &format!("graphicsMode split to {new_field_name}"),
                &df.input_schema().get_type(&References::OPTIONS),
                move |input| {
                    input.update_remainder(|tag| {
                        let value = match tag.get("graphicsMode").as_string().result() {
                            Some(mode) => match mode.as_lossy_str() {
                                "2" => &value_if_fabulous,
                                "0" => &value_if_fast,
                                _ => &value_if_fancy,
                            },
                            None => &value_if_fancy,
                        };
                        tag.set(&new_field_name, tag.create_string(value.as_str()))
                    })
                },
            )
        },
    )
}
