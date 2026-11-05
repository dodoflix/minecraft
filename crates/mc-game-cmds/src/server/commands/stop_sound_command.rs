//! `StopSoundCommand`.
use super::play_sound_command::SOUND_SOURCES;
use mc_game_core::commands::arguments::selector::EntityRef;
use mc_game_core::commands::arguments::{EntityArgument, IdentifierArgument};
use mc_game_core::commands::synchronization::suggestion_providers;
use mc_game_core::commands::{CommandSourceStack, LEVEL_GAMEMASTERS, argument, has_permission, host, literal};
use mc_game_core::world::entity::ServerPlayer;
use mc_game_core::{register_command, translatable};
use mc_brigadier::{ArgumentBuilder, CommandContext, CommandSyntaxException};
use mc_core::ident::Identifier;
use mc_protocol::packets::play::{ClientboundStopSoundPacket, SoundSource};
use mc_text::Component;

register_command!("StopSoundCommand", |ctx| {
    fn run(c: &CommandContext<CommandSourceStack>, source: Option<(SoundSource, &'static str)>, with_sound: bool) -> Result<i32, CommandSyntaxException> {
        let sound = with_sound.then(|| IdentifierArgument::get_id(c, "sound"));
        Ok(stop_sound(c.get_source(), &EntityArgument::get_players(c, "targets")?, source, sound))
    }
    let sound_argument = || argument("sound", IdentifierArgument::id()).suggests_provider(Some(suggestion_providers::available_sounds()));
    let mut target = argument("targets", EntityArgument::players()).executes(|c| run(c, None, false)).then(
        literal("*").then(sound_argument().executes(|c| run(c, None, true))),
    );
    for entry in SOUND_SOURCES {
        target = target.then(literal(entry.1).executes(move |c| run(c, Some(entry), false)).then(sound_argument().executes(move |c| run(c, Some(entry), true))));
    }
    ctx.dispatcher.register(literal("stopsound").requires(has_permission(LEVEL_GAMEMASTERS)).then(target));
});

fn stop_sound(source: &CommandSourceStack, targets: &[EntityRef], sound_source: Option<(SoundSource, &'static str)>, sound: Option<Identifier>) -> i32 {
    let packet = ClientboundStopSoundPacket { name: sound.clone(), source: sound_source.map(|(s, _)| s) };
    source.with_server(|server| {
        for player in targets {
            if let Some(target) = host::get_entity(server, player).and_then(|e| e.as_any().downcast_ref::<ServerPlayer>()) {
                target.send(&packet);
            }
        }
    });
    source.send_success(
        || match (sound_source, sound) {
            (Some((_, name)), Some(sound)) => translatable!("commands.stopsound.success.source.sound", Component::literal(sound.to_string()), name),
            (Some((_, name)), None) => translatable!("commands.stopsound.success.source.any", name),
            (None, Some(sound)) => translatable!("commands.stopsound.success.sourceless.sound", Component::literal(sound.to_string())),
            (None, None) => translatable!("commands.stopsound.success.sourceless.any"),
        },
        true,
    );
    targets.len() as i32
}
