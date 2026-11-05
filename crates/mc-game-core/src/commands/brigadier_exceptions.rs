//! `net.minecraft.commands.BrigadierExceptions`: brigadier's built-in exceptions with translatable messages.

use super::message::{dynamic_translatable, message, simple_translatable};
use crate::translatable;
use mc_brigadier::exceptions::{BuiltInExceptionProvider, CommandSyntaxException};
use mc_brigadier::{Dynamic2CommandExceptionType, DynamicCommandExceptionType, SimpleCommandExceptionType};
use std::sync::Arc;

/// `Component.translatableEscape(key, min, found)` with the `Dynamic2CommandExceptionType(found, limit)` argument order.
fn bound<T: Into<mc_text::component::TranslatableArg> + Copy + Send + Sync + 'static>(key: &'static str) -> Dynamic2CommandExceptionType<T, T> {
    Dynamic2CommandExceptionType::new(move |found: T, limit: T| message(translatable!(key, limit.into(), found.into())))
}

pub struct BrigadierExceptions {
    double_too_small: Dynamic2CommandExceptionType<f64, f64>,
    double_too_big: Dynamic2CommandExceptionType<f64, f64>,
    float_too_small: Dynamic2CommandExceptionType<f32, f32>,
    float_too_big: Dynamic2CommandExceptionType<f32, f32>,
    integer_too_small: Dynamic2CommandExceptionType<i32, i32>,
    integer_too_big: Dynamic2CommandExceptionType<i32, i32>,
    long_too_small: Dynamic2CommandExceptionType<i64, i64>,
    long_too_big: Dynamic2CommandExceptionType<i64, i64>,
    literal_incorrect: DynamicCommandExceptionType<String>,
    reader_expected_start_of_quote: SimpleCommandExceptionType,
    reader_expected_end_of_quote: SimpleCommandExceptionType,
    reader_invalid_escape: DynamicCommandExceptionType<String>,
    reader_invalid_bool: DynamicCommandExceptionType<String>,
    reader_invalid_int: DynamicCommandExceptionType<String>,
    reader_expected_int: SimpleCommandExceptionType,
    reader_invalid_long: DynamicCommandExceptionType<String>,
    reader_expected_long: SimpleCommandExceptionType,
    reader_invalid_double: DynamicCommandExceptionType<String>,
    reader_expected_double: SimpleCommandExceptionType,
    reader_invalid_float: DynamicCommandExceptionType<String>,
    reader_expected_float: SimpleCommandExceptionType,
    reader_expected_bool: SimpleCommandExceptionType,
    reader_expected_symbol: DynamicCommandExceptionType<String>,
    dispatcher_unknown_command: SimpleCommandExceptionType,
    dispatcher_unknown_argument: SimpleCommandExceptionType,
    dispatcher_expected_argument_separator: SimpleCommandExceptionType,
    dispatcher_parse_exception: DynamicCommandExceptionType<String>,
}

impl BrigadierExceptions {
    pub fn new() -> Self {
        Self {
            double_too_small: bound("argument.double.low"),
            double_too_big: bound("argument.double.big"),
            float_too_small: bound("argument.float.low"),
            float_too_big: bound("argument.float.big"),
            integer_too_small: bound("argument.integer.low"),
            integer_too_big: bound("argument.integer.big"),
            long_too_small: bound("argument.long.low"),
            long_too_big: bound("argument.long.big"),
            literal_incorrect: dynamic_translatable("argument.literal.incorrect"),
            reader_expected_start_of_quote: simple_translatable("parsing.quote.expected.start"),
            reader_expected_end_of_quote: simple_translatable("parsing.quote.expected.end"),
            reader_invalid_escape: dynamic_translatable("parsing.quote.escape"),
            reader_invalid_bool: dynamic_translatable("parsing.bool.invalid"),
            reader_invalid_int: dynamic_translatable("parsing.int.invalid"),
            reader_expected_int: simple_translatable("parsing.int.expected"),
            reader_invalid_long: dynamic_translatable("parsing.long.invalid"),
            reader_expected_long: simple_translatable("parsing.long.expected"),
            reader_invalid_double: dynamic_translatable("parsing.double.invalid"),
            reader_expected_double: simple_translatable("parsing.double.expected"),
            reader_invalid_float: dynamic_translatable("parsing.float.invalid"),
            reader_expected_float: simple_translatable("parsing.float.expected"),
            reader_expected_bool: simple_translatable("parsing.bool.expected"),
            reader_expected_symbol: dynamic_translatable("parsing.expected"),
            dispatcher_unknown_command: simple_translatable("command.unknown.command"),
            dispatcher_unknown_argument: simple_translatable("command.unknown.argument"),
            dispatcher_expected_argument_separator: simple_translatable("command.expected.separator"),
            dispatcher_parse_exception: dynamic_translatable("command.exception"),
        }
    }

