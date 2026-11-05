//! `ComponentUtils.resolve` (`ResolutionContext`): substitutes `selector`, `score` and `nbt` contents.

use super::arguments::selector::{EntityRef, EntitySelectorParser};
use super::arguments::selector::entity_selector::ERROR_NOT_SINGLE_ENTITY;
use super::{host, CommandSourceStack};
use mc_brigadier::{CommandSyntaxException, StringReader};
use mc_nbt::Tag;
use mc_text::component::{ComponentContents, DataSource, ScoreName, TranslatableArg};
use mc_text::events::HoverEvent;
use mc_text::nbt_text::{Styling, TextComponentTagVisitor};
use mc_text::util::{default_no_style_separator, default_separator, ellipsis, format_list};
use mc_text::Component;
use std::cell::Cell;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LimitBehavior {
    DiscardRemaining,
    StopProcessingAndCopyRemaining,
}

/// `ResolutionContext`.
pub struct ResolutionContext<'a> {
    pub source: Option<&'a CommandSourceStack>,
    /// `defaultScoreboardEntity`.
    pub default_scoreboard_entity: Option<EntityRef>,
    pub depth_limit: i32,
    pub depth_limit_behavior: LimitBehavior,
    pub resolved_component_limit: i32,
    pub resolved_component_count: Cell<i32>,
}

impl<'a> ResolutionContext<'a> {
    /// `ResolutionContext.create(source)`.
    pub fn create(source: &'a CommandSourceStack) -> Self {
        Self {
            source: Some(source),
            default_scoreboard_entity: source.get_entity().map(|e| EntityRef { dimension: source.get_level().clone(), id: e.id }),
            depth_limit: 100,
            depth_limit_behavior: LimitBehavior::StopProcessingAndCopyRemaining,
            resolved_component_limit: 65536,
            resolved_component_count: Cell::new(0),
        }
    }

    /// `builder().withSource(source).withEntityOverride(entity).build()`.
    pub fn with_entity_override(source: &'a CommandSourceStack, entity: EntityRef) -> Self {
        Self { default_scoreboard_entity: Some(entity), ..Self::create(source) }
    }
}

/// `ComponentUtils.resolve(context, component)`.
pub fn resolve(context: &ResolutionContext<'_>, component: &Component) -> Result<Component, CommandSyntaxException> {
    resolve_at(context, component, 0)
}

fn resolve_opt(context: &ResolutionContext<'_>, component: Option<&Component>, depth: i32) -> Result<Option<Component>, CommandSyntaxException> {
    component.map(|c| resolve_at(context, c, depth)).transpose()
}

fn resolve_at(context: &ResolutionContext<'_>, component: &Component, depth: i32) -> Result<Component, CommandSyntaxException> {
    if depth > context.depth_limit {
        return Ok(match context.depth_limit_behavior {
            LimitBehavior::DiscardRemaining => ellipsis(),
            LimitBehavior::StopProcessingAndCopyRemaining => component.copy(),
        });
    }
    let mut result = resolve_contents(context, component.contents(), depth + 1)?;
    for sibling in component.siblings() {
        result = result.append(resolve_at(context, sibling, depth + 1)?);
    }
    let style = resolve_style(context, component.style(), depth)?;
    Ok(result.with_style(&style))
}

fn resolve_style(context: &ResolutionContext<'_>, style: &mc_text::Style, depth: i32) -> Result<mc_text::Style, CommandSyntaxException> {
    if let Some(HoverEvent::ShowText(text)) = style.hover_event() {
        let resolved = HoverEvent::show_text(resolve_at(context, text, depth + 1)?);
        return Ok(style.with_hover_event(Some(resolved)));
    }
    Ok(style.clone())
}

fn find_entities(source: &CommandSourceStack, selector: &str) -> Result<Vec<EntityRef>, CommandSyntaxException> {
    let mut reader = StringReader::new(selector);
    let parsed = EntitySelectorParser::new(&mut reader, true).parse()?;
    source.with_server(|server| parsed.find_entities(source, server))
}

