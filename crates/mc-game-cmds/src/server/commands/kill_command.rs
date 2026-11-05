//! `KillCommand`.
use mc_game_core::commands::arguments::EntityArgument;
use mc_game_core::commands::arguments::selector::EntityRef;
use super::command_response_tracker::CommandResponseTracker;
use mc_game_core::commands::{CommandSourceStack, LEVEL_GAMEMASTERS, argument, has_permission, host, literal};
use mc_game_core::{register_command, translatable};
use mc_brigadier::{ArgumentBuilder, CommandSyntaxException};
use mc_text::Component;

register_command!("KillCommand", |ctx| {
    ctx.dispatcher.register(
        literal("kill")
            .requires(has_permission(LEVEL_GAMEMASTERS))
            .executes(|c| kill(c.get_source(), vec![host::source_entity_ref(c.get_source())?]))
            .then(argument("targets", EntityArgument::entities()).executes(|c| kill(c.get_source(), EntityArgument::get_entities(c, "targets")?))),
    );
});

fn kill(source: &CommandSourceStack, victims: Vec<EntityRef>) -> Result<i32, CommandSyntaxException> {
    // The tracked element carries the display name taken before the kill: a killed non-living entity leaves the level at once.
    let mut tracker = CommandResponseTracker::<Component>::create();
    for entity in &victims {
        let name = host::display_name_of(source, entity);
        source.with_server(|server| host::kill_entity(server, entity));
        tracker.track_one(&name);
    }
    tracker.send_feedback_default(
        source,
        true,
        None,
        |name, _| translatable!("commands.kill.success.single", name.clone()),
        |entity_count, _| translatable!("commands.kill.success.multiple", entity_count),
    )
}
