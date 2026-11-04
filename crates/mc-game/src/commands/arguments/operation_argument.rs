//! `OperationArgument`.

use crate::commands::message::simple_translatable;
use crate::commands::shared_suggestion_provider::suggest;
use crate::commands::CommandSourceStack;
use mc_brigadier::arguments::value;
use mc_brigadier::{
    ArgumentType, CommandContext, CommandSyntaxException, ImmutableStringReader, ParsedValue, SimpleCommandExceptionType, StringReader,
    Suggestions, SuggestionsBuilder,
};
use mc_core::math::java::{floor_div_i32, floor_mod_i32};
use std::sync::LazyLock;

static ERROR_INVALID_OPERATION: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("arguments.operation.invalid"));
static ERROR_DIVIDE_BY_ZERO: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("arguments.operation.div0"));

/// `OperationArgument.Operation`: the operation on two scores (`a op= b`; `><` swaps).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Operation {
    Assign,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Min,
    Max,
    Swap,
}

impl Operation {
    /// `Operation.apply(a, b)`: the new values of `(a, b)`.
    pub fn apply(self, a: i32, b: i32) -> Result<(i32, i32), CommandSyntaxException> {
        Ok(match self {
            Operation::Assign => (b, b),
            Operation::Add => (a.wrapping_add(b), b),
            Operation::Sub => (a.wrapping_sub(b), b),
            Operation::Mul => (a.wrapping_mul(b), b),
            Operation::Div => {
                if b == 0 {
                    return Err(ERROR_DIVIDE_BY_ZERO.create());
                }
                (floor_div_i32(a, b), b)
            }
            Operation::Mod => {
                if b == 0 {
                    return Err(ERROR_DIVIDE_BY_ZERO.create());
                }
                (floor_mod_i32(a, b), b)
            }
            Operation::Min => (a.min(b), b),
            Operation::Max => (a.max(b), b),
            Operation::Swap => (b, a),
        })
    }

    fn by_symbol(op: &str) -> Result<Operation, CommandSyntaxException> {
        match op {
            "=" => Ok(Operation::Assign),
            "+=" => Ok(Operation::Add),
            "-=" => Ok(Operation::Sub),
            "*=" => Ok(Operation::Mul),
            "/=" => Ok(Operation::Div),
            "%=" => Ok(Operation::Mod),
            "<" => Ok(Operation::Min),
            ">" => Ok(Operation::Max),
            "><" => Ok(Operation::Swap),
            _ => Err(ERROR_INVALID_OPERATION.create()),
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct OperationArgument;

impl OperationArgument {
    pub fn operation() -> Self {
        Self
    }

    pub fn get_operation(context: &CommandContext<CommandSourceStack>, name: &str) -> Operation {
        *context.get_argument::<Operation>(name)
    }
}

impl ArgumentType<CommandSourceStack> for OperationArgument {
    fn parse(&self, reader: &mut StringReader) -> Result<ParsedValue, CommandSyntaxException> {
        if !reader.can_read() {
            return Err(ERROR_INVALID_OPERATION.create_with_context(reader));
        }
        let start = reader.get_cursor();
        while reader.can_read() && reader.peek() != ' ' {
            reader.skip();
        }
        Operation::by_symbol(&mc_brigadier::utf16::substring(reader.get_string(), start, reader.get_cursor())).map(value)
    }

    fn list_suggestions(&self, _context: &CommandContext<CommandSourceStack>, builder: SuggestionsBuilder) -> Suggestions {
        suggest(["=", "+=", "-=", "*=", "/=", "%=", "<", ">", "><"], builder)
    }

    fn get_examples(&self) -> Vec<String> {
        ["=", ">", "<"].map(String::from).to_vec()
    }

    fn describe(&self) -> String {
        "OperationArgument".to_owned()
    }
}

crate::register_argument_type!(OperationArgument, |_a| mc_protocol::packets::play::ArgumentTypeTemplate::Operation);
