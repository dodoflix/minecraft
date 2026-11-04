use crate::commands::CommandSourceStack;
use crate::commands::message::simple_translatable;
use mc_brigadier::arguments::value;
use mc_brigadier::{
    ArgumentType, CommandContext, CommandSyntaxException, ImmutableStringReader, ParsedValue, SimpleCommandExceptionType, StringReader,
};
use mc_core::direction::Axis;
use std::collections::BTreeSet;
use std::sync::LazyLock;

static ERROR_INVALID: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("arguments.swizzle.invalid"));

/// `SwizzleArgument` (`EnumSet<Direction.Axis>`, here an ordinal-ordered set).
#[derive(Clone, Copy, Debug, Default)]
pub struct SwizzleArgument;

impl SwizzleArgument {
    pub fn swizzle() -> Self {
        Self
    }

    pub fn get_swizzle(context: &CommandContext<CommandSourceStack>, name: &str) -> BTreeSet<Axis> {
        context.get_argument::<BTreeSet<Axis>>(name).clone()
    }
}

impl ArgumentType<CommandSourceStack> for SwizzleArgument {
    fn parse(&self, reader: &mut StringReader) -> Result<ParsedValue, CommandSyntaxException> {
        let mut result = BTreeSet::new();
        while reader.can_read() && reader.peek() != ' ' {
            let c = reader.read();
            let axis = match c {
                'x' => Axis::X,
                'y' => Axis::Y,
                'z' => Axis::Z,
                _ => return Err(ERROR_INVALID.create_with_context(reader)),
            };
            if result.contains(&axis) {
                return Err(ERROR_INVALID.create_with_context(reader));
            }
            result.insert(axis);
        }
        Ok(value(result))
    }

    fn get_examples(&self) -> Vec<String> {
        ["xyz", "x"].map(String::from).to_vec()
    }

    fn describe(&self) -> String {
        "SwizzleArgument".to_owned()
    }
}
