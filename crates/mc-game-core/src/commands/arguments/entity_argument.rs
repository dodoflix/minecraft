//! `EntityArgument`.

use super::selector::entity_selector::{
    ERROR_NOT_SINGLE_ENTITY, ERROR_NOT_SINGLE_PLAYER, ERROR_ONLY_PLAYERS_ALLOWED,
    NO_ENTITIES_FOUND, NO_PLAYERS_FOUND,
};
use super::selector::{EntityRef, EntitySelector, EntitySelectorParser};
use crate::commands::CommandSourceStack;
use crate::commands::shared_suggestion_provider::suggest;
use mc_brigadier::arguments::value;
use mc_brigadier::{
    ArgumentType, CommandContext, CommandSyntaxException, ParsedValue, StringReader, Suggestions,
    SuggestionsBuilder,
};
use mc_protocol::packets::play::ArgumentTypeTemplate;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct EntityArgument {
    single: bool,
    players_only: bool,
}

impl EntityArgument {
    pub fn entity() -> Self {
        Self {
            single: true,
            players_only: false,
        }
    }

    pub fn entities() -> Self {
        Self {
            single: false,
            players_only: false,
        }
    }

    pub fn player() -> Self {
        Self {
            single: true,
            players_only: true,
        }
    }

    pub fn players() -> Self {
        Self {
            single: false,
            players_only: true,
        }
    }

    fn selector<'a>(
        context: &'a CommandContext<CommandSourceStack>,
        name: &str,
    ) -> &'a EntitySelector {
        context.get_argument::<EntitySelector>(name)
    }

    /// `getEntity`.
    pub fn get_entity(
        context: &CommandContext<CommandSourceStack>,
        name: &str,
    ) -> Result<EntityRef, CommandSyntaxException> {
        let source = context.get_source();
        source
            .with_server(|server| Self::selector(context, name).find_single_entity(source, server))
    }

    /// `getEntities`: fails when nothing matched.
    pub fn get_entities(
        context: &CommandContext<CommandSourceStack>,
        name: &str,
    ) -> Result<Vec<EntityRef>, CommandSyntaxException> {
        let result = Self::get_optional_entities(context, name)?;
        if result.is_empty() {
            Err(NO_ENTITIES_FOUND.create())
        } else {
            Ok(result)
        }
    }

    /// `getOptionalEntities`.
    pub fn get_optional_entities(
        context: &CommandContext<CommandSourceStack>,
        name: &str,
    ) -> Result<Vec<EntityRef>, CommandSyntaxException> {
        let source = context.get_source();
        source.with_server(|server| Self::selector(context, name).find_entities(source, server))
    }

    /// `getOptionalPlayers`.
    pub fn get_optional_players(
        context: &CommandContext<CommandSourceStack>,
        name: &str,
    ) -> Result<Vec<EntityRef>, CommandSyntaxException> {
        let source = context.get_source();
        source.with_server(|server| Self::selector(context, name).find_players(source, server))
    }

    /// `getPlayer`.
    pub fn get_player(
        context: &CommandContext<CommandSourceStack>,
        name: &str,
    ) -> Result<EntityRef, CommandSyntaxException> {
        let source = context.get_source();
        source
            .with_server(|server| Self::selector(context, name).find_single_player(source, server))
    }

    /// `getPlayers`: fails when nothing matched.
    pub fn get_players(
        context: &CommandContext<CommandSourceStack>,
        name: &str,
    ) -> Result<Vec<EntityRef>, CommandSyntaxException> {
        let players = Self::get_optional_players(context, name)?;
        if players.is_empty() {
            Err(NO_PLAYERS_FOUND.create())
        } else {
            Ok(players)
        }
    }

    fn parse_selector(
        &self,
        reader: &mut StringReader,
        allow_selectors: bool,
    ) -> Result<EntitySelector, CommandSyntaxException> {
        let mut parser = EntitySelectorParser::new(reader, allow_selectors);
        let selector = parser.parse()?;
        if selector.get_max_results() > 1 && self.single {
            reader.set_cursor(0);
            return Err(if self.players_only {
                ERROR_NOT_SINGLE_PLAYER.create_with_context(reader)
            } else {
                ERROR_NOT_SINGLE_ENTITY.create_with_context(reader)
            });
        }
        if selector.includes_entities() && self.players_only && !selector.is_self_selector() {
            reader.set_cursor(0);
            return Err(ERROR_ONLY_PLAYERS_ALLOWED.create_with_context(reader));
        }
        Ok(selector)
    }
}

impl ArgumentType<CommandSourceStack> for EntityArgument {
    fn parse(&self, reader: &mut StringReader) -> Result<ParsedValue, CommandSyntaxException> {
        self.parse_selector(reader, true).map(value)
    }

    fn parse_with_source(
        &self,
        reader: &mut StringReader,
        source: &CommandSourceStack,
    ) -> Result<ParsedValue, CommandSyntaxException> {
        self.parse_selector(reader, EntitySelectorParser::allow_selectors(source))
            .map(value)
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
        let names: Vec<String> = source.get_online_player_names();
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
            "@e",
            "@e[type=foo]",
            "dd12be42-52a9-4a91-a8a1-11c01849e498",
        ]
        .map(String::from)
        .to_vec()
    }

    fn describe(&self) -> String {
        "EntityArgument".to_owned()
    }
}

crate::register_argument_type!(EntityArgument, |a| ArgumentTypeTemplate::Entity {
    single: a.single,
    players_only: a.players_only
});
