use super::block_state_parser::{BlockStateParser, PropertyValue};
use crate::commands::{CommandBuildContext, CommandSourceStack, RegistryView};
use mc_brigadier::arguments::value;
use mc_brigadier::{ArgumentType, CommandContext, CommandSyntaxException, ParsedValue, StringReader, Suggestions, SuggestionsBuilder};
use mc_data::block::{Block, BlockState};
use mc_nbt::CompoundTag;
use std::collections::BTreeMap;
use std::sync::Arc;

/// `BlockPredicateArgument.Result`: a block (state) predicate; `matches` takes the state, the block entity data is
/// compared by the caller through [`BlockPredicate::nbt`].
#[derive(Clone, Debug)]
pub enum BlockPredicate {
    /// `BlockPredicateArgument.BlockPredicate`.
    Block { state: BlockState, properties: Vec<PropertyValue>, nbt: Option<CompoundTag> },
    /// `BlockPredicateArgument.TagPredicate`.
    Tag { blocks: Vec<Block>, vague_properties: BTreeMap<String, String>, nbt: Option<CompoundTag> },
}

impl BlockPredicate {
    /// The state part of `test(BlockInWorld)`.
    pub fn matches_state(&self, state: BlockState) -> bool {
        match self {
            BlockPredicate::Block { state: expected, properties, .. } => {
                state.block() == expected.block()
                    && properties.iter().all(|(p, _)| state.get_property_str(p.name) == expected.get_property_str(p.name))
            }
            BlockPredicate::Tag { blocks, vague_properties, .. } => {
                blocks.contains(&state.block())
                    && vague_properties.iter().all(|(key, raw)| {
                        let Some(property) = state.block().properties().find(|p| p.name == *key) else { return false };
                        let Some(index) = super::block_state_parser::parse_property_value(property, raw) else { return false };
                        state.get_property_str(key) == Some(property.values[index])
                    })
            }
        }
    }

    pub fn nbt(&self) -> Option<&CompoundTag> {
        match self {
            BlockPredicate::Block { nbt, .. } | BlockPredicate::Tag { nbt, .. } => nbt.as_ref(),
        }
    }

    /// `requiresNbt()`.
    pub fn requires_nbt(&self) -> bool {
        self.nbt().is_some()
    }

    /// `BlockPredicate.test(BlockInWorld)` / `TagPredicate.test`: the state part, then (when the predicate carries NBT)
    /// the block entity at `pos` must exist and partially match `saveWithFullMetadata`.
    pub fn test(&self, level: &crate::server::level::ServerLevel, pos: mc_core::pos::BlockPos) -> bool {
        use crate::world::level::BlockGetter;
        if !self.matches_state(level.get_block_state(pos)) {
            return false;
        }
        let Some(nbt) = self.nbt() else { return true };
        level.get_block_entity(pos).is_some_and(|entity| {
            mc_nbt::nbt_utils::compare_nbt(
                Some(&mc_nbt::Tag::Compound(nbt.clone())),
                Some(&mc_nbt::Tag::Compound(entity.save_with_full_metadata())),
                true,
            )
        })
    }
}

/// `BlockPredicateArgument`.
#[derive(Clone)]
pub struct BlockPredicateArgument {
    blocks: Arc<dyn RegistryView>,
}

impl BlockPredicateArgument {
    pub fn block_predicate(context: &CommandBuildContext) -> Self {
        Self { blocks: context.lookup("minecraft:block").expect("block registry") }
    }

    pub fn parse_with(blocks: &dyn RegistryView, reader: &mut StringReader) -> Result<BlockPredicate, CommandSyntaxException> {
        Ok(match BlockStateParser::parse_for_testing(blocks, reader, true)? {
            Ok(block) => BlockPredicate::Block { state: block.block_state, properties: block.properties, nbt: block.nbt },
            Err(tag) => BlockPredicate::Tag { blocks: tag.tag_blocks, vague_properties: tag.vague_properties, nbt: tag.nbt },
        })
    }

    pub fn get_block_predicate(context: &CommandContext<CommandSourceStack>, name: &str) -> BlockPredicate {
        context.get_argument::<BlockPredicate>(name).clone()
    }
}

impl ArgumentType<CommandSourceStack> for BlockPredicateArgument {
    fn parse(&self, reader: &mut StringReader) -> Result<ParsedValue, CommandSyntaxException> {
        Self::parse_with(self.blocks.as_ref(), reader).map(value)
    }

    fn list_suggestions(&self, _context: &CommandContext<CommandSourceStack>, builder: SuggestionsBuilder) -> Suggestions {
        BlockStateParser::fill_suggestions(self.blocks.as_ref(), builder, true, true)
    }

    fn get_examples(&self) -> Vec<String> {
        ["stone", "minecraft:stone", "stone[foo=bar]", "#stone", "#stone[foo=bar]{baz=nbt}"].map(String::from).to_vec()
    }

    fn describe(&self) -> String {
        "BlockPredicateArgument".to_owned()
    }
}

crate::register_argument_type!(BlockPredicateArgument, |_a| mc_protocol::packets::play::ArgumentTypeTemplate::BlockPredicate);
