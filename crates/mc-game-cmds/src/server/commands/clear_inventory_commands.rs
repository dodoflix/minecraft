//! `ClearInventoryCommands`: `/clear [targets [item [maxCount]]]`.
use super::command_response_tracker::{CommandResponseTracker, ElementType};
use mc_brigadier::arguments::{IntegerArgumentType, integer_min};
use mc_brigadier::{ArgumentBuilder, CommandSyntaxException, DynamicCommandExceptionType};
use mc_game_core::commands::arguments::EntityArgument;
use mc_game_core::commands::arguments::item::{ItemPredicateArgument, ItemPredicateResult};
use mc_game_core::commands::arguments::selector::EntityRef;
use mc_game_core::commands::message::message;
use mc_game_core::commands::{
    CommandSourceStack, LEVEL_GAMEMASTERS, argument, has_permission, host, literal,
};
use mc_game_core::server::commands::item::slots::clear_or_count_matching_items;
use mc_game_core::{register_command, translatable};
use mc_protocol::components::ItemStack;
use mc_text::Component;
use std::sync::LazyLock;

static ERROR_SINGLE: LazyLock<DynamicCommandExceptionType<Component>> = LazyLock::new(|| {
    DynamicCommandExceptionType::new(|name: Component| {
        message(translatable!("clear.failed.single", name))
    })
});
static ERROR_MULTIPLE: LazyLock<DynamicCommandExceptionType<i32>> = LazyLock::new(|| {
    DynamicCommandExceptionType::new(|count: i32| {
        message(translatable!("clear.failed.multiple", count))
    })
});

register_command!("ClearInventoryCommands", |ctx| {
    ctx.dispatcher.register(
        literal("clear")
            .requires(has_permission(LEVEL_GAMEMASTERS))
            .executes(|c| {
                let source = c.get_source();
                let player = source.get_player_or_exception()?;
                let target = EntityRef {
                    dimension: source.get_level().clone(),
                    id: player.id,
                };
                clear_inventory(source, &[target], &|_| true, -1)
            })
            .then(
                argument("targets", EntityArgument::players())
                    .executes(|c| {
                        clear_inventory(
                            c.get_source(),
                            &EntityArgument::get_players(c, "targets")?,
                            &|_| true,
                            -1,
                        )
                    })
                    .then(
                        argument(
                            "item",
                            ItemPredicateArgument::item_predicate(ctx.build_context),
                        )
                        .executes(|c| {
                            let predicate: ItemPredicateResult =
                                ItemPredicateArgument::get_item_predicate(c, "item");
                            clear_inventory(
                                c.get_source(),
                                &EntityArgument::get_players(c, "targets")?,
                                &|s| predicate.test(s),
                                -1,
                            )
                        })
                        .then(
                            argument("maxCount", integer_min(0)).executes(|c| {
                                let predicate =
                                    ItemPredicateArgument::get_item_predicate(c, "item");
                                clear_inventory(
                                    c.get_source(),
                                    &EntityArgument::get_players(c, "targets")?,
                                    &|s: &ItemStack| predicate.test(s),
                                    IntegerArgumentType::get_integer(c, "maxCount"),
                                )
                            }),
                        ),
                    ),
            ),
    );
});

fn display_name(source: &CommandSourceStack, player: &EntityRef) -> Component {
    source.with_server(|server| {
        host::get_entity(server, player)
            .map_or_else(Component::empty, |e| host::entity_display_name(server, e))
    })
}

/// `clearInventory(source, players, predicate, maxCount)`.
fn clear_inventory(
    source: &CommandSourceStack,
    players: &[EntityRef],
    predicate: &dyn Fn(&ItemStack) -> bool,
    max_count: i32,
) -> Result<i32, CommandSyntaxException> {
    let mut tracker = CommandResponseTracker::<EntityRef>::create();
    let counting_only = max_count == 0;
    for player in players {
        let count = source.with_server(|server| {
            server.level_mut(&player.dimension).map_or(0, |level| {
                clear_or_count_matching_items(level, player.id, predicate, counting_only, max_count)
            })
        });
        tracker.track(player, count);
    }
    if tracker.total_value() == 0 {
        return Err(tracker.dispatch(
            ElementType::Any,
            |player, _| ERROR_SINGLE.create(display_name(source, player)),
            |player_count, _| ERROR_MULTIPLE.create(player_count),
        ));
    }
    let key = if counting_only {
        "commands.clear.test"
    } else {
        "commands.clear.success"
    };
    tracker.send_feedback(
        source,
        true,
        ElementType::NonZero,
        None::<fn() -> CommandSyntaxException>,
        |player, total| {
            Component::translatable(
                format!("{key}.single"),
                vec![total.into(), display_name(source, player).into()],
            )
        },
        |player_count, total| {
            Component::translatable(
                format!("{key}.multiple"),
                vec![total.into(), player_count.into()],
            )
        },
    )
}
