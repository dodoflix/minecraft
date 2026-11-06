//! `com.mojang.brigadier.exceptions`.

use std::any::Any;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, RwLock};

use crate::java_fmt::{double_to_string, float_to_string};
use crate::message::{MessageRef, literal};
use crate::string_reader::ImmutableStringReader;
use crate::utf16::{len16, substring};

/// Identity of an exception type (`CommandExceptionType`): two exceptions have the same type iff
/// they were created by the same `*CommandExceptionType` instance.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct CommandExceptionType(u64);

impl CommandExceptionType {
    pub fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        Self(NEXT.fetch_add(1, Ordering::Relaxed))
    }
}

impl Default for CommandExceptionType {
    fn default() -> Self {
        Self::new()
    }
}

/// `com.mojang.brigadier.exceptions.CommandSyntaxException`.
#[derive(Clone, Debug)]
pub struct CommandSyntaxException {
    type_: CommandExceptionType,
    message: MessageRef,
    /// `(input, cursor)`; vanilla's `input == null / cursor == -1` is `None`.
    context: Option<(String, usize)>,
}

impl CommandSyntaxException {
    pub const CONTEXT_AMOUNT: usize = 10;

    pub fn new(type_: CommandExceptionType, message: MessageRef) -> Self {
        Self {
            type_,
            message,
            context: None,
        }
    }

    pub fn with_context(
        type_: CommandExceptionType,
        message: MessageRef,
        input: String,
        cursor: usize,
    ) -> Self {
        Self {
            type_,
            message,
            context: Some((input, cursor)),
        }
    }

    /// `BUILT_IN_EXCEPTIONS`.
    pub fn built_in_exceptions() -> Arc<dyn BuiltInExceptionProvider> {
        built_in_exceptions()
    }

    /// Replaces `BUILT_IN_EXCEPTIONS` (vanilla installs one with translatable messages at startup).
    pub fn set_built_in_exceptions(provider: Arc<dyn BuiltInExceptionProvider>) {
        set_built_in_exceptions(provider)
    }

    /// `getMessage()`: message plus " at position N: ...<--[HERE]" when there is a context.
    pub fn get_message(&self) -> String {
        let mut message = self.message.get_string();
        if let Some(context) = self.get_context() {
            message.push_str(&format!(
                " at position {}: {}",
                self.context.as_ref().map_or(0, |c| c.1),
                context
            ));
        }
        message
    }

    pub fn get_raw_message(&self) -> &MessageRef {
        &self.message
    }

    pub fn get_context(&self) -> Option<String> {
        let (input, cursor) = self.context.as_ref()?;
        let cursor = (*cursor).min(len16(input));
        let mut out = String::new();
        if cursor > Self::CONTEXT_AMOUNT {
            out.push_str("...");
        }
        out.push_str(&substring(
            input,
            cursor.saturating_sub(Self::CONTEXT_AMOUNT),
            cursor,
        ));
        out.push_str("<--[HERE]");
        Some(out)
    }

    pub fn get_type(&self) -> CommandExceptionType {
        self.type_
    }

    pub fn get_input(&self) -> Option<&str> {
        self.context.as_ref().map(|c| c.0.as_str())
    }

    pub fn get_cursor(&self) -> Option<usize> {
        self.context.as_ref().map(|c| c.1)
    }
}

impl fmt::Display for CommandSyntaxException {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.get_message())
    }
}

impl std::error::Error for CommandSyntaxException {}

fn with_reader(
    type_: CommandExceptionType,
    message: MessageRef,
    reader: &dyn ImmutableStringReader,
) -> CommandSyntaxException {
    CommandSyntaxException::with_context(
        type_,
        message,
        reader.get_string().to_owned(),
        reader.get_cursor(),
    )
}

/// `SimpleCommandExceptionType`.
pub struct SimpleCommandExceptionType {
    id: CommandExceptionType,
    message: MessageRef,
}

impl SimpleCommandExceptionType {
    pub fn new(message: MessageRef) -> Self {
        Self {
            id: CommandExceptionType::new(),
            message,
        }
    }

    pub fn create(&self) -> CommandSyntaxException {
        CommandSyntaxException::new(self.id, self.message.clone())
    }

    pub fn create_with_context(
        &self,
        reader: &dyn ImmutableStringReader,
    ) -> CommandSyntaxException {
        with_reader(self.id, self.message.clone(), reader)
    }
}

impl fmt::Display for SimpleCommandExceptionType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message.get_string())
    }
}

macro_rules! dynamic_exception_type {
    ($name:ident<$($g:ident),+>, $($a:ident: $t:ident),+) => {
        /// `DynamicNCommandExceptionType` with statically typed arguments.
        pub struct $name<$($g),+> {
            id: CommandExceptionType,
            function: Box<dyn Fn($($g),+) -> MessageRef + Send + Sync>,
        }

        impl<$($g),+> $name<$($g),+> {
            pub fn new(function: impl Fn($($g),+) -> MessageRef + Send + Sync + 'static) -> Self {
                Self { id: CommandExceptionType::new(), function: Box::new(function) }
            }

            pub fn create(&self, $($a: $t),+) -> CommandSyntaxException {
                CommandSyntaxException::new(self.id, (self.function)($($a),+))
            }

            pub fn create_with_context(&self, reader: &dyn ImmutableStringReader, $($a: $t),+) -> CommandSyntaxException {
                with_reader(self.id, (self.function)($($a),+), reader)
            }
        }
    };
}

