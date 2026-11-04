//! `MsgCommand` (`msg`, `tell`, `w`).
use crate::commands::arguments::{EntityArgument, MessageArgument};
use crate::commands::arguments::selector::EntityRef;
use crate::commands::{CommandSourceStack, argument, host, literal};
use crate::register_command;
use crate::server::chat::chat_type;
use crate::server::chat::{OutgoingChatMessage, PlayerChatMessage};

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

fn send_message(source: &CommandSourceStack, players: &[EntityRef], message: &PlayerChatMessage) {
    let source_name = source.get_display_name();
    let tracked = OutgoingChatMessage::create(message);
    let mut was_fully_filtered = false;
    let sender_player = source.get_player().and_then(|p| p.player.clone());
    let delivery = source.with_server(|server| {
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
    let (incoming, deliveries) = delivery;
    for (target, outgoing) in &deliveries {
        source.send_chat_message(&tracked, false, outgoing);
        let filtered = match &sender_player {
            Some(sender) => !std::ptr::eq(sender, target) && (sender.text_filtering_enabled() || target.text_filtering_enabled()) && sender.uuid != target.uuid,
            None => target.text_filtering_enabled(),
        };
        target.send_chat_message(&tracked, filtered, &incoming);
        was_fully_filtered |= filtered && message.is_fully_filtered();
    }
    if was_fully_filtered {
        source.send_system_message(&crate::server::players::player_list::chat_filtered_full());
    }
}
