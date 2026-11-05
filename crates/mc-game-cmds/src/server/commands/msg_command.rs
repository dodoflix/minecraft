//! `MsgCommand` (`msg`, `tell`, `w`).
use mc_game_core::commands::arguments::selector::EntityRef;
use mc_game_core::commands::arguments::{EntityArgument, MessageArgument};
use mc_game_core::commands::{CommandSourceStack, argument, host, literal};
use mc_game_core::register_command;
use mc_game_core::server::chat::chat_type;
use mc_game_core::server::chat::{OutgoingChatMessage, PlayerChatMessage};
use mc_game_core::server::players::PlayerHandle;
use mc_brigadier::ArgumentBuilder;

register_command!("MsgCommand", |ctx| {
    let msg = ctx.dispatcher.register(literal("msg").then(argument("targets", EntityArgument::players()).then(argument("message", MessageArgument::message()).executes(|c| {
        let players = EntityArgument::get_players(c, "targets")?;
        if !players.is_empty() {
            MessageArgument::resolve_chat_message(c, "message", |message| send_message(c.get_source(), &players, &message))?;
        }
        Ok(players.len() as i32)
    }))));
    ctx.dispatcher.register(literal("tell").redirect(msg.clone()));
    ctx.dispatcher.register(literal("w").redirect(msg));
});

/// `CommandSourceStack.sendChatMessage(message, filtered, chatType)`.
fn source_send_chat_message(source: &CommandSourceStack, message: &OutgoingChatMessage, filtered: bool, chat_type: &mc_game_core::server::chat::chat_type::Bound) {
    if source.is_silent() {
        return;
    }
    match source.get_player().and_then(|p| p.player.as_ref()) {
        Some(player) => player.send_chat_message(message, filtered, chat_type),
        None => source.source().send_system_message(&chat_type.decorate(&message.content())),
    }
}

fn send_message(source: &CommandSourceStack, players: &[EntityRef], message: &PlayerChatMessage) {
    let source_name = source.get_display_name();
    let tracked = OutgoingChatMessage::create(message);
    let mut was_fully_filtered = false;
    let sender: Option<PlayerHandle> = source.get_player().and_then(|p| p.player.clone());
    let (incoming, deliveries) = source.with_server(|server| {
        let incoming = host::bind_chat_type(server, chat_type::MSG_COMMAND_INCOMING, source_name.clone());
        let mut deliveries = Vec::new();
        for player in players {
            let Some(target) = host::player_handle(server, player) else { continue };
            let target_name = host::player_display_name(server, &target);
            let outgoing = host::bind_chat_type(server, chat_type::MSG_COMMAND_OUTGOING, source_name.clone()).with_target_name(target_name);
            deliveries.push((target, outgoing));
        }
        (incoming, deliveries)
    });
    for (target, outgoing) in &deliveries {
        source_send_chat_message(source, &tracked, false, outgoing);
        let filtered = sender.as_ref().is_some_and(|s| s.should_filter_message_to(target));
        target.send_chat_message(&tracked, filtered, &incoming);
        was_fully_filtered |= filtered && message.is_fully_filtered();
    }
    if was_fully_filtered {
        source.send_system_message(&mc_game_core::server::players::player_list::chat_filtered_full());
    }
}
