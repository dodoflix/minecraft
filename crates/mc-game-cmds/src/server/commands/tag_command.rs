//! `TagCommand`.
use super::score_tracker::ScoreTracker as CommandResponseTracker;
use mc_brigadier::arguments::StringArgumentType;
use mc_brigadier::{ArgumentBuilder, CommandSyntaxException, SimpleCommandExceptionType};
use mc_game_core::commands::arguments::EntityArgument;
use mc_game_core::commands::arguments::selector::EntityRef;
use mc_game_core::commands::message::simple_translatable;
use mc_game_core::commands::shared_suggestion_provider::suggest;
use mc_game_core::commands::{
    CommandSourceStack, LEVEL_GAMEMASTERS, argument, has_permission, host, literal,
};
use mc_game_core::{register_command, translatable};
use mc_text::Component;
use mc_text::util::format_string_list;
use std::sync::LazyLock;

static ERROR_ADD_FAILED: LazyLock<SimpleCommandExceptionType> =
    LazyLock::new(|| simple_translatable("commands.tag.add.failed"));
static ERROR_REMOVE_FAILED: LazyLock<SimpleCommandExceptionType> =
    LazyLock::new(|| simple_translatable("commands.tag.remove.failed"));

register_command!("TagCommand", |ctx| {
    ctx.dispatcher.register(
        literal("tag")
            .requires(has_permission(LEVEL_GAMEMASTERS))
            .then(
                argument("targets", EntityArgument::entities())
                    .then(literal("add").then(
                        argument("name", StringArgumentType::word()).executes(|c| {
                            add_tag(
                                c.get_source(),
                                &EntityArgument::get_entities(c, "targets")?,
                                StringArgumentType::get_string(c, "name"),
                            )
                        }),
                    ))
                    .then(
                        literal("remove").then(
                            argument("name", StringArgumentType::word())
                                .suggests(|c, p| {
                                    let entities = EntityArgument::get_entities(c, "targets")?;
                                    Ok(suggest(get_tags(c.get_source(), &entities), p))
                                })
                                .executes(|c| {
                                    remove_tag(
                                        c.get_source(),
                                        &EntityArgument::get_entities(c, "targets")?,
                                        StringArgumentType::get_string(c, "name"),
                                    )
                                }),
                        ),
                    )
                    .then(literal("list").executes(|c| {
                        list_tags(c.get_source(), &EntityArgument::get_entities(c, "targets")?)
                    })),
            ),
    );
});

/// `entity.getDisplayName()` of the (live) entity.
fn display_name(source: &CommandSourceStack, entity: &EntityRef) -> Component {
    source.with_server(|server| {
        host::get_entity(server, entity)
            .map_or_else(Component::empty, |e| host::entity_display_name(server, e))
    })
}

/// `entity.entityTags()`.
fn entity_tags(source: &CommandSourceStack, entity: &EntityRef) -> Vec<String> {
    source.with_server(|server| {
        host::get_entity(server, entity)
            .map(|e| {
                e.base()
                    .tags
                    .iter_java_order()
                    .into_iter()
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default()
    })
}

/// `getTags(entities)`: the union of the entities' tags.
fn get_tags(source: &CommandSourceStack, entities: &[EntityRef]) -> Vec<String> {
    let mut result: Vec<String> = Vec::new();
    for entity in entities {
        for tag in entity_tags(source, entity) {
            if !result.contains(&tag) {
                result.push(tag);
            }
        }
    }
    result
}

fn add_tag(
    source: &CommandSourceStack,
    targets: &[EntityRef],
    name: &str,
) -> Result<i32, CommandSyntaxException> {
    let mut response = CommandResponseTracker::new();
    for entity in targets {
        let added = source.with_server(|server| {
            server
                .level_mut(&entity.dimension)
                .and_then(|l| l.get_entity_mut(entity.id))
                .is_some_and(|e| e.base_mut().add_tag(name))
        });
        response.track_bool(entity.clone(), added);
    }
    response.send_feedback(
        source,
        true,
        Some(ERROR_ADD_FAILED.create()),
        |entity, _| {
            translatable!(
                "commands.tag.add.success.single",
                name.to_owned(),
                display_name(source, entity)
            )
        },
        |count, _| translatable!("commands.tag.add.success.multiple", name.to_owned(), count),
    )
}

fn remove_tag(
    source: &CommandSourceStack,
    targets: &[EntityRef],
    name: &str,
) -> Result<i32, CommandSyntaxException> {
    let mut response = CommandResponseTracker::new();
    for entity in targets {
        let removed = source.with_server(|server| {
            server
                .level_mut(&entity.dimension)
                .and_then(|l| l.get_entity_mut(entity.id))
                .is_some_and(|e| e.base_mut().remove_tag(name))
        });
        response.track_bool(entity.clone(), removed);
    }
    response.send_feedback(
        source,
        true,
        Some(ERROR_REMOVE_FAILED.create()),
        |entity, _| {
            translatable!(
                "commands.tag.remove.success.single",
                name.to_owned(),
                display_name(source, entity)
            )
        },
        |count, _| {
            translatable!(
                "commands.tag.remove.success.multiple",
                name.to_owned(),
                count
            )
        },
    )
}

fn list_tags(
    source: &CommandSourceStack,
    targets: &[EntityRef],
) -> Result<i32, CommandSyntaxException> {
    let mut response = CommandResponseTracker::new();
    let mut tags: Vec<String> = Vec::new();
    for entity in targets {
        let entity_tags = entity_tags(source, entity);
        for tag in &entity_tags {
            if !tags.contains(tag) {
                tags.push(tag.clone());
            }
        }
        response.track_value(entity.clone(), entity_tags.len() as i32);
    }
    if tags.is_empty() {
        response.send_feedback(
            source,
            false,
            None,
            |entity, _| {
                translatable!(
                    "commands.tag.list.single.empty",
                    display_name(source, entity)
                )
            },
            |count, _| translatable!("commands.tag.list.multiple.empty", count),
        )?;
    } else {
        response.send_feedback(
            source,
            false,
            None,
            |entity, _| {
                translatable!(
                    "commands.tag.list.single.success",
                    display_name(source, entity),
                    tags.len() as i32,
                    format_string_list(&tags)
                )
            },
            |count, _| {
                translatable!(
                    "commands.tag.list.multiple.success",
                    count,
                    tags.len() as i32,
                    format_string_list(&tags)
                )
            },
        )?;
    }
    Ok(tags.len() as i32)
}
