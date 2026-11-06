//! `TitleCommand`.
use mc_brigadier::arguments::IntegerArgumentType;
use mc_brigadier::{ArgumentBuilder, CommandContext, CommandSyntaxException};
use mc_game_core::commands::arguments::selector::EntityRef;
use mc_game_core::commands::arguments::{ComponentArgument, EntityArgument, TimeArgument};
use mc_game_core::commands::component_utils::{ResolutionContext, resolve};
use mc_game_core::commands::{
    CommandSourceStack, LEVEL_GAMEMASTERS, argument, has_permission, host, literal,
};
use mc_game_core::server::players::PlayerHandle;
use mc_game_core::{register_command, translatable};
use mc_protocol::Packet;
use mc_protocol::packets::play::{
    ClientboundClearTitlesPacket, ClientboundSetActionBarTextPacket,
    ClientboundSetSubtitleTextPacket, ClientboundSetTitleTextPacket,
    ClientboundSetTitlesAnimationPacket,
};
use mc_text::Component;

type Ctx = CommandContext<CommandSourceStack>;

/// `CommandResponseTracker.Messages`: `<prefix>.single` (player display name) / `<prefix>.multiple` (count).
#[derive(Clone, Copy)]
struct Messages(&'static str);

/// Builds the packet of a `title|subtitle|actionbar` text for one player.
type SendText = fn(&PlayerHandle, Component);

register_command!("TitleCommand", |ctx| {
    let targets = |c: &Ctx| EntityArgument::get_players(c, "targets");
    let title_node = |name: &'static str, messages: Messages, send: SendText| {
        literal(name).then(
            argument("title", ComponentArgument::text_component()).executes(move |c| {
                show_title(
                    c.get_source(),
                    targets(c)?,
                    ComponentArgument::get_raw_component(c, "title"),
                    messages,
                    send,
                )
            }),
        )
    };
    ctx.dispatcher.register(
        literal("title")
            .requires(has_permission(LEVEL_GAMEMASTERS))
            .then(
                argument("targets", EntityArgument::players())
                    .then(literal("clear").executes(move |c| {
                        send_packet_to_players(
                            c.get_source(),
                            targets(c)?,
                            &ClientboundClearTitlesPacket { reset_times: false },
                            Messages("commands.title.cleared"),
                        )
                    }))
                    .then(literal("reset").executes(move |c| {
                        send_packet_to_players(
                            c.get_source(),
                            targets(c)?,
                            &ClientboundClearTitlesPacket { reset_times: true },
                            Messages("commands.title.reset"),
                        )
                    }))
                    .then(title_node(
                        "title",
                        Messages("commands.title.show.title"),
                        |p, text| p.connection.send(&ClientboundSetTitleTextPacket { text }),
                    ))
                    .then(title_node(
                        "subtitle",
                        Messages("commands.title.show.subtitle"),
                        |p, text| {
                            p.connection
                                .send(&ClientboundSetSubtitleTextPacket { text })
                        },
                    ))
                    .then(title_node(
                        "actionbar",
                        Messages("commands.title.show.actionbar"),
                        |p, text| {
                            p.connection
                                .send(&ClientboundSetActionBarTextPacket { text })
                        },
                    ))
                    .then(
                        literal("times").then(argument("fadeIn", TimeArgument::time()).then(
                            argument("stay", TimeArgument::time()).then(
                                argument("fadeOut", TimeArgument::time()).executes(move |c| {
                                    let packet = ClientboundSetTitlesAnimationPacket {
                                        fade_in: IntegerArgumentType::get_integer(c, "fadeIn"),
                                        stay: IntegerArgumentType::get_integer(c, "stay"),
                                        fade_out: IntegerArgumentType::get_integer(c, "fadeOut"),
                                    };
                                    send_packet_to_players(
                                        c.get_source(),
                                        targets(c)?,
                                        &packet,
                                        Messages("commands.title.times"),
                                    )
                                }),
                            ),
                        )),
                    ),
            ),
    );
});

/// `tracker.sendFeedback(source, true, messages)`: every target is tracked with value 1 (no `onZero`).
fn send_feedback(source: &CommandSourceStack, targets: &[EntityRef], messages: Messages) -> i32 {
    let total = targets.len() as i32;
    source.send_success(
        || match targets {
            [only] => {
                let name = source.with_server(|server| {
                    host::get_entity(server, only)
                        .map_or_else(Component::empty, |e| host::entity_display_name(server, e))
                });
                translatable!(format!("{}.single", messages.0), name)
            }
            _ => translatable!(format!("{}.multiple", messages.0), total),
        },
        true,
    );
    total
}

fn send_packet_to_players<P: Packet>(
    source: &CommandSourceStack,
    targets: Vec<EntityRef>,
    packet: &P,
    messages: Messages,
) -> Result<i32, CommandSyntaxException> {
    for player in &targets {
        if let Some(handle) = source.with_server(|server| host::player_handle(server, player)) {
            handle.connection.send(packet);
        }
    }
    Ok(send_feedback(source, &targets, messages))
}

fn show_title(
    source: &CommandSourceStack,
    targets: Vec<EntityRef>,
    title: Component,
    messages: Messages,
    send: SendText,
) -> Result<i32, CommandSyntaxException> {
    for player in &targets {
        let resolved = resolve(
            &ResolutionContext::with_entity_override(source, player.clone()),
            &title,
        )?;
        if let Some(handle) = source.with_server(|server| host::player_handle(server, player)) {
            send(&handle, resolved);
        }
    }
    Ok(send_feedback(source, &targets, messages))
}
