//! `GiveCommand`.
use mc_game_core::commands::arguments::item::{ItemArgument, ItemInput};
use mc_game_core::commands::arguments::selector::EntityRef;
use mc_game_core::commands::arguments::EntityArgument;
use mc_game_core::commands::message::message;
use mc_game_core::commands::{CommandSourceStack, LEVEL_GAMEMASTERS, argument, has_permission, host, literal};
use mc_game_core::world::item::ItemStackExt;
use mc_game_core::{register_command, translatable};
use mc_brigadier::arguments::{IntegerArgumentType, integer_min};
use mc_brigadier::{ArgumentBuilder, CommandContext, CommandSyntaxException, Dynamic2CommandExceptionType};
use mc_text::Component;
use std::sync::LazyLock;

static TOO_MANY_ITEMS: LazyLock<Dynamic2CommandExceptionType<i32, Component>> = LazyLock::new(|| {
    Dynamic2CommandExceptionType::new(|max_count: i32, item: Component| message(translatable!("commands.give.failed.toomanyitems", max_count, item)))
});

const MAX_ALLOWED_ITEMSTACKS: i32 = 100;

register_command!("GiveCommand", |ctx| {
    fn give(c: &CommandContext<CommandSourceStack>, count: i32) -> Result<i32, CommandSyntaxException> {
        give_item(c.get_source(), ItemArgument::get_item(c, "item"), EntityArgument::get_players(c, "targets")?, count)
    }
    ctx.dispatcher.register(
        literal("give").requires(has_permission(LEVEL_GAMEMASTERS)).then(
            argument("targets", EntityArgument::players()).then(
                argument("item", ItemArgument::item(ctx.build_context))
                    .executes(|c| give(c, 1))
                    .then(argument("count", integer_min(1)).executes(|c| give(c, IntegerArgumentType::get_integer(c, "count")))),
            ),
        ),
    );
});

fn give_item(
    source: &CommandSourceStack,
    input: ItemInput,
    players: Vec<EntityRef>,
    count: i32,
) -> Result<i32, CommandSyntaxException> {
    let prototype = input.create_item_stack(1)?;
    let max_stack_size = prototype.get_max_stack_size();
    let max_allowed_count = max_stack_size * MAX_ALLOWED_ITEMSTACKS;
    if count > max_allowed_count {
        return Err(TOO_MANY_ITEMS.create(max_allowed_count, prototype.get_display_name()));
    }
    for player in &players {
        source.with_server(|server| host::give_item(server, player, &prototype, max_stack_size, count));
    }
    // `CommandResponseTracker`: every player is tracked with value 1, so the total is `players.len()` and
    // `getPlayers` guarantees at least one (zero would throw the tracker's `onZero`, which is unset here).
    let total = players.len() as i32;
    source.send_success(
        || match players.as_slice() {
            [only] => {
                let name = source.with_server(|server| host::get_entity(server, only).map_or_else(Component::empty, |e| host::entity_display_name(server, e)));
                translatable!("commands.give.success.single", count, prototype.get_display_name(), name)
            }
            _ => translatable!("commands.give.success.multiple", count, prototype.get_display_name(), total),
        },
        true,
    );
    Ok(total)
}