dynamic_exception_type!(DynamicCommandExceptionType<A>, a: A);
dynamic_exception_type!(Dynamic2CommandExceptionType<A, B>, a: A, b: B);
dynamic_exception_type!(Dynamic3CommandExceptionType<A, B, C>, a: A, b: B, c: C);
dynamic_exception_type!(Dynamic4CommandExceptionType<A, B, C, D>, a: A, b: B, c: C, d: D);

type DynamicNFunction = Box<dyn Fn(&[&dyn Any]) -> MessageRef + Send + Sync>;

/// `DynamicNCommandExceptionType`: arguments are type-erased.
pub struct DynamicNCommandExceptionType {
    id: CommandExceptionType,
    function: DynamicNFunction,
}

impl DynamicNCommandExceptionType {
    pub fn new(function: impl Fn(&[&dyn Any]) -> MessageRef + Send + Sync + 'static) -> Self {
        Self {
            id: CommandExceptionType::new(),
            function: Box::new(function),
        }
    }

    pub fn create(&self, args: &[&dyn Any]) -> CommandSyntaxException {
        CommandSyntaxException::new(self.id, (self.function)(args))
    }

    pub fn create_with_context(
        &self,
        reader: &dyn ImmutableStringReader,
        args: &[&dyn Any],
    ) -> CommandSyntaxException {
        with_reader(self.id, (self.function)(args), reader)
    }
}

/// `BuiltInExceptionProvider`. Argument order matches vanilla `create(found, limit)`; values are
/// passed typed so a translating provider can build components from them.
pub trait BuiltInExceptionProvider: Send + Sync {
    fn double_too_low(&self) -> &Dynamic2CommandExceptionType<f64, f64>;
    fn double_too_high(&self) -> &Dynamic2CommandExceptionType<f64, f64>;
    fn float_too_low(&self) -> &Dynamic2CommandExceptionType<f32, f32>;
    fn float_too_high(&self) -> &Dynamic2CommandExceptionType<f32, f32>;
    fn integer_too_low(&self) -> &Dynamic2CommandExceptionType<i32, i32>;
    fn integer_too_high(&self) -> &Dynamic2CommandExceptionType<i32, i32>;
    fn long_too_low(&self) -> &Dynamic2CommandExceptionType<i64, i64>;
    fn long_too_high(&self) -> &Dynamic2CommandExceptionType<i64, i64>;
    fn literal_incorrect(&self) -> &DynamicCommandExceptionType<String>;
    fn reader_expected_start_of_quote(&self) -> &SimpleCommandExceptionType;
    fn reader_expected_end_of_quote(&self) -> &SimpleCommandExceptionType;
    fn reader_invalid_escape(&self) -> &DynamicCommandExceptionType<String>;
    fn reader_invalid_bool(&self) -> &DynamicCommandExceptionType<String>;
    fn reader_invalid_int(&self) -> &DynamicCommandExceptionType<String>;
    fn reader_expected_int(&self) -> &SimpleCommandExceptionType;
    fn reader_invalid_long(&self) -> &DynamicCommandExceptionType<String>;
    fn reader_expected_long(&self) -> &SimpleCommandExceptionType;
    fn reader_invalid_double(&self) -> &DynamicCommandExceptionType<String>;
    fn reader_expected_double(&self) -> &SimpleCommandExceptionType;
    fn reader_invalid_float(&self) -> &DynamicCommandExceptionType<String>;
    fn reader_expected_float(&self) -> &SimpleCommandExceptionType;
    fn reader_expected_bool(&self) -> &SimpleCommandExceptionType;
    fn reader_expected_symbol(&self) -> &DynamicCommandExceptionType<String>;
    fn dispatcher_unknown_command(&self) -> &SimpleCommandExceptionType;
    fn dispatcher_unknown_argument(&self) -> &SimpleCommandExceptionType;
    fn dispatcher_expected_argument_separator(&self) -> &SimpleCommandExceptionType;
    /// Argument is the cause's message (`RuntimeException.getMessage()`, empty when null).
    fn dispatcher_parse_exception(&self) -> &DynamicCommandExceptionType<String>;
}

macro_rules! built_in_exceptions_struct {
    ($($field:ident: $ty:ty = $init:expr;)+) => {
        /// `BuiltInExceptions`: brigadier's own English literal messages.
        pub struct BuiltInExceptions {
            $($field: $ty,)+
        }

        impl BuiltInExceptions {
            pub fn new() -> Self {
                Self { $($field: $init,)+ }
            }
        }

        impl BuiltInExceptionProvider for BuiltInExceptions {
            $(fn $field(&self) -> &$ty { &self.$field })+
        }
    };
}

