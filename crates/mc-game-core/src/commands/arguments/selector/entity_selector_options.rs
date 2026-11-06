//! `EntitySelectorOptions`: the `[key=value,...]` options of a selector.

use super::entity_selector::Order;
use super::entity_selector_parser::{EntitySelectorParser, Suggest};
use crate::commands::arguments::min_max_bounds::{Doubles, FloatDegrees, Ints};
use crate::commands::arguments::read_identifier;
use crate::commands::host::{self, AdvancementExpectation};
use crate::commands::message::{dynamic_translatable, message, simple_translatable};
use crate::commands::shared_suggestion_provider::{
    suggest, suggest_resource_into, suggest_resource_prefixed_into,
};
use crate::translatable;
use mc_brigadier::{
    CommandSyntaxException, DynamicCommandExceptionType, ImmutableStringReader, MessageRef,
    SimpleCommandExceptionType, StringReader, SuggestionsBuilder,
};
use mc_core::ident::Identifier;
use mc_data::EntityType;
use mc_nbt::Tag;
use mc_nbt::nbt_utils::compare_nbt;
use mc_text::Component;
use mc_world::level_storage::GameType;
use std::sync::{Arc, LazyLock};

pub static ERROR_UNKNOWN_OPTION: LazyLock<DynamicCommandExceptionType<String>> =
    LazyLock::new(|| dynamic_translatable("argument.entity.options.unknown"));
pub static ERROR_INAPPLICABLE_OPTION: LazyLock<DynamicCommandExceptionType<String>> =
    LazyLock::new(|| dynamic_translatable("argument.entity.options.inapplicable"));
pub static ERROR_RANGE_NEGATIVE: LazyLock<SimpleCommandExceptionType> =
    LazyLock::new(|| simple_translatable("argument.entity.options.distance.negative"));
pub static ERROR_LEVEL_NEGATIVE: LazyLock<SimpleCommandExceptionType> =
    LazyLock::new(|| simple_translatable("argument.entity.options.level.negative"));
pub static ERROR_LIMIT_TOO_SMALL: LazyLock<SimpleCommandExceptionType> =
    LazyLock::new(|| simple_translatable("argument.entity.options.limit.toosmall"));
pub static ERROR_SORT_UNKNOWN: LazyLock<DynamicCommandExceptionType<String>> =
    LazyLock::new(|| dynamic_translatable("argument.entity.options.sort.irreversible"));
pub static ERROR_GAME_MODE_INVALID: LazyLock<DynamicCommandExceptionType<String>> =
    LazyLock::new(|| dynamic_translatable("argument.entity.options.mode.invalid"));
pub static ERROR_ENTITY_TYPE_INVALID: LazyLock<DynamicCommandExceptionType<String>> =
    LazyLock::new(|| dynamic_translatable("argument.entity.options.type.invalid"));

