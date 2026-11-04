//! `CompoundTagArgument`, `NbtTagArgument`, `ComponentArgument`, `StyleArgument`.
//!
//! Vanilla parses with the packrat `SnbtGrammar`; `mc_nbt::snbt` is a port of it, so error texts and cursors match.
//! Grammar-driven suggestions (`parseForSuggestions`) are not exposed by `mc_nbt` and yield no suggestions.

use super::util::{reader_byte_cursor, set_reader_byte_cursor, snbt_exception};
use crate::commands::message::dynamic_translatable;
use crate::commands::CommandSourceStack;
use mc_brigadier::arguments::value;
use mc_brigadier::{
    ArgumentType, CommandContext, CommandSyntaxException, DynamicCommandExceptionType, ImmutableStringReader, ParsedValue, StringReader,
};
use mc_nbt::{CompoundTag, Tag, snbt};
use mc_text::{Component, Style};
use std::sync::LazyLock;

/// `TagParser.parseAsArgument(reader)`.
pub fn parse_tag_as_argument(reader: &mut StringReader) -> Result<Tag, CommandSyntaxException> {
    let input = reader.get_string().to_owned();
    let (tag, end) = snbt::parse_as_argument(&input, reader_byte_cursor(reader)).map_err(|e| snbt_exception(&e, &input))?;
    set_reader_byte_cursor(reader, end);
    Ok(tag)
}

/// `TagParser.parseCompoundAsArgument(reader)`.
pub fn parse_compound_as_argument(reader: &mut StringReader) -> Result<CompoundTag, CommandSyntaxException> {
    let input = reader.get_string().to_owned();
    let (tag, end) = snbt::parse_compound_as_argument(&input, reader_byte_cursor(reader)).map_err(|e| snbt_exception(&e, &input))?;
    set_reader_byte_cursor(reader, end);
    Ok(tag)
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CompoundTagArgument;

impl CompoundTagArgument {
    pub fn compound_tag() -> Self {
        Self
    }

    pub fn get_compound_tag(context: &CommandContext<CommandSourceStack>, name: &str) -> CompoundTag {
        context.get_argument::<CompoundTag>(name).clone()
    }
}

impl ArgumentType<CommandSourceStack> for CompoundTagArgument {
    fn parse(&self, reader: &mut StringReader) -> Result<ParsedValue, CommandSyntaxException> {
        parse_compound_as_argument(reader).map(value)
    }

    fn get_examples(&self) -> Vec<String> {
        ["{}", "{foo=bar}"].map(String::from).to_vec()
    }

    fn describe(&self) -> String {
        "CompoundTagArgument".to_owned()
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct NbtTagArgument;

impl NbtTagArgument {
    pub fn nbt_tag() -> Self {
        Self
    }

    pub fn get_nbt_tag(context: &CommandContext<CommandSourceStack>, name: &str) -> Tag {
        context.get_argument::<Tag>(name).clone()
    }
}

impl ArgumentType<CommandSourceStack> for NbtTagArgument {
    fn parse(&self, reader: &mut StringReader) -> Result<ParsedValue, CommandSyntaxException> {
        parse_tag_as_argument(reader).map(value)
    }

    fn get_examples(&self) -> Vec<String> {
        ["0", "0b", "0l", "0.0", "\"foo\"", "{foo=bar}", "[0]"].map(String::from).to_vec()
    }

    fn describe(&self) -> String {
        "NbtTagArgument".to_owned()
    }
}

pub static ERROR_INVALID_COMPONENT: LazyLock<DynamicCommandExceptionType<String>> = LazyLock::new(|| dynamic_translatable("argument.component.invalid"));
pub static ERROR_INVALID_STYLE: LazyLock<DynamicCommandExceptionType<String>> = LazyLock::new(|| dynamic_translatable("argument.style.invalid"));

/// `CommandArgumentParser.withCodec`: parse an SNBT tag, then decode it, failing with `exception` at the start cursor.
fn parse_with_codec<T>(
    reader: &mut StringReader,
    decode: impl FnOnce(&Tag) -> Result<T, mc_text::CodecError>,
    exception: &DynamicCommandExceptionType<String>,
) -> Result<T, CommandSyntaxException> {
    let cursor = reader.get_cursor();
    let tag = parse_tag_as_argument(reader)?;
    decode(&tag).map_err(|e| {
        reader.set_cursor(cursor);
        exception.create_with_context(reader, e.0)
    })
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ComponentArgument;

impl ComponentArgument {
    pub fn text_component() -> Self {
        Self
    }

    pub fn get_raw_component(context: &CommandContext<CommandSourceStack>, name: &str) -> Component {
        context.get_argument::<Component>(name).clone()
    }
}

impl ArgumentType<CommandSourceStack> for ComponentArgument {
    fn parse(&self, reader: &mut StringReader) -> Result<ParsedValue, CommandSyntaxException> {
        parse_with_codec(reader, Component::from_nbt, &ERROR_INVALID_COMPONENT).map(value)
    }

    fn get_examples(&self) -> Vec<String> {
        ["\"hello world\"", "'hello world'", "\"\"", "{text:\"hello world\"}", "[\"\"]"].map(String::from).to_vec()
    }

    fn describe(&self) -> String {
        "ComponentArgument".to_owned()
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct StyleArgument;

impl StyleArgument {
    pub fn style() -> Self {
        Self
    }

    pub fn get_style(context: &CommandContext<CommandSourceStack>, name: &str) -> Style {
        context.get_argument::<Style>(name).clone()
    }
}

impl ArgumentType<CommandSourceStack> for StyleArgument {
    fn parse(&self, reader: &mut StringReader) -> Result<ParsedValue, CommandSyntaxException> {
        parse_with_codec(reader, Style::from_nbt, &ERROR_INVALID_STYLE).map(value)
    }

    fn get_examples(&self) -> Vec<String> {
        ["{bold: true}", "{color: 'red'}", "{}"].map(String::from).to_vec()
    }

    fn describe(&self) -> String {
        "StyleArgument".to_owned()
    }
}

crate::register_argument_type!(CompoundTagArgument, |_a| mc_protocol::packets::play::ArgumentTypeTemplate::NbtCompoundTag);
crate::register_argument_type!(NbtTagArgument, |_a| mc_protocol::packets::play::ArgumentTypeTemplate::NbtTag);
crate::register_argument_type!(ComponentArgument, |_a| mc_protocol::packets::play::ArgumentTypeTemplate::Component);
crate::register_argument_type!(StyleArgument, |_a| mc_protocol::packets::play::ArgumentTypeTemplate::Style);
