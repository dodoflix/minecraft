//! `PlaySoundCommand`.
use super::command_response_tracker::{CommandResponseTracker, ElementType};
use mc_game_core::commands::arguments::coordinates::Vec3Argument;
use mc_game_core::commands::arguments::selector::EntityRef;
use mc_game_core::commands::arguments::{EntityArgument, IdentifierArgument};
use mc_game_core::commands::message::simple_translatable;
use mc_game_core::commands::synchronization::suggestion_providers;
use mc_game_core::commands::{CommandSourceStack, LEVEL_GAMEMASTERS, argument, has_permission, host, literal};
use mc_game_core::world::entity::ServerPlayer;
use mc_game_core::{register_command, translatable};
use mc_brigadier::arguments::{FloatArgumentType, float_arg_min, float_arg_range};
use mc_brigadier::{ArgumentBuilder, CommandContext, CommandSyntaxException, LiteralArgumentBuilder, SimpleCommandExceptionType};
use mc_core::ident::Identifier;
use mc_core::phys::Vec3;
use mc_core::random::RandomSource;
use mc_protocol::packets::play::{ClientboundSoundPacket, DirectSoundEvent, SoundEventHolder, SoundSource};
use mc_text::Component;
use std::sync::LazyLock;

static ERROR_TOO_FAR: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("commands.playsound.failed"));

/// `SoundSource.values()` with `getName()`.
pub(super) const SOUND_SOURCES: [(SoundSource, &str); 11] = [
    (SoundSource::Master, "master"),
    (SoundSource::Music, "music"),
    (SoundSource::Records, "record"),
    (SoundSource::Weather, "weather"),
    (SoundSource::Blocks, "block"),
    (SoundSource::Hostile, "hostile"),
    (SoundSource::Neutral, "neutral"),
    (SoundSource::Players, "player"),
    (SoundSource::Ambient, "ambient"),
    (SoundSource::Voice, "voice"),
    (SoundSource::Ui, "ui"),
];

register_command!("PlaySoundCommand", |ctx| {
    let mut name = argument("sound", IdentifierArgument::id())
        .suggests_provider(Some(suggestion_providers::available_sounds()))
        .executes(|c| play_to_caller(c, SoundSource::Master));
    for (source, source_name) in SOUND_SOURCES {
        name = name.then(source_literal(source, source_name));
    }
    ctx.dispatcher.register(literal("playsound").requires(has_permission(LEVEL_GAMEMASTERS)).then(name));
});

fn play_to_caller(c: &CommandContext<CommandSourceStack>, source: SoundSource) -> Result<i32, CommandSyntaxException> {
    play_sound(c.get_source(), calling_player(c.get_source()), IdentifierArgument::get_id(c, "sound"), source, c.get_source().get_position(), 1.0, 1.0, 0.0)
}

/// `depth`: how many of `pos`, `volume`, `pitch`, `minVolume` the executed command line has.
fn play_to_targets(c: &CommandContext<CommandSourceStack>, source: SoundSource, depth: u8) -> Result<i32, CommandSyntaxException> {
    let float = |name: &str, default: f32, at: u8| if depth >= at { *c.get_argument::<f32>(name) } else { default };
    play_sound(
        c.get_source(),
        EntityArgument::get_players(c, "targets")?,
        IdentifierArgument::get_id(c, "sound"),
        source,
        if depth >= 1 { Vec3Argument::get_vec3(c, "pos") } else { c.get_source().get_position() },
        float("volume", 1.0, 2),
        float("pitch", 1.0, 3),
        float("minVolume", 0.0, 4),
    )
}

fn source_literal(source: SoundSource, source_name: &str) -> LiteralArgumentBuilder<CommandSourceStack> {
    literal(source_name).executes(move |c| play_to_caller(c, source)).then(
        argument("targets", EntityArgument::players()).executes(move |c| play_to_targets(c, source, 0)).then(
            argument("pos", Vec3Argument::vec3()).executes(move |c| play_to_targets(c, source, 1)).then(
                argument("volume", float_arg_min(0.0)).executes(move |c| play_to_targets(c, source, 2)).then(
                    argument("pitch", float_arg_range(0.0, 2.0)).executes(move |c| play_to_targets(c, source, 3)).then(
                        argument("minVolume", float_arg_range(0.0, 1.0)).executes(move |c| play_to_targets(c, source, 4)),
                    ),
                ),
            ),
        ),
    )
}

/// `getCallingPlayerAsCollection(source.getPlayer())`.
fn calling_player(source: &CommandSourceStack) -> Vec<EntityRef> {
    let Some(player) = source.get_player() else { return Vec::new() };
    source.with_server(|server| {
        server.player_list.get_players().into_iter().find(|p| p.id == player.id).map(|p| EntityRef { dimension: p.dimension(), id: p.id }).into_iter().collect()
    })
}

#[allow(clippy::too_many_arguments)]
fn play_sound(
    source: &CommandSourceStack,
    players: Vec<EntityRef>,
    sound: Identifier,
    sound_source: SoundSource,
    position: Vec3,
    volume: f32,
    pitch: f32,
    min_volume: f32,
) -> Result<i32, CommandSyntaxException> {
    // `SoundEvent.createVariableRangeEvent(sound)` as a direct holder: `getRange(volume)` is a float.
    let range = if volume > 1.0 { 16.0 * volume } else { 16.0f32 };
    let max_dist_sqr = f64::from(range * range);
    let mut tracker = CommandResponseTracker::<EntityRef>::create();
    source.with_server(|server| {
        let Some(seed) = server.level_mut(source.get_level()).map(|level| level.tick_state.random.next_long()) else { return };
        for player in &players {
            if player.dimension != *source.get_level() {
                continue;
            }
            let Some(entity) = host::get_entity(server, player) else { continue };
            let Some(target) = entity.as_any().downcast_ref::<ServerPlayer>() else { continue };
            let p = entity.position();
            let (delta_x, delta_y, delta_z) = (position.x - p.x, position.y - p.y, position.z - p.z);
            let dist_sqr = delta_x * delta_x + delta_y * delta_y + delta_z * delta_z;
            let mut local_position = position;
            let mut local_volume = volume;
            if dist_sqr > max_dist_sqr {
                if min_volume <= 0.0 {
                    continue;
                }
                let distance = dist_sqr.sqrt();
                local_position = Vec3::new(p.x + delta_x / distance * 2.0, p.y + delta_y / distance * 2.0, p.z + delta_z / distance * 2.0);
                local_volume = min_volume;
            }
            target.send(&ClientboundSoundPacket::new(
                SoundEventHolder::Direct(DirectSoundEvent { location: sound.clone(), fixed_range: None }),
                sound_source,
                local_position.x,
                local_position.y,
                local_position.z,
                local_volume,
                pitch,
                seed,
            ));
            tracker.track(player, 1);
        }
    });
    let sound_arg = Component::literal(sound.to_string());
    let (single_arg, multiple_arg) = (sound_arg.clone(), sound_arg);
    tracker.send_feedback(
        source,
        true,
        ElementType::NonZero,
        Some(|| ERROR_TOO_FAR.create()),
        |player, _| {
            let name = source.with_server(|server| host::get_entity(server, player).map_or_else(Component::empty, |e| host::entity_display_name(server, e)));
            translatable!("commands.playsound.success.single", single_arg, name)
        },
        |count, _| translatable!("commands.playsound.success.multiple", multiple_arg, count),
    )
}