fn resolve_contents(context: &ResolutionContext<'_>, contents: &ComponentContents, depth: i32) -> Result<Component, CommandSyntaxException> {
    let Some(source) = context.source else {
        // Without a source only the plain contents are copied (`selector`/`score`/`nbt` become empty).
        return Ok(match contents {
            ComponentContents::Selector(_) | ComponentContents::Score(_) | ComponentContents::Nbt(_) => Component::empty(),
            other => Component::new(other.clone()),
        });
    };
    match contents {
        ComponentContents::Translatable(t) => {
            let mut args = Vec::with_capacity(t.args.len());
            for arg in &t.args {
                args.push(match arg {
                    TranslatableArg::Component(c) => TranslatableArg::Component(resolve_at(context, c, depth)?),
                    other => other.clone(),
                });
            }
            Ok(Component::translatable_with_fallback(t.key.clone(), t.fallback.clone(), args))
        }
        ComponentContents::Selector(s) => {
            let separator = resolve_opt(context, s.separator.as_deref(), depth)?.unwrap_or_else(default_separator);
            let entities = find_entities(source, &s.selector)?;
            let formatted = source.with_server(|server| {
                format_list(entities, &separator, |r| host::get_entity(server, &r).map_or_else(Component::empty, |e| host::entity_display_name(server, e)))
            });
            Ok(formatted)
        }
        ComponentContents::Score(score) => {
            let holder = match &score.name {
                ScoreName::Selector(selector) => {
                    let entities = find_entities(source, selector)?;
                    match entities.len() {
                        0 => super::arguments::scores::ScoreHolder::for_name_only(selector),
                        1 => source
                            .with_server(|server| host::score_holder_of(server, &entities[0]))
                            .unwrap_or_else(|| super::arguments::scores::ScoreHolder::for_name_only(selector)),
                        _ => return Err(ERROR_NOT_SINGLE_ENTITY.create()),
                    }
                }
                ScoreName::Name(name) => super::arguments::scores::ScoreHolder::for_name_only(name),
            };
            Ok(source.with_server(|server| host::score_component(server, &holder, &score.objective)))
        }
        ComponentContents::Nbt(nbt) => {
            if context.resolved_component_count.get() > context.resolved_component_limit {
                return Ok(Component::empty());
            }
            context.resolved_component_count.set(context.resolved_component_count.get() + 1);
            let path = super::arguments::nbt_path_argument::NbtPath::of(&nbt.nbt_path).ok();
            let data = data_source_tags(source, &nbt.data_source)?;
            let elements: Vec<Tag> = data.iter().flat_map(|t| path.as_ref().and_then(|p| p.get(t).ok()).unwrap_or_default()).collect();
            let separator = resolve_opt(context, nbt.separator.as_deref(), depth)?.unwrap_or_else(default_no_style_separator);
            let mut parts: Vec<Component> = Vec::new();
            if nbt.interpreting {
                for tag in &elements {
                    match Component::from_nbt(tag).map_err(|e| e.0) {
                        Ok(c) => match resolve_at(context, &c, depth) {
                            Ok(r) => parts.push(r),
                            Err(e) => log::warn!("Failed to parse component: {tag}: {}", e.get_message()),
                        },
                        Err(e) => log::warn!("Failed to parse component: {tag}: {e}"),
                    }
                }
            } else {
                let styling = if nbt.plain { Styling::Plain } else { Styling::Rich };
                for tag in &elements {
                    parts.push(TextComponentTagVisitor::with_options("", styling, false).visit(tag));
                }
            }
            let mut iter = parts.into_iter();
            Ok(match iter.next() {
                None => Component::empty(),
                Some(first) => iter.fold(first, |acc, next| acc.append(separator.clone()).append(next)),
            })
        }
        ComponentContents::Object(o) => {
            let fallback = resolve_opt(context, o.fallback.as_deref(), depth)?;
            Ok(Component::object(o.contents.clone(), fallback))
        }
        other => Ok(Component::new(other.clone())),
    }
}

/// `DataSource.getData(source)`.
fn data_source_tags(source: &CommandSourceStack, data_source: &DataSource) -> Result<Vec<Tag>, CommandSyntaxException> {
    Ok(match data_source {
        DataSource::Entity(selector) => {
            let entities = find_entities(source, selector)?;
            source.with_server(|server| entities.iter().filter_map(|r| host::entity_data_with_selected_item(server, r)).map(Tag::Compound).collect())
        }
        DataSource::Block(coordinates) => {
            let mut reader = StringReader::new(coordinates.as_str());
            let parsed = mc_brigadier::ArgumentType::<CommandSourceStack>::parse(&super::arguments::coordinates::BlockPosArgument::block_pos(), &mut reader)?;
            let coords = parsed.downcast_ref::<super::arguments::coordinates::Coordinates>().copied();
            match coords {
                Some(c) => {
                    let pos = c.get_block_pos(source);
                    source.with_server(|server| host::block_entity_data(server, source.get_level(), pos)).into_iter().map(Tag::Compound).collect()
                }
                None => Vec::new(),
            }
        }
        DataSource::Storage(id) => vec![Tag::Compound(source.with_server(|server| host::command_storage_get(server, id)))],
    })
}

