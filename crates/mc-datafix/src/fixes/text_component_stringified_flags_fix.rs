//! `TextComponentStringifiedFlagsFix`: turns the string style flags of a text component (`"bold": "true"`) into booleans.

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;
use crate::value::{Value, mismatch};
use mc_dfu::Dynamic;

/// `new TextComponentStringifiedFlagsFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new(
        "TextComponentStringifiedFlagsFix",
        output_schema,
        false,
        |df| {
            let text_component_type = df.input_schema().get_type(&References::TEXT_COMPONENT);
            df.fix_type_everywhere(
                "TextComponentStringyFlagsFix",
                &text_component_type,
                |_, named| {
                    named.map_second(|e1| {
                        map_right(e1, |p2| {
                            p2.map_second(|p3| {
                                p3.map_second(|p4| {
                                    p4.map_second(|remainder| {
                                        Value::Dynamic(
                                            remainder
                                                .into_dynamic()
                                                .update("bold", string_to_bool)
                                                .update("italic", string_to_bool)
                                                .update("underlined", string_to_bool)
                                                .update("strikethrough", string_to_bool)
                                                .update("obfuscated", string_to_bool),
                                        )
                                    })
                                })
                            })
                        })
                    })
                },
            )
        },
    )
}

/// `Either.mapRight`.
#[track_caller]
fn map_right(either: Value, f: impl FnOnce(Value) -> Value) -> Value {
    match either {
        Value::Right(b) => Value::right(f(*b)),
        left @ Value::Left(_) => left,
        other => mismatch("Either", &other),
    }
}

fn string_to_bool(input: Dynamic) -> Dynamic {
    match input.as_string().result() {
        // `Boolean.parseBoolean`
        Some(string) => input.create_boolean(string.as_lossy_str().eq_ignore_ascii_case("true")),
        None => input,
    }
}
