use super::block_input::BlockInput;
use super::block_state_parser::BlockStateParser;
use crate::commands::{CommandBuildContext, CommandSourceStack, RegistryView};
use mc_brigadier::arguments::value;
use mc_brigadier::{ArgumentType, CommandContext, CommandSyntaxException, ParsedValue, StringReader, Suggestions, SuggestionsBuilder};
use std::sync::Arc;

/// `BlockStateArgument`.
#[derive(Clone)]
pub struct BlockStateArgument {
    blocks: Arc<dyn RegistryView>,
}

impl BlockStateArgument {
    pub fn block(context: &CommandBuildContext) -> Self {
        Self { blocks: context.lookup("minecraft:block").expect("block registry") }
    }

    pub fn get_block(context: &CommandContext<CommandSourceStack>, name: &str) -> BlockInput {
        context.get_argument::<BlockInput>(name).clone()
    }
}

impl ArgumentType<CommandSourceStack> for BlockStateArgument {
    fn parse(&self, reader: &mut StringReader) -> Result<ParsedValue, CommandSyntaxException> {
        let result = BlockStateParser::parse_for_block(self.blocks.as_ref(), reader, true)?;
        Ok(value(BlockInput::new(result.block_state, result.properties, result.nbt)))
    }

    fn list_suggestions(&self, _context: &CommandContext<CommandSourceStack>, builder: SuggestionsBuilder) -> Suggestions {
        BlockStateParser::fill_suggestions(self.blocks.as_ref(), builder, false, true)
    }

    fn get_examples(&self) -> Vec<String> {
        ["stone", "minecraft:stone", "stone[foo=bar]", "foo{bar=baz}"].map(String::from).to_vec()
    }

    fn describe(&self) -> String {
        "BlockStateArgument".to_owned()
    }
}

crate::register_argument_type!(BlockStateArgument, |_a| mc_protocol::packets::play::ArgumentTypeTemplate::BlockState);
