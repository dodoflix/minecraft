//! Scoreboard-related argument types: `ObjectiveArgument`, `ObjectiveCriteriaArgument`, `TeamArgument`,
//! `ScoreboardSlotArgument`, `ScoreHolderArgument` (+ the small `world.scores` value types they produce).

use super::selector::entity_selector::{ERROR_NOT_SINGLE_ENTITY, NO_ENTITIES_FOUND};
use super::selector::{EntityRef, EntitySelector, EntitySelectorParser};
use crate::commands::CommandSourceStack;
use crate::commands::host;
use crate::commands::message::{dynamic_translatable, simple_translatable};
use crate::commands::shared_suggestion_provider::suggest;
use crate::world::scores::objective_criteria::stat_type_registry;
pub use crate::world::scores::{DisplaySlot, ObjectiveCriteria, RenderType};
use mc_brigadier::arguments::value;
use mc_brigadier::{
    ArgumentType, CommandContext, CommandSyntaxException, DynamicCommandExceptionType,
    ImmutableStringReader, ParsedValue, SimpleCommandExceptionType, StringReader,
    SuggestionProvider, Suggestions, SuggestionsBuilder,
};
use mc_protocol::packets::play::ArgumentTypeTemplate;
use mc_text::Component;
use std::sync::{Arc, LazyLock};
use uuid::Uuid;

/// All criteria names offered by `ObjectiveCriteriaArgument.listSuggestions`: the custom ones, then every
/// `<stat type>:<value>` (`Stat.buildName`).
fn criteria_suggestion_names() -> Vec<String> {
    let mut ids = ObjectiveCriteria::get_custom_criteria_names();
    for stat_type in [
        "mined",
        "crafted",
        "used",
        "broken",
        "picked_up",
        "dropped",
        "killed",
        "killed_by",
        "custom",
    ] {
        let registry =
            mc_data::registry::by_name(stat_type_registry(stat_type).expect("known stat type"))
                .expect("registry");
        for entry in registry.entries() {
            ids.push(format!("minecraft.{stat_type}:{}", entry.replace(':', ".")));
        }
    }
    ids
}

// ---- ScoreHolder ----

/// `ScoreHolder`: a score holder name plus how it is shown in feedback.
#[derive(Clone, PartialEq, Debug)]
pub struct ScoreHolder {
    pub scoreboard_name: String,
    pub display_name: Option<Component>,
    /// `Entity` holders keep their identity for commands that act on the entity.
    pub entity: Option<EntityRef>,
}

impl ScoreHolder {
    pub const WILDCARD_NAME: &'static str = "*";

    /// `forNameOnly(name)`.
    pub fn for_name_only(name: &str) -> ScoreHolder {
        ScoreHolder {
            scoreboard_name: name.to_owned(),
            display_name: None,
            entity: None,
        }
    }

    /// `getFeedbackDisplayName()`.
    pub fn get_feedback_display_name(&self) -> Component {
        match &self.display_name {
            Some(display) if self.entity.is_some() => display.clone().with_style_fn(|s| {
                s.with_hover_event(Some(mc_text::events::HoverEvent::show_text(
                    Component::literal(self.scoreboard_name.clone()),
                )))
            }),
            _ => Component::literal(self.scoreboard_name.clone()),
        }
    }

    /// `Entity` as a score holder (`getScoreboardName`: the profile name of a player, else the UUID).
    pub fn of_entity(
        server: &crate::server::MinecraftServer,
        entity_ref: &EntityRef,
    ) -> Option<ScoreHolder> {
        let entity = host::get_entity(server, entity_ref)?;
        let name = match host::player_name_and_id(entity) {
            Some(n) => n.name,
            None => entity.uuid().to_string(),
        };
        Some(ScoreHolder {
            scoreboard_name: name,
            display_name: Some(host::entity_display_name(server, entity)),
            entity: Some(entity_ref.clone()),
        })
    }
}

