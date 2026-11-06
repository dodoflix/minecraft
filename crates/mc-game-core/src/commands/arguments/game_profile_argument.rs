//! `GameProfileArgument`.

use super::selector::entity_selector::{ERROR_ONLY_PLAYERS_ALLOWED, NO_PLAYERS_FOUND};
use super::selector::{EntitySelector, EntitySelectorParser};
use crate::commands::CommandSourceStack;
use crate::commands::host;
use crate::commands::message::simple_translatable;
use crate::commands::shared_suggestion_provider::suggest;
use crate::server::players::NameAndId;
use mc_brigadier::arguments::value;
use mc_brigadier::{
    ArgumentType, CommandContext, CommandSyntaxException, ImmutableStringReader, ParsedValue,
    SimpleCommandExceptionType, StringReader, Suggestions, SuggestionsBuilder,
};
use mc_protocol::packets::play::ArgumentTypeTemplate;
use std::sync::LazyLock;

pub static ERROR_UNKNOWN_PLAYER: LazyLock<SimpleCommandExceptionType> =
    LazyLock::new(|| simple_translatable("argument.player.unknown"));

/// `GameProfileArgument.Result`.
#[derive(Clone, Debug)]
pub enum GameProfileResult {
    /// `SelectorResult`.
    Selector(Box<EntitySelector>),
    /// A plain player name (resolved through the name -> id cache).
    Name(String),
}

impl GameProfileResult {
    /// `getNames(sender)`.
    pub fn get_names(
        &self,
        sender: &CommandSourceStack,
    ) -> Result<Vec<NameAndId>, CommandSyntaxException> {
        sender.with_server(|server| match self {
            GameProfileResult::Selector(selector) => {
                let players = selector.find_players(sender, server)?;
                if players.is_empty() {
                    return Err(NO_PLAYERS_FOUND.create());
                }
                Ok(players
                    .iter()
                    .filter_map(|p| host::get_entity(server, p).and_then(host::player_name_and_id))
                    .collect())
            }
            GameProfileResult::Name(name) => host::name_to_id(server, name)
                .map(|n| vec![n])
                .ok_or_else(|| ERROR_UNKNOWN_PLAYER.create()),
        })
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct GameProfileArgument;

impl GameProfileArgument {
    pub fn game_profile() -> Self {
        Self
    }

    pub fn get_game_profiles(
        context: &CommandContext<CommandSourceStack>,
        name: &str,
    ) -> Result<Vec<NameAndId>, CommandSyntaxException> {
        context
            .get_argument::<GameProfileResult>(name)
            .get_names(context.get_source())
    }

    fn parse_result(
        reader: &mut StringReader,
        allow_selectors: bool,
    ) -> Result<GameProfileResult, CommandSyntaxException> {
        if reader.can_read() && reader.peek() == '@' {
            let mut parser = EntitySelectorParser::new(reader, allow_selectors);
            let parse = parser.parse()?;
            if parse.includes_entities() {
                return Err(ERROR_ONLY_PLAYERS_ALLOWED.create_with_context(reader));
            }
            Ok(GameProfileResult::Selector(Box::new(parse)))
        } else {
            let start = reader.get_cursor();
            while reader.can_read() && reader.peek() != ' ' {
                reader.skip();
            }
            Ok(GameProfileResult::Name(mc_brigadier::utf16::substring(
                reader.get_string(),
                start,
                reader.get_cursor(),
            )))
        }
    }
}

impl ArgumentType<CommandSourceStack> for GameProfileArgument {
    fn parse(&self, reader: &mut StringReader) -> Result<ParsedValue, CommandSyntaxException> {
        Self::parse_result(reader, true).map(value)
    }

    fn parse_with_source(
        &self,
        reader: &mut StringReader,
        source: &CommandSourceStack,
    ) -> Result<ParsedValue, CommandSyntaxException> {
        Self::parse_result(reader, EntitySelectorParser::allow_selectors(source)).map(value)
    }

    fn list_suggestions(
        &self,
        context: &CommandContext<CommandSourceStack>,
        builder: SuggestionsBuilder,
    ) -> Suggestions {
        let source = context.get_source();
        let mut reader = StringReader::new(builder.get_input());
        reader.set_cursor(builder.get_start());
        let mut parser =
            EntitySelectorParser::new(&mut reader, EntitySelectorParser::allow_selectors(source));
        let _ = parser.parse();
        let names = source.get_online_player_names();
        parser.fill_suggestions(builder, &|b| {
            let suggested = suggest(names.iter().map(String::as_str), b.restart());
            for s in suggested.get_list() {
                b.suggest(s.get_text());
            }
        })
    }

    fn get_examples(&self) -> Vec<String> {
        [
            "Player",
            "0123",
            "dd12be42-52a9-4a91-a8a1-11c01849e498",
            "@e",
        ]
        .map(String::from)
        .to_vec()
    }

    fn describe(&self) -> String {
        "GameProfileArgument".to_owned()
    }
}

crate::register_argument_type!(GameProfileArgument, |_a| ArgumentTypeTemplate::GameProfile);
