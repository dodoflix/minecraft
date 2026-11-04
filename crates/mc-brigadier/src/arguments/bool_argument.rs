use crate::arguments::{ArgumentType, ParsedValue, value};
use crate::context::CommandContext;
use crate::exceptions::CommandSyntaxException;
use crate::string_reader::StringReader;
use crate::suggestion::{Suggestions, SuggestionsBuilder};

/// `BoolArgumentType`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BoolArgumentType;

impl BoolArgumentType {
    pub fn bool() -> Self {
        Self
    }

    pub fn get_bool<S: 'static>(context: &CommandContext<S>, name: &str) -> bool {
        *context.get_argument::<bool>(name)
    }
}

impl<S: 'static> ArgumentType<S> for BoolArgumentType {
    fn parse(&self, reader: &mut StringReader) -> Result<ParsedValue, CommandSyntaxException> {
        reader.read_boolean().map(value)
    }

    fn list_suggestions(&self, _context: &CommandContext<S>, mut builder: SuggestionsBuilder) -> Suggestions {
        if "true".starts_with(builder.get_remaining_lower_case()) {
            builder.suggest("true");
        }
        if "false".starts_with(builder.get_remaining_lower_case()) {
            builder.suggest("false");
        }
        builder.build()
    }

    fn get_examples(&self) -> Vec<String> {
        vec!["true".into(), "false".into()]
    }

    fn describe(&self) -> String {
        "BoolArgumentType".into()
    }
}