impl crate::world::scores::ScoreHolder for ScoreHolder {
    fn get_scoreboard_name(&self) -> String {
        self.scoreboard_name.clone()
    }

    fn get_display_name(&self) -> Option<Component> {
        self.display_name.clone()
    }

    fn get_feedback_display_name(&self) -> Component {
        ScoreHolder::get_feedback_display_name(self)
    }
}

// ---- ObjectiveArgument ----

static ERROR_OBJECTIVE_NOT_FOUND: LazyLock<DynamicCommandExceptionType<String>> =
    LazyLock::new(|| dynamic_translatable("arguments.objective.notFound"));
static ERROR_OBJECTIVE_READ_ONLY: LazyLock<DynamicCommandExceptionType<String>> =
    LazyLock::new(|| dynamic_translatable("arguments.objective.readonly"));

#[derive(Clone, Copy, Debug, Default)]
pub struct ObjectiveArgument;

impl ObjectiveArgument {
    pub fn objective() -> Self {
        Self
    }

    /// `getObjective`: the name of an existing objective.
    pub fn get_objective(
        context: &CommandContext<CommandSourceStack>,
        name: &str,
    ) -> Result<String, CommandSyntaxException> {
        let id = context.get_argument::<String>(name).clone();
        if context
            .get_source()
            .with_server(|server| host::scoreboard_has_objective(server, &id))
        {
            Ok(id)
        } else {
            Err(ERROR_OBJECTIVE_NOT_FOUND.create(id))
        }
    }

    /// `getWritableObjective`.
    pub fn get_writable_objective(
        context: &CommandContext<CommandSourceStack>,
        name: &str,
    ) -> Result<String, CommandSyntaxException> {
        let objective = Self::get_objective(context, name)?;
        if context
            .get_source()
            .with_server(|server| host::scoreboard_objective_read_only(server, &objective))
        {
            Err(ERROR_OBJECTIVE_READ_ONLY.create(objective))
        } else {
            Ok(objective)
        }
    }
}

impl ArgumentType<CommandSourceStack> for ObjectiveArgument {
    fn parse(&self, reader: &mut StringReader) -> Result<ParsedValue, CommandSyntaxException> {
        Ok(value(reader.read_unquoted_string()))
    }

    fn list_suggestions(
        &self,
        context: &CommandContext<CommandSourceStack>,
        builder: SuggestionsBuilder,
    ) -> Suggestions {
        let names = context
            .get_source()
            .with_server(|server| host::scoreboard_objective_names(server));
        suggest(names, builder)
    }

    fn get_examples(&self) -> Vec<String> {
        ["foo", "*", "012"].map(String::from).to_vec()
    }

    fn describe(&self) -> String {
        "ObjectiveArgument".to_owned()
    }
}

// ---- ObjectiveCriteriaArgument ----

pub static ERROR_INVALID_CRITERIA: LazyLock<DynamicCommandExceptionType<String>> =
    LazyLock::new(|| dynamic_translatable("argument.criteria.invalid"));

#[derive(Clone, Copy, Debug, Default)]
pub struct ObjectiveCriteriaArgument;

impl ObjectiveCriteriaArgument {
    pub fn criteria() -> Self {
        Self
    }

    pub fn get_criteria(
        context: &CommandContext<CommandSourceStack>,
        name: &str,
    ) -> ObjectiveCriteria {
        context.get_argument::<ObjectiveCriteria>(name).clone()
    }
}

impl ArgumentType<CommandSourceStack> for ObjectiveCriteriaArgument {
    fn parse(&self, reader: &mut StringReader) -> Result<ParsedValue, CommandSyntaxException> {
        let start = reader.get_cursor();
        while reader.can_read() && reader.peek() != ' ' {
            reader.skip();
        }
        let id = mc_brigadier::utf16::substring(reader.get_string(), start, reader.get_cursor());
        match ObjectiveCriteria::by_name(&id) {
            Some(c) => Ok(value(c)),
            None => {
                reader.set_cursor(start);
                Err(ERROR_INVALID_CRITERIA.create_with_context(reader, id))
            }
        }
    }