    /// Installs these exceptions as brigadier's `BUILT_IN_EXCEPTIONS` (vanilla does this in `CommandSyntaxException`'s
    /// static initializer replacement at bootstrap). Call once at server start.
    pub fn install() {
        CommandSyntaxException::set_built_in_exceptions(Arc::new(BrigadierExceptions::new()));
    }
}

impl Default for BrigadierExceptions {
    fn default() -> Self {
        Self::new()
    }
}

impl BuiltInExceptionProvider for BrigadierExceptions {
    fn double_too_low(&self) -> &Dynamic2CommandExceptionType<f64, f64> {
        &self.double_too_small
    }
    fn double_too_high(&self) -> &Dynamic2CommandExceptionType<f64, f64> {
        &self.double_too_big
    }
    fn float_too_low(&self) -> &Dynamic2CommandExceptionType<f32, f32> {
        &self.float_too_small
    }
    fn float_too_high(&self) -> &Dynamic2CommandExceptionType<f32, f32> {
        &self.float_too_big
    }
    fn integer_too_low(&self) -> &Dynamic2CommandExceptionType<i32, i32> {
        &self.integer_too_small
    }
    fn integer_too_high(&self) -> &Dynamic2CommandExceptionType<i32, i32> {
        &self.integer_too_big
    }
    fn long_too_low(&self) -> &Dynamic2CommandExceptionType<i64, i64> {
        &self.long_too_small
    }
    fn long_too_high(&self) -> &Dynamic2CommandExceptionType<i64, i64> {
        &self.long_too_big
    }
    fn literal_incorrect(&self) -> &DynamicCommandExceptionType<String> {
        &self.literal_incorrect
    }
    fn reader_expected_start_of_quote(&self) -> &SimpleCommandExceptionType {
        &self.reader_expected_start_of_quote
    }
    fn reader_expected_end_of_quote(&self) -> &SimpleCommandExceptionType {
        &self.reader_expected_end_of_quote
    }
    fn reader_invalid_escape(&self) -> &DynamicCommandExceptionType<String> {
        &self.reader_invalid_escape
    }
    fn reader_invalid_bool(&self) -> &DynamicCommandExceptionType<String> {
        &self.reader_invalid_bool
    }
    fn reader_invalid_int(&self) -> &DynamicCommandExceptionType<String> {
        &self.reader_invalid_int
    }
    fn reader_expected_int(&self) -> &SimpleCommandExceptionType {
        &self.reader_expected_int
    }
    fn reader_invalid_long(&self) -> &DynamicCommandExceptionType<String> {
        &self.reader_invalid_long
    }
    fn reader_expected_long(&self) -> &SimpleCommandExceptionType {
        &self.reader_expected_long
    }
    fn reader_invalid_double(&self) -> &DynamicCommandExceptionType<String> {
        &self.reader_invalid_double
    }
    fn reader_expected_double(&self) -> &SimpleCommandExceptionType {
        &self.reader_expected_double
    }
    fn reader_invalid_float(&self) -> &DynamicCommandExceptionType<String> {
        &self.reader_invalid_float
    }
    fn reader_expected_float(&self) -> &SimpleCommandExceptionType {
        &self.reader_expected_float
    }
    fn reader_expected_bool(&self) -> &SimpleCommandExceptionType {
        &self.reader_expected_bool
    }
    fn reader_expected_symbol(&self) -> &DynamicCommandExceptionType<String> {
        &self.reader_expected_symbol
    }
    fn dispatcher_unknown_command(&self) -> &SimpleCommandExceptionType {
        &self.dispatcher_unknown_command
    }
    fn dispatcher_unknown_argument(&self) -> &SimpleCommandExceptionType {
        &self.dispatcher_unknown_argument
    }
    fn dispatcher_expected_argument_separator(&self) -> &SimpleCommandExceptionType {
        &self.dispatcher_expected_argument_separator
    }
    fn dispatcher_parse_exception(&self) -> &DynamicCommandExceptionType<String> {
        &self.dispatcher_parse_exception
    }
}
