//! The `mc_text::Validators` that need the command parsers (`CompilableString.CommandParserHelper`).

use super::arguments::NbtPathArgument;
use super::arguments::coordinates::BlockPosArgument;
use super::arguments::selector::EntitySelectorParser;
use super::message::message;
use crate::translatable;
use mc_brigadier::{
    ArgumentType, CommandSyntaxException, DynamicCommandExceptionType, ImmutableStringReader,
    StringReader,
};
use mc_core::ident::Identifier;
use std::sync::LazyLock;

static TRAILING_DATA: LazyLock<DynamicCommandExceptionType<String>> = LazyLock::new(|| {
    DynamicCommandExceptionType::new(|s: String| message(translatable!("command.trailing_data", s)))
});

/// `CommandParserHelper.apply(contents)`: parse everything or fail with the vanilla error text.
fn compile(
    contents: &str,
    parse: impl FnOnce(&mut StringReader) -> Result<(), CommandSyntaxException>,
    error_message: impl Fn(&str, &CommandSyntaxException) -> String,
) -> Result<(), String> {
    let mut reader = StringReader::new(contents);
    let result = parse(&mut reader).and_then(|()| {
        if reader.can_read() {
            let parsed = mc_brigadier::utf16::substring(contents, 0, reader.get_cursor());
            let leftovers = mc_brigadier::utf16::substring_from(contents, reader.get_cursor());
            Err(TRAILING_DATA.create(format!("{parsed}[{leftovers}]")))
        } else {
            Ok(())
        }
    });
    result.map_err(|e| error_message(contents, &e))
}

fn entity_selector(s: &str) -> Result<(), String> {
    compile(
        s,
        |r| EntitySelectorParser::new(r, true).parse().map(|_| ()),
        |original, e| {
            format!(
                "Invalid selector component: {original}: {}",
                e.get_message()
            )
        },
    )
}

fn nbt_path(s: &str) -> Result<(), String> {
    compile(
        s,
        |r| NbtPathArgument.parse(r).map(|_| ()),
        |original, e| format!("Invalid NBT path: {original}: {}", e.get_message()),
    )
}

fn block_coordinates(s: &str) -> Result<(), String> {
    compile(
        s,
        |r| BlockPosArgument::block_pos().parse(r).map(|_| ()),
        |original, e| format!("Invalid coordinates path: {original}: {}", e.get_message()),
    )
}

fn entity_type_exists(id: &Identifier) -> bool {
    mc_data::EntityType::by_name(&id.to_string()).is_some()
}

fn item_exists(id: &Identifier) -> bool {
    mc_data::Item::by_name(&id.to_string()).is_some()
}

/// Installs the validators `mc_text` needs (call once at server start, after `BrigadierExceptions::install`).
pub fn install_text_validators() {
    mc_text::set_validators(mc_text::Validators {
        entity_selector,
        nbt_path,
        block_coordinates,
        entity_type_exists,
        item_exists,
    });
}