    fn list_suggestions(
        &self,
        _context: &CommandContext<CommandSourceStack>,
        builder: SuggestionsBuilder,
    ) -> Suggestions {
        suggest(criteria_suggestion_names(), builder)
    }

    fn get_examples(&self) -> Vec<String> {
        ["foo", "foo.bar.baz", "minecraft:foo"]
            .map(String::from)
            .to_vec()
    }

    fn describe(&self) -> String {
        "ObjectiveCriteriaArgument".to_owned()
    }
}

// ---- TeamArgument ----

static ERROR_TEAM_NOT_FOUND: LazyLock<DynamicCommandExceptionType<String>> =
    LazyLock::new(|| dynamic_translatable("team.notFound"));

#[derive(Clone, Copy, Debug, Default)]
pub struct TeamArgument;

impl TeamArgument {
    pub fn team() -> Self {
        Self
    }

    /// `getTeam`: the name of an existing team.
    pub fn get_team(
        context: &CommandContext<CommandSourceStack>,
        name: &str,
    ) -> Result<String, CommandSyntaxException> {
        let id = context.get_argument::<String>(name).clone();
        if context
            .get_source()
            .with_server(|server| host::scoreboard_has_team(server, &id))
        {
            Ok(id)
        } else {
            Err(ERROR_TEAM_NOT_FOUND.create(id))
        }
    }
}

impl ArgumentType<CommandSourceStack> for TeamArgument {
    fn parse(&self, reader: &mut StringReader) -> Result<ParsedValue, CommandSyntaxException> {
        Ok(value(reader.read_unquoted_string()))
    }

    fn list_suggestions(
        &self,
        context: &CommandContext<CommandSourceStack>,
        builder: SuggestionsBuilder,
    ) -> Suggestions {
        let names = context
            .get_source()
            .with_server(|server| host::scoreboard_team_names(server));
        suggest(names, builder)
    }

    fn get_examples(&self) -> Vec<String> {
        ["foo", "123"].map(String::from).to_vec()
    }

    fn describe(&self) -> String {
        "TeamArgument".to_owned()
    }
}

// ---- ScoreboardSlotArgument ----

pub static ERROR_INVALID_SLOT: LazyLock<DynamicCommandExceptionType<String>> =
    LazyLock::new(|| dynamic_translatable("argument.scoreboardDisplaySlot.invalid"));

#[derive(Clone, Copy, Debug, Default)]
pub struct ScoreboardSlotArgument;

impl ScoreboardSlotArgument {
    pub fn display_slot() -> Self {
        Self
    }

    pub fn get_display_slot(
        context: &CommandContext<CommandSourceStack>,
        name: &str,
    ) -> DisplaySlot {
        *context.get_argument::<DisplaySlot>(name)
    }
}

impl ArgumentType<CommandSourceStack> for ScoreboardSlotArgument {
    fn parse(&self, reader: &mut StringReader) -> Result<ParsedValue, CommandSyntaxException> {
        let name = reader.read_unquoted_string();
        match DisplaySlot::by_name(&name) {
            Some(slot) => Ok(value(slot)),
            None => Err(ERROR_INVALID_SLOT.create_with_context(reader, name)),
        }
    }

    fn list_suggestions(
        &self,
        _context: &CommandContext<CommandSourceStack>,
        builder: SuggestionsBuilder,
    ) -> Suggestions {
        suggest(
            DisplaySlot::VALUES.map(DisplaySlot::get_serialized_name),
            builder,
        )
    }

    fn get_examples(&self) -> Vec<String> {
        ["sidebar", "foo.bar"].map(String::from).to_vec()
    }

