//! Vanilla replaces `BUILT_IN_EXCEPTIONS` at startup with translatable messages; this runs in its own
//! test binary because the provider is process-global.

use std::sync::Arc;

use mc_brigadier::message::{Message, MessageRef};
use mc_brigadier::*;

#[derive(Debug)]
struct Translatable(&'static str, Vec<String>);

impl Message for Translatable {
    fn get_string(&self) -> String {
        format!("{}{:?}", self.0, self.1)
    }
}

fn tr(key: &'static str, args: Vec<String>) -> MessageRef {
    Arc::new(Translatable(key, args))
}

struct Translating(BuiltInExceptions);

macro_rules! delegate {
    ($($name:ident -> $ty:ty),+) => { $(fn $name(&self) -> &$ty { self.0.$name() })+ };
}

impl BuiltInExceptionProvider for Translating {
    delegate!(
        double_too_low -> Dynamic2CommandExceptionType<f64, f64>, double_too_high -> Dynamic2CommandExceptionType<f64, f64>,
        float_too_low -> Dynamic2CommandExceptionType<f32, f32>, float_too_high -> Dynamic2CommandExceptionType<f32, f32>,
        integer_too_low -> Dynamic2CommandExceptionType<i32, i32>, integer_too_high -> Dynamic2CommandExceptionType<i32, i32>,
        long_too_low -> Dynamic2CommandExceptionType<i64, i64>, long_too_high -> Dynamic2CommandExceptionType<i64, i64>,
        literal_incorrect -> DynamicCommandExceptionType<String>,
        reader_expected_start_of_quote -> SimpleCommandExceptionType, reader_expected_end_of_quote -> SimpleCommandExceptionType,
        reader_invalid_escape -> DynamicCommandExceptionType<String>, reader_invalid_bool -> DynamicCommandExceptionType<String>,
        reader_invalid_int -> DynamicCommandExceptionType<String>, reader_expected_int -> SimpleCommandExceptionType,
        reader_invalid_long -> DynamicCommandExceptionType<String>, reader_expected_long -> SimpleCommandExceptionType,
        reader_invalid_double -> DynamicCommandExceptionType<String>, reader_expected_double -> SimpleCommandExceptionType,
        reader_invalid_float -> DynamicCommandExceptionType<String>, reader_expected_float -> SimpleCommandExceptionType,
        reader_expected_bool -> SimpleCommandExceptionType, reader_expected_symbol -> DynamicCommandExceptionType<String>,
        dispatcher_unknown_command -> SimpleCommandExceptionType, dispatcher_unknown_argument -> SimpleCommandExceptionType,
        dispatcher_expected_argument_separator -> SimpleCommandExceptionType
    );

    fn dispatcher_parse_exception(&self) -> &DynamicCommandExceptionType<String> {
        // Not delegated on purpose: proves custom types plug in.
        static T: std::sync::LazyLock<DynamicCommandExceptionType<String>> =
            std::sync::LazyLock::new(|| {
                DynamicCommandExceptionType::new(|m| tr("command.exception", vec![m]))
            });
        &T
    }
}

#[test]
fn replaced_provider_is_used_and_message_is_downcastable() {
    CommandSyntaxException::set_built_in_exceptions(Arc::new(
        Translating(BuiltInExceptions::new()),
    ));
    let e = StringReader::new("x").read_int().unwrap_err();
    assert_eq!(e.get_message(), "Expected integer at position 0: <--[HERE]");

    struct Boom;
    impl ArgumentType<()> for Boom {
        fn parse(&self, _: &mut StringReader) -> Result<ParsedValue, CommandSyntaxException> {
            panic!("bad")
        }
    }
    let d = CommandDispatcher::<()>::new();
    d.register(literal("a").then(argument("b", Boom)));
    let e = d.execute("a b", ()).unwrap_err();
    let raw = e.get_raw_message();
    let t = raw
        .downcast_ref::<Translatable>()
        .expect("custom message type survives");
    assert_eq!(
        (t.0, t.1.clone()),
        ("command.exception", vec!["bad".to_owned()])
    );
}