fn simple(s: &str) -> SimpleCommandExceptionType {
    SimpleCommandExceptionType::new(literal(s))
}

fn dynamic(
    f: impl Fn(String) -> String + Send + Sync + 'static,
) -> DynamicCommandExceptionType<String> {
    DynamicCommandExceptionType::new(move |a| literal(f(a)))
}

built_in_exceptions_struct! {
    double_too_low: Dynamic2CommandExceptionType<f64, f64> = Dynamic2CommandExceptionType::new(|found, min| literal(format!("Double must not be less than {}, found {}", double_to_string(min), double_to_string(found))));
    double_too_high: Dynamic2CommandExceptionType<f64, f64> = Dynamic2CommandExceptionType::new(|found, max| literal(format!("Double must not be more than {}, found {}", double_to_string(max), double_to_string(found))));
    float_too_low: Dynamic2CommandExceptionType<f32, f32> = Dynamic2CommandExceptionType::new(|found, min| literal(format!("Float must not be less than {}, found {}", float_to_string(min), float_to_string(found))));
    float_too_high: Dynamic2CommandExceptionType<f32, f32> = Dynamic2CommandExceptionType::new(|found, max| literal(format!("Float must not be more than {}, found {}", float_to_string(max), float_to_string(found))));
    integer_too_low: Dynamic2CommandExceptionType<i32, i32> = Dynamic2CommandExceptionType::new(|found, min| literal(format!("Integer must not be less than {min}, found {found}")));
    integer_too_high: Dynamic2CommandExceptionType<i32, i32> = Dynamic2CommandExceptionType::new(|found, max| literal(format!("Integer must not be more than {max}, found {found}")));
    long_too_low: Dynamic2CommandExceptionType<i64, i64> = Dynamic2CommandExceptionType::new(|found, min| literal(format!("Long must not be less than {min}, found {found}")));
    long_too_high: Dynamic2CommandExceptionType<i64, i64> = Dynamic2CommandExceptionType::new(|found, max| literal(format!("Long must not be more than {max}, found {found}")));
    literal_incorrect: DynamicCommandExceptionType<String> = dynamic(|expected| format!("Expected literal {expected}"));
    reader_expected_start_of_quote: SimpleCommandExceptionType = simple("Expected quote to start a string");
    reader_expected_end_of_quote: SimpleCommandExceptionType = simple("Unclosed quoted string");
    reader_invalid_escape: DynamicCommandExceptionType<String> = dynamic(|c| format!("Invalid escape sequence '{c}' in quoted string"));
    reader_invalid_bool: DynamicCommandExceptionType<String> = dynamic(|v| format!("Invalid bool, expected true or false but found '{v}'"));
    reader_invalid_int: DynamicCommandExceptionType<String> = dynamic(|v| format!("Invalid integer '{v}'"));
    reader_expected_int: SimpleCommandExceptionType = simple("Expected integer");
    reader_invalid_long: DynamicCommandExceptionType<String> = dynamic(|v| format!("Invalid long '{v}'"));
    reader_expected_long: SimpleCommandExceptionType = simple("Expected long");
    reader_invalid_double: DynamicCommandExceptionType<String> = dynamic(|v| format!("Invalid double '{v}'"));
    reader_expected_double: SimpleCommandExceptionType = simple("Expected double");
    reader_invalid_float: DynamicCommandExceptionType<String> = dynamic(|v| format!("Invalid float '{v}'"));
    reader_expected_float: SimpleCommandExceptionType = simple("Expected float");
    reader_expected_bool: SimpleCommandExceptionType = simple("Expected bool");
    reader_expected_symbol: DynamicCommandExceptionType<String> = dynamic(|s| format!("Expected '{s}'"));
    dispatcher_unknown_command: SimpleCommandExceptionType = simple("Unknown command");
    dispatcher_unknown_argument: SimpleCommandExceptionType = simple("Incorrect argument for command");
    dispatcher_expected_argument_separator: SimpleCommandExceptionType = simple("Expected whitespace to end one argument, but found trailing data");
    dispatcher_parse_exception: DynamicCommandExceptionType<String> = dynamic(|m| format!("Could not parse command: {m}"));
}

impl Default for BuiltInExceptions {
    fn default() -> Self {
        Self::new()
    }
}

static BUILT_IN: LazyLock<RwLock<Arc<dyn BuiltInExceptionProvider>>> =
    LazyLock::new(|| RwLock::new(Arc::new(BuiltInExceptions::new())));

pub fn built_in_exceptions() -> Arc<dyn BuiltInExceptionProvider> {
    BUILT_IN.read().unwrap_or_else(|e| e.into_inner()).clone()
}

pub fn set_built_in_exceptions(provider: Arc<dyn BuiltInExceptionProvider>) {
    *BUILT_IN.write().unwrap_or_else(|e| e.into_inner()) = provider;
}
