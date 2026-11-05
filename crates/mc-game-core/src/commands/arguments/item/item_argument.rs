use super::item_input::ItemInput;
use super::item_parser::ItemParser;
use crate::commands::{CommandBuildContext, CommandSourceStack};
use mc_brigadier::arguments::value;
use mc_brigadier::{ArgumentType, CommandContext, CommandSyntaxException, ParsedValue, StringReader, Suggestions, SuggestionsBuilder};
use mc_protocol::packets::play::ArgumentTypeTemplate;

/// `ItemArgument`.
#[derive(Clone)]
pub struct ItemArgument {
    parser: ItemParser,
}

impl ItemArgument {
    pub fn item(context: &CommandBuildContext) -> Self {
        Self { parser: ItemParser::new(context) }
    }

    pub fn get_item(context: &CommandContext<CommandSourceStack>, name: &str) -> ItemInput {
        context.get_argument::<ItemInput>(name).clone()
    }
}

impl ArgumentType<CommandSourceStack> for ItemArgument {
    fn parse(&self, reader: &mut StringReader) -> Result<ParsedValue, CommandSyntaxException> {
        self.parser.parse(reader).map(value)
    }

    fn list_suggestions(&self, _context: &CommandContext<CommandSourceStack>, builder: SuggestionsBuilder) -> Suggestions {
        self.parser.fill_suggestions(builder)
    }

    fn get_examples(&self) -> Vec<String> {
        ["stick", "minecraft:stick", "stick{foo=bar}"].map(String::from).to_vec()
    }

    fn describe(&self) -> String {
        "ItemArgument".to_owned()
    }
}

crate::register_argument_type!(ItemArgument, |_a| ArgumentTypeTemplate::ItemStack);