    fn describe(&self) -> String {
        "ScoreboardSlotArgument".to_owned()
    }
}

// ---- ScoreHolderArgument ----

static ERROR_NO_RESULTS: LazyLock<SimpleCommandExceptionType> =
    LazyLock::new(|| simple_translatable("argument.scoreHolder.empty"));

/// `ScoreHolderArgument.Result`.
#[derive(Clone, Debug)]
pub enum ScoreHolderResult {
    Selector(EntitySelector),
    /// `*`.
    Wildcard,
    /// `#name`: a name-only holder.
    NameOnly(ScoreHolder),
    Uuid(Uuid, ScoreHolder),
    Name(String, ScoreHolder),
}

impl ScoreHolderResult {
    /// `getNames(sender, wildcard)`.
    pub fn get_names(
        &self,
        sender: &CommandSourceStack,
        wildcard: &dyn Fn(&crate::server::MinecraftServer) -> Vec<ScoreHolder>,
    ) -> Result<Vec<ScoreHolder>, CommandSyntaxException> {
        sender.with_server(|server| match self {
            ScoreHolderResult::Selector(selector) => {
                let entities = selector.find_entities(sender, server)?;
                if entities.is_empty() {
                    return Err(NO_ENTITIES_FOUND.create());
                }
                Ok(entities
                    .iter()
                    .filter_map(|e| ScoreHolder::of_entity(server, e))
                    .collect())
            }
            ScoreHolderResult::Wildcard => {
                let results = wildcard(server);
                if results.is_empty() {
                    Err(ERROR_NO_RESULTS.create())
                } else {
                    Ok(results)
                }
            }
            ScoreHolderResult::NameOnly(holder) => Ok(vec![holder.clone()]),
            ScoreHolderResult::Uuid(uuid, name_only) => {
                let mut found = Vec::new();
                for dimension in server.level_keys() {
                    if let Some(entity) = server
                        .level(&dimension)
                        .and_then(|l| l.get_entity_by_uuid(*uuid))
                    {
                        let r = EntityRef {
                            dimension: dimension.clone(),
                            id: entity.id(),
                        };
                        found.extend(ScoreHolder::of_entity(server, &r));
                    }
                }
                if found.is_empty() {
                    Ok(vec![name_only.clone()])
                } else {
                    Ok(found)
                }
            }
            ScoreHolderResult::Name(name, name_only) => match host::player_by_name(server, name)
                .and_then(|p| ScoreHolder::of_entity(server, &p))
            {
                Some(p) => Ok(vec![p]),
                None => Ok(vec![name_only.clone()]),
            },
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ScoreHolderArgument {
    multiple: bool,
}

/// `ScoreHolderArgument.SUGGEST_SCORE_HOLDERS`.
pub fn suggest_score_holders() -> SuggestionProvider<CommandSourceStack> {
    Arc::new(|context, builder| {
        let source = context.get_source();
        let mut reader = StringReader::new(builder.get_input());
        reader.set_cursor(builder.get_start());
        let mut parser =
            EntitySelectorParser::new(&mut reader, EntitySelectorParser::allow_selectors(source));
        let _ = parser.parse();
        let names = source.get_online_player_names();
        Ok(parser.fill_suggestions(builder, &|b| {
            let suggested = suggest(names.iter().map(String::as_str), b.restart());
            for s in suggested.get_list() {
                b.suggest(s.get_text());
            }
        }))
    })
}

impl ScoreHolderArgument {
    pub fn score_holder() -> Self {
        Self { multiple: false }
    }

    pub fn score_holders() -> Self {
        Self { multiple: true }
    }

    /// `getName`.
    pub fn get_name(
        context: &CommandContext<CommandSourceStack>,
        name: &str,
    ) -> Result<ScoreHolder, CommandSyntaxException> {
        Ok(Self::get_names(context, name)?.remove(0))
    }

    pub fn get_names(
        context: &CommandContext<CommandSourceStack>,
        name: &str,
    ) -> Result<Vec<ScoreHolder>, CommandSyntaxException> {
        Self::get_names_with(context, name, &|_| Vec::new())
    }

    /// `getNamesWithDefaultWildcard`.
    pub fn get_names_with_default_wildcard(
        context: &CommandContext<CommandSourceStack>,
        name: &str,
    ) -> Result<Vec<ScoreHolder>, CommandSyntaxException> {
        Self::get_names_with(context, name, &|server| {
            host::scoreboard_tracked_players(server)
                .iter()
                .map(|n| ScoreHolder::for_name_only(n))
                .collect()
        })
    }

    pub fn get_names_with(
        context: &CommandContext<CommandSourceStack>,
        name: &str,
        wildcard: &dyn Fn(&crate::server::MinecraftServer) -> Vec<ScoreHolder>,
    ) -> Result<Vec<ScoreHolder>, CommandSyntaxException> {
        let result = context
            .get_argument::<ScoreHolderResult>(name)
            .get_names(context.get_source(), wildcard)?;
        if result.is_empty() {
            Err(NO_ENTITIES_FOUND.create())
        } else {
            Ok(result)
        }
    }

    fn parse_result(
        &self,
        reader: &mut StringReader,
        allow_selectors: bool,
    ) -> Result<ScoreHolderResult, CommandSyntaxException> {
        if reader.can_read() && reader.peek() == '@' {
            let mut parser = EntitySelectorParser::new(reader, allow_selectors);
            let selector = parser.parse()?;
            if !self.multiple && selector.get_max_results() > 1 {
                return Err(ERROR_NOT_SINGLE_ENTITY.create_with_context(reader));
            }
            return Ok(ScoreHolderResult::Selector(selector));
        }
        let start = reader.get_cursor();
        while reader.can_read() && reader.peek() != ' ' {
            reader.skip();
        }
        let text = mc_brigadier::utf16::substring(reader.get_string(), start, reader.get_cursor());
        if text == "*" {
            return Ok(ScoreHolderResult::Wildcard);
        }
        let name_only = ScoreHolder::for_name_only(&text);
        if text.starts_with('#') {
            return Ok(ScoreHolderResult::NameOnly(name_only));
        }
        Ok(match mc_core::uuid_util::uuid_from_string(&text) {
            Ok(uuid) => ScoreHolderResult::Uuid(uuid, name_only),
            Err(_) => ScoreHolderResult::Name(text, name_only),
        })
    }
}

impl ArgumentType<CommandSourceStack> for ScoreHolderArgument {
    fn parse(&self, reader: &mut StringReader) -> Result<ParsedValue, CommandSyntaxException> {
        self.parse_result(reader, true).map(value)
    }

    fn parse_with_source(
        &self,
        reader: &mut StringReader,
        source: &CommandSourceStack,
    ) -> Result<ParsedValue, CommandSyntaxException> {
        self.parse_result(reader, EntitySelectorParser::allow_selectors(source))
            .map(value)
    }

    fn get_examples(&self) -> Vec<String> {
        ["Player", "0123", "*", "@e"].map(String::from).to_vec()
    }

    fn describe(&self) -> String {
        "ScoreHolderArgument".to_owned()
    }
}

crate::register_argument_type!(ObjectiveArgument, |_a| ArgumentTypeTemplate::Objective);
crate::register_argument_type!(ObjectiveCriteriaArgument, |_a| {
    ArgumentTypeTemplate::ObjectiveCriteria
});
crate::register_argument_type!(TeamArgument, |_a| ArgumentTypeTemplate::Team);
crate::register_argument_type!(ScoreboardSlotArgument, |_a| {
    ArgumentTypeTemplate::ScoreboardSlot
});
crate::register_argument_type!(ScoreHolderArgument, |a| ArgumentTypeTemplate::ScoreHolder {
    multiple: a.multiple
});
