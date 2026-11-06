//! `OperationArgument`.

use crate::commands::CommandSourceStack;
use crate::commands::message::simple_translatable;
use crate::commands::shared_suggestion_provider::suggest;
use mc_brigadier::arguments::value;
use mc_brigadier::{
    ArgumentType, CommandContext, CommandSyntaxException, ImmutableStringReader, ParsedValue,
    SimpleCommandExceptionType, StringReader, Suggestions, SuggestionsBuilder,
};
use mc_core::math::java::{floor_div_i32, floor_mod_i32};
use std::sync::LazyLock;

static ERROR_INVALID_OPERATION: LazyLock<SimpleCommandExceptionType> =
    LazyLock::new(|| simple_translatable("arguments.operation.invalid"));
static ERROR_DIVIDE_BY_ZERO: LazyLock<SimpleCommandExceptionType> =
    LazyLock::new(|| simple_translatable("arguments.operation.div0"));

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
    /// `Operation.apply(a, b)`: the new value of `a`, and the new value of `b` when the operation writes it (only `><`;
    /// the others never touch the source score).
    pub fn apply(self, a: i32, b: i32) -> Result<(i32, Option<i32>), CommandSyntaxException> {
        Ok(match self {
            Operation::Assign => (b, None),
            Operation::Add => (a.wrapping_add(b), None),
            Operation::Sub => (a.wrapping_sub(b), None),
            Operation::Mul => (a.wrapping_mul(b), None),
            Operation::Div => {
                if b == 0 {
                    return Err(ERROR_DIVIDE_BY_ZERO.create());
                }
                (floor_div_i32(a, b), None)
            }
            Operation::Mod => {
                if b == 0 {
                    return Err(ERROR_DIVIDE_BY_ZERO.create());
                }
                (floor_mod_i32(a, b), None)
            }
            Operation::Min => (a.min(b), None),
            Operation::Max => (a.max(b), None),
            Operation::Swap => (b, Some(a)),
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
        Operation::by_symbol(&mc_brigadier::utf16::substring(
            reader.get_string(),
            start,
            reader.get_cursor(),
        ))
        .map(value)
    }

    fn list_suggestions(
        &self,
        _context: &CommandContext<CommandSourceStack>,
        builder: SuggestionsBuilder,
    ) -> Suggestions {
        suggest(["=", "+=", "-=", "*=", "/=", "%=", "<", ">", "><"], builder)
    }

    fn get_examples(&self) -> Vec<String> {
        ["=", ">", "<"].map(String::from).to_vec()
    }

    fn describe(&self) -> String {
        "OperationArgument".to_owned()
    }
}

crate::register_argument_type!(OperationArgument, |_a| {
    mc_protocol::packets::play::ArgumentTypeTemplate::Operation
});
