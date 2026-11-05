//! `SeedCommand`.
use mc_game_core::commands::{LEVEL_ALL, LEVEL_GAMEMASTERS, CommandSelection, has_permission, literal};
use mc_brigadier::ArgumentBuilder;
use mc_game_core::{register_command, translatable};
use mc_text::util::copy_on_click_text;

register_command!("SeedCommand", |ctx| {
    let check_permissions = ctx.selection != CommandSelection::Integrated;
    ctx.dispatcher.register(
        literal("seed").requires(has_permission(if check_permissions { LEVEL_GAMEMASTERS } else { LEVEL_ALL })).executes(|c| {
            let seed = c.get_source().with_server(|server| server.world_gen_settings.options.seed);
            let seed_text = copy_on_click_text(&seed.to_string());
            c.get_source().send_success(|| translatable!("commands.seed.success", seed_text), false);
            Ok(seed as i32)
        }),
    );
});
