//! `RangeArgument.Ints` / `RangeArgument.Floats`.

use super::min_max_bounds::{Doubles, Ints};
use crate::commands::CommandSourceStack;
use mc_brigadier::arguments::value;
use mc_brigadier::{
    ArgumentType, CommandContext, CommandSyntaxException, ParsedValue, StringReader,
};

/// `RangeArgument.Ints`.
#[derive(Clone, Copy, Debug, Default)]
pub struct IntRangeArgument;

impl IntRangeArgument {
    pub fn int_range() -> Self {
        Self
    }

    pub fn get_range(context: &CommandContext<CommandSourceStack>, name: &str) -> Ints {
        *context.get_argument::<Ints>(name)
    }
}

impl ArgumentType<CommandSourceStack> for IntRangeArgument {
    fn parse(&self, reader: &mut StringReader) -> Result<ParsedValue, CommandSyntaxException> {
        Ints::from_reader(reader).map(value)
    }

    fn get_examples(&self) -> Vec<String> {
        ["0..5", "0", "-5", "-100..", "..100"]
            .map(String::from)
            .to_vec()
    }

    fn describe(&self) -> String {
        "Ints".to_owned()
    }
}

/// `RangeArgument.Floats`.
#[derive(Clone, Copy, Debug, Default)]
pub struct FloatRangeArgument;

impl FloatRangeArgument {
    pub fn float_range() -> Self {
        Self
    }

    pub fn get_range(context: &CommandContext<CommandSourceStack>, name: &str) -> Doubles {
        *context.get_argument::<Doubles>(name)
    }
}

impl ArgumentType<CommandSourceStack> for FloatRangeArgument {
    fn parse(&self, reader: &mut StringReader) -> Result<ParsedValue, CommandSyntaxException> {
        Doubles::from_reader(reader).map(value)
    }

    fn get_examples(&self) -> Vec<String> {
        ["0..5.2", "0", "-5.4", "-100.76..", "..100"]
            .map(String::from)
            .to_vec()
    }

    fn describe(&self) -> String {
        "Floats".to_owned()
    }
}

crate::register_argument_type!(IntRangeArgument, |_a| {
    mc_protocol::packets::play::ArgumentTypeTemplate::IntRange
});
crate::register_argument_type!(FloatRangeArgument, |_a| {
    mc_protocol::packets::play::ArgumentTypeTemplate::FloatRange
});