pub type Modifier = fn(&mut EntitySelectorParser<'_>) -> Result<(), CommandSyntaxException>;

struct Option_ {
    name: &'static str,
    modifier: Modifier,
    can_use: fn(&EntitySelectorParser<'_>) -> bool,
    description: &'static str,
}

fn rollback_and_throw_simple(
    parser: &mut EntitySelectorParser<'_>,
    start: usize,
    type_: &SimpleCommandExceptionType,
) -> CommandSyntaxException {
    parser.get_reader().set_cursor(start);
    type_.create_with_context(parser.get_reader())
}

fn rollback_and_throw(
    parser: &mut EntitySelectorParser<'_>,
    start: usize,
    type_: &DynamicCommandExceptionType<String>,
    argument: impl Into<String>,
) -> CommandSyntaxException {
    parser.get_reader().set_cursor(start);
    type_.create_with_context(parser.get_reader(), argument.into())
}

/// `OPTIONS` in registration order.
static OPTIONS: &[Option_] = &[
    Option_ {
        name: "name",
        modifier: name,
        can_use: |s| s.name_option_ref().can_parse_any(),
        description: "argument.entity.options.name.description",
    },
    Option_ {
        name: "distance",
        modifier: distance,
        can_use: |s| s.get_distance().is_none(),
        description: "argument.entity.options.distance.description",
    },
    Option_ {
        name: "level",
        modifier: level,
        can_use: |s| s.get_level().is_none(),
        description: "argument.entity.options.level.description",
    },
    Option_ {
        name: "x",
        modifier: |p| {
            p.set_world_limited();
            let v = p.get_reader().read_double()?;
            p.set_x(v);
            Ok(())
        },
        can_use: |s| s.get_x().is_none(),
        description: "argument.entity.options.x.description",
    },
    Option_ {
        name: "y",
        modifier: |p| {
            p.set_world_limited();
            let v = p.get_reader().read_double()?;
            p.set_y(v);
            Ok(())
        },
        can_use: |s| s.get_y().is_none(),
        description: "argument.entity.options.y.description",
    },
    Option_ {
        name: "z",
        modifier: |p| {
            p.set_world_limited();
            let v = p.get_reader().read_double()?;
            p.set_z(v);
            Ok(())
        },
        can_use: |s| s.get_z().is_none(),
        description: "argument.entity.options.z.description",
    },
    Option_ {
        name: "dx",
        modifier: |p| {
            p.set_world_limited();
            let v = p.get_reader().read_double()?;
            p.set_delta_x(v);
            Ok(())
        },
        can_use: |s| s.get_delta_x().is_none(),
        description: "argument.entity.options.dx.description",
    },
    Option_ {
        name: "dy",
        modifier: |p| {
            p.set_world_limited();
            let v = p.get_reader().read_double()?;
            p.set_delta_y(v);
            Ok(())
        },
        can_use: |s| s.get_delta_y().is_none(),
        description: "argument.entity.options.dy.description",
    },
    Option_ {
        name: "dz",
        modifier: |p| {
            p.set_world_limited();
            let v = p.get_reader().read_double()?;
            p.set_delta_z(v);
            Ok(())
        },
        can_use: |s| s.get_delta_z().is_none(),
        description: "argument.entity.options.dz.description",
    },
    Option_ {
        name: "x_rotation",
        modifier: |p| {
            let v = FloatDegrees::from_reader(p.get_reader())?;
            p.set_rot_x(v);
            Ok(())
        },
        can_use: |s| s.get_rot_x().is_none(),
        description: "argument.entity.options.x_rotation.description",
    },
    Option_ {
        name: "y_rotation",
        modifier: |p| {
            let v = FloatDegrees::from_reader(p.get_reader())?;
            p.set_rot_y(v);
            Ok(())
        },
        can_use: |s| s.get_rot_y().is_none(),
        description: "argument.entity.options.y_rotation.description",
    },
    Option_ {
        name: "limit",
        modifier: limit,
        can_use: |s| !s.is_current_entity() && s.limited_option_ref().can_parse(),
        description: "argument.entity.options.limit.description",
    },
    Option_ {
        name: "sort",
        modifier: sort,
        can_use: |s| !s.is_current_entity() && s.sorted_option_ref().can_parse(),
        description: "argument.entity.options.sort.description",
    },
    Option_ {
        name: "gamemode",
        modifier: gamemode,
        can_use: |s| s.gamemode_option_ref().can_parse_any(),
        description: "argument.entity.options.gamemode.description",
    },
    Option_ {
        name: "team",
        modifier: team,
        can_use: |s| s.team_option_ref().can_parse_any(),
        description: "argument.entity.options.team.description",
    },
    Option_ {
        name: "type",
        modifier: type_,
        can_use: |s| s.type_option_ref().can_parse_any(),
        description: "argument.entity.options.type.description",
    },
    Option_ {
        name: "tag",
        modifier: tag,
        can_use: |_| true,
        description: "argument.entity.options.tag.description",
    },
    Option_ {
        name: "nbt",
        modifier: nbt,
        can_use: |_| true,
        description: "argument.entity.options.nbt.description",
    },
    Option_ {
        name: "scores",
        modifier: scores,
        can_use: |s| s.scores_option_ref().can_parse(),
        description: "argument.entity.options.scores.description",
    },
    Option_ {
        name: "advancements",
        modifier: advancements,
        can_use: |s| s.advancements_option_ref().can_parse(),
        description: "argument.entity.options.advancements.description",
    },
    Option_ {
        name: "predicate",
        modifier: predicate,
        can_use: |_| true,
        description: "argument.entity.options.predicate.description",
    },
];

pub struct EntitySelectorOptions;

impl EntitySelectorOptions {
    /// `get(parser, key, start)`.
    pub fn get(
        parser: &mut EntitySelectorParser<'_>,
        key: &str,
        start: usize,
    ) -> Result<Modifier, CommandSyntaxException> {
        match OPTIONS.iter().find(|o| o.name == key) {
            Some(option) => {
                if (option.can_use)(parser) {
                    Ok(option.modifier)
                } else {
                    Err(rollback_and_throw(
                        parser,
                        start,
                        &ERROR_INAPPLICABLE_OPTION,
                        key,
                    ))
                }
            }
            None => Err(rollback_and_throw(
                parser,
                start,
                &ERROR_UNKNOWN_OPTION,
                key,
            )),
        }
    }
}

/// `suggestNames(parser, builder)`.
pub fn suggest_names(parser: &EntitySelectorParser<'_>, builder: &mut SuggestionsBuilder) {
    let lower_prefix = builder.get_remaining().to_lowercase();
    for option in OPTIONS {
        let usable = (option.can_use)(parser);
        if usable && option.name.to_lowercase().starts_with(&lower_prefix) {
            builder.suggest_with_tooltip(
                format!("{}=", option.name),
                Some(message(Component::translatable(
                    option.description,
                    Vec::new(),
                ))),
            );
        }
    }
}

fn name(parser: &mut EntitySelectorParser<'_>) -> Result<(), CommandSyntaxException> {
    let start = parser.get_reader().get_cursor();
    let inverted = parser.should_invert_value();
    let name = parser.get_reader().read_string()?;
    if !parser.name_option().can_parse_element(inverted) {
        return Err(rollback_and_throw(
            parser,
            start,
            &ERROR_INAPPLICABLE_OPTION,
            "name",
        ));
    }
    parser.name_option().mark_parsed_element(inverted);
    parser.add_predicate(Arc::new(move |_, e| {
        (host::entity_plain_text_name(e) == name) != inverted
    }));
    Ok(())
}

fn distance(parser: &mut EntitySelectorParser<'_>) -> Result<(), CommandSyntaxException> {
    let start = parser.get_reader().get_cursor();
    let value = Doubles::from_reader(parser.get_reader())?;
    if value.bounds.min.is_some_and(|m| m < 0.0) || value.bounds.max.is_some_and(|m| m < 0.0) {
        return Err(rollback_and_throw_simple(
            parser,
            start,
            &ERROR_RANGE_NEGATIVE,
        ));
    }
    parser.set_distance(value);
    parser.set_world_limited();
    Ok(())
}

fn level(parser: &mut EntitySelectorParser<'_>) -> Result<(), CommandSyntaxException> {
    let start = parser.get_reader().get_cursor();
    let value = Ints::from_reader(parser.get_reader())?;
    if value.bounds.min.is_some_and(|m| m < 0) || value.bounds.max.is_some_and(|m| m < 0) {
        return Err(rollback_and_throw_simple(
            parser,
            start,
            &ERROR_LEVEL_NEGATIVE,
        ));
    }
    parser.set_level(value);
    parser.set_includes_entities(false);
    Ok(())
}

fn limit(parser: &mut EntitySelectorParser<'_>) -> Result<(), CommandSyntaxException> {
    let start = parser.get_reader().get_cursor();
    let count = parser.get_reader().read_int()?;
    if count < 1 {
        return Err(rollback_and_throw_simple(
            parser,
            start,
            &ERROR_LIMIT_TOO_SMALL,
        ));
    }
    parser.set_max_results(count);
    parser.limited_option().mark_parsed();
    Ok(())
}

fn sort(parser: &mut EntitySelectorParser<'_>) -> Result<(), CommandSyntaxException> {
    let start = parser.get_reader().get_cursor();
    let name = parser.get_reader().read_unquoted_string();
    parser.set_suggestions(Suggest::Custom(Arc::new(|b| {
        suggest(["nearest", "furthest", "random", "arbitrary"], b)
    })));
    let order = match name.as_str() {
        "nearest" => Order::Nearest,
        "furthest" => Order::Furthest,
        "random" => Order::Random,
        "arbitrary" => Order::Arbitrary,
        _ => return Err(rollback_and_throw(parser, start, &ERROR_SORT_UNKNOWN, name)),
    };
    parser.set_order(order);
    parser.sorted_option().mark_parsed();
    Ok(())
}

fn gamemode(parser: &mut EntitySelectorParser<'_>) -> Result<(), CommandSyntaxException> {
    let state = parser.gamemode_option().clone();
    parser.set_suggestions(Suggest::Custom(Arc::new(move |mut b| {
        let mut prefix = b.get_remaining().to_lowercase();
        let mut add_normal = state.can_parse_positive_element();
        let mut add_inverted = state.can_parse_negative_element();
        if let Some(first) = prefix.chars().next() {
            if first == '!' {
                add_normal = false;
                prefix.remove(0);
            } else {
                add_inverted = false;
            }
        }
        for t in [
            GameType::Survival,
            GameType::Creative,
            GameType::Adventure,
            GameType::Spectator,
        ] {
            if t.get_name().to_lowercase().starts_with(&prefix) {
                if add_inverted {
                    b.suggest(format!("!{}", t.get_name()));
                }
                if add_normal {
                    b.suggest(t.get_name());
                }
            }
        }
        b.build()
    })));
    let start = parser.get_reader().get_cursor();
    let inverted = parser.should_invert_value();
    if !parser.gamemode_option().can_parse_element(inverted) {
        return Err(rollback_and_throw(
            parser,
            start,
            &ERROR_INAPPLICABLE_OPTION,
            "gamemode",
        ));
    }
    let name = parser.get_reader().read_unquoted_string();
    let Some(expected) = [
        GameType::Survival,
        GameType::Creative,
        GameType::Adventure,
        GameType::Spectator,
    ]
    .into_iter()
    .find(|g| g.get_name() == name) else {
        return Err(rollback_and_throw(
            parser,
            start,
            &ERROR_GAME_MODE_INVALID,
            name,
        ));
    };
    parser.set_includes_entities(false);
    parser.add_predicate(Arc::new(move |_, e| match host::player_game_mode(e) {
        Some(current) => (current == expected) ^ inverted,
        None => false,
    }));
    parser.gamemode_option().mark_parsed_element(inverted);
    Ok(())
}

fn team(parser: &mut EntitySelectorParser<'_>) -> Result<(), CommandSyntaxException> {
    let start = parser.get_reader().get_cursor();
    let inverted = parser.should_invert_value();
    let expected = parser.get_reader().read_unquoted_string();
    if !parser.team_option().can_parse_element(inverted) {
        return Err(rollback_and_throw(
            parser,
            start,
            &ERROR_INAPPLICABLE_OPTION,
            "team",
        ));
    }
    parser.add_predicate(Arc::new(move |ctx, e| {
        let current_name = host::entity_team_name(ctx.server, e).unwrap_or_default();
        (current_name == expected) != inverted
    }));
    parser.team_option().mark_parsed_element(inverted);
    Ok(())
}

fn type_(parser: &mut EntitySelectorParser<'_>) -> Result<(), CommandSyntaxException> {
    let state = parser.type_option().clone();
    parser.set_suggestions(Suggest::Custom(Arc::new(move |mut b| {
        let all: Vec<Identifier> = EntityType::iter().map(|t| t.identifier()).collect();
        if state.can_parse_negative_element() {
            suggest_resource_prefixed_into(all.clone(), &mut b, "!");
        }
        if state.can_parse_positive_element() {
            suggest_resource_into(all, &mut b);
        }
        if state.can_parse_any_tag() {
            let allowed_tags: Vec<Identifier> = mc_data::tags::list("entity_type")
                .iter()
                .filter_map(|n| Identifier::try_parse(n))
                .filter(|t| state.can_parse_tag(t))
                .collect();
            if !allowed_tags.is_empty() {
                suggest_resource_prefixed_into(allowed_tags.clone(), &mut b, "#");
                suggest_resource_prefixed_into(allowed_tags, &mut b, "!#");
            }
        }
        b.build()
    })));
    let start = parser.get_reader().get_cursor();
    let inverted = parser.should_invert_value();
    if parser.is_tag() {
        if !parser.type_option().can_parse_any_tag() {
            return Err(rollback_and_throw(
                parser,
                start,
                &ERROR_INAPPLICABLE_OPTION,
                "type",
            ));
        }
        let id = read_identifier(parser.get_reader())?;
        if !parser.type_option().can_parse_tag(&id) {
            return Err(rollback_and_throw(
                parser,
                start,
                &ERROR_INAPPLICABLE_OPTION,
                "type",
            ));
        }
        let tag_id = id.clone();
        parser.add_predicate(Arc::new(move |_, e| {
            mc_data::tags::get::<EntityType>(&tag_id.to_string())
                .is_some_and(|t| t.contains(e.get_type()))
                != inverted
        }));
        parser.type_option().mark_parsed_tag(id);
    } else {
        if !parser.type_option().can_parse_element(inverted) {
            return Err(rollback_and_throw(
                parser,
                start,
                &ERROR_INAPPLICABLE_OPTION,
                "type",
            ));
        }
        let id = read_identifier(parser.get_reader())?;
        let Some(entity_type) = EntityType::by_name(&id.to_string()) else {
            return Err(rollback_and_throw(
                parser,
                start,
                &ERROR_ENTITY_TYPE_INVALID,
                id.to_string(),
            ));
        };
        if entity_type == EntityType::PLAYER && !inverted {
            parser.set_includes_entities(false);
        }
        parser.add_predicate(Arc::new(move |_, e| {
            (e.get_type() == entity_type) != inverted
        }));
        if !inverted {
            parser.limit_to_type(entity_type);
        }
        parser.type_option().mark_parsed_element(inverted);
    }
    Ok(())
}

fn tag(parser: &mut EntitySelectorParser<'_>) -> Result<(), CommandSyntaxException> {
    let inverted = parser.should_invert_value();
    let tag = parser.get_reader().read_unquoted_string();
    parser.add_predicate(Arc::new(move |_, e| {
        let tags = &e.base().tags;
        if tag.is_empty() {
            tags.is_empty() != inverted
        } else {
            tags.contains(&tag) != inverted
        }
    }));
    Ok(())
}

fn nbt(parser: &mut EntitySelectorParser<'_>) -> Result<(), CommandSyntaxException> {
    let inverted = parser.should_invert_value();
    let pattern =
        crate::commands::arguments::nbt_arguments::parse_compound_as_argument(parser.get_reader())?;
    parser.add_predicate(Arc::new(move |ctx, e| {
        let mut output = mc_nbt::CompoundTag::new();
        let level = ctx.server.level(ctx.dimension);
        let lookup = |id| level.and_then(|l| l.get_entity(id));
        crate::world::entity::entity::save_without_id(e, &mut output, &lookup);
        if e.is_player()
            && let Some(selected) = host::player_selected_item_nbt(e)
        {
            output.put("SelectedItem", selected);
        }
        compare_nbt(
            Some(&Tag::Compound(pattern.clone())),
            Some(&Tag::Compound(output)),
            true,
        ) != inverted
    }));
    Ok(())
}

fn scores(parser: &mut EntitySelectorParser<'_>) -> Result<(), CommandSyntaxException> {
    let reader: &mut StringReader = parser.get_reader();
    let mut expected: Vec<(String, Ints)> = Vec::new();
    reader.expect('{')?;
    reader.skip_whitespace();
    while reader.can_read() && reader.peek() != '}' {
        reader.skip_whitespace();
        let name = reader.read_unquoted_string();
        reader.skip_whitespace();
        reader.expect('=')?;
        reader.skip_whitespace();
        let value = Ints::from_reader(reader)?;
        // `Map.put`: a repeated objective replaces the earlier bounds.
        match expected.iter_mut().find(|(n, _)| *n == name) {
            Some(entry) => entry.1 = value,
            None => expected.push((name, value)),
        }
        reader.skip_whitespace();
        if reader.can_read() && reader.peek() == ',' {
            reader.skip();
        }
    }
    reader.expect('}')?;
    if !expected.is_empty() {
        parser.add_predicate(Arc::new(move |ctx, e| {
            host::entity_scores_match(ctx.server, e, &expected)
        }));
    }
    parser.scores_option().mark_parsed();
    Ok(())
}

fn advancements(parser: &mut EntitySelectorParser<'_>) -> Result<(), CommandSyntaxException> {
    let reader: &mut StringReader = parser.get_reader();
    let mut expected: Vec<(Identifier, AdvancementExpectation)> = Vec::new();
    reader.expect('{')?;
    reader.skip_whitespace();
    while reader.can_read() && reader.peek() != '}' {
        reader.skip_whitespace();
        let name = read_identifier(reader)?;
        reader.skip_whitespace();
        reader.expect('=')?;
        reader.skip_whitespace();
        let entry = if reader.can_read() && reader.peek() == '{' {
            let mut progress: Vec<(String, bool)> = Vec::new();
            reader.skip_whitespace();
            reader.expect('{')?;
            reader.skip_whitespace();
            while reader.can_read() && reader.peek() != '}' {
                reader.skip_whitespace();
                let criterion = reader.read_unquoted_string();
                reader.skip_whitespace();
                reader.expect('=')?;
                reader.skip_whitespace();
                let value = reader.read_boolean()?;
                match progress.iter_mut().find(|(c, _)| *c == criterion) {
                    Some(entry) => entry.1 = value,
                    None => progress.push((criterion, value)),
                }
                reader.skip_whitespace();
                if reader.can_read() && reader.peek() == ',' {
                    reader.skip();
                }
            }
            reader.skip_whitespace();
            reader.expect('}')?;
            reader.skip_whitespace();
            AdvancementExpectation::Criteria(progress)
        } else {
            AdvancementExpectation::Done(reader.read_boolean()?)
        };
        match expected.iter_mut().find(|(n, _)| *n == name) {
            Some(slot) => slot.1 = entry,
            None => expected.push((name, entry)),
        }
        reader.skip_whitespace();
        if reader.can_read() && reader.peek() == ',' {
            reader.skip();
        }
    }
    reader.expect('}')?;
    if !expected.is_empty() {
        parser.add_predicate(Arc::new(move |ctx, e| {
            e.is_player() && host::advancements_match(ctx.server, e, &expected)
        }));
        parser.set_includes_entities(false);
    }
    parser.advancements_option().mark_parsed();
    Ok(())
}

fn predicate(parser: &mut EntitySelectorParser<'_>) -> Result<(), CommandSyntaxException> {
    let inverted = parser.should_invert_value();
    let id = read_identifier(parser.get_reader())?;
    parser.add_predicate(Arc::new(move |ctx, e| {
        inverted ^ host::loot_predicate_matches(ctx.server, ctx.dimension, e, &id)
    }));
    Ok(())
}
