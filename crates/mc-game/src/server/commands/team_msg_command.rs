//! `TeamMsgCommand` (`teammsg`, `tm`).
use crate::commands::arguments::selector::EntityRef;
use crate::commands::arguments::MessageArgument;
use crate::commands::message::simple_translatable;
use crate::commands::{CommandSourceStack, argument, host, literal};
use crate::server::chat::chat_type;
use crate::server::chat::{OutgoingChatMessage, PlayerChatMessage};
use crate::server::players::PlayerHandle;
use crate::{register_command, translatable};
use mc_brigadier::{ArgumentBuilder, CommandSyntaxException, SimpleCommandExceptionType};
use mc_text::events::{ClickEvent, HoverEvent};
use mc_text::Style;
use std::sync::LazyLock;

static ERROR_NOT_ON_TEAM: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("commands.teammsg.failed.noteam"));

register_command!("TeamMsgCommand", |ctx| {
    let msg = ctx.dispatcher.register(literal("teammsg").then(argument("message", MessageArgument::message()).executes(|c| {
        let source = c.get_source();
        let entity = EntityRef { dimension: source.get_level().clone(), id: source.get_entity_or_exception()?.id };
        let Some((team, receivers)) = source.with_server(|server| {
            let scoreboard_name = host::get_entity(server, &entity)?.get_scoreboard_name();
            let team = server.scoreboard.get_players_team(&scoreboard_name)?.get_name().to_owned();
            let receivers: Vec<PlayerHandle> = server
                .player_list
                .get_players()
                .into_iter()
                .filter(|receiver| receiver.id == entity.id || server.scoreboard.get_players_team(receiver.name()).is_some_and(|t| t.get_name() == team))
                .collect();
            Some((team, receivers))
        }) else {
            return Err::<i32, CommandSyntaxException>(ERROR_NOT_ON_TEAM.create());
        };
        if !receivers.is_empty() {
            MessageArgument::resolve_chat_message(c, "message", |message| send_message(source, &entity, &team, &receivers, &message))?;
        }
        Ok(receivers.len() as i32)
    })));
    ctx.dispatcher.register(literal("tm").redirect(msg));
});

fn send_message(source: &CommandSourceStack, entity: &EntityRef, team: &str, receivers: &[PlayerHandle], message: &PlayerChatMessage) {
    let suggest_style = Style::EMPTY
        .with_hover_event(Some(HoverEvent::show_text(translatable!("chat.type.team.hover"))))
        .with_click_event(Some(ClickEvent::SuggestCommand("/teammsg ".to_owned())));
    let team_name = source.with_server(|s| s.scoreboard.get_player_team(team).expect("team exists").get_formatted_display_name()).with_style(&suggest_style);
    let source_name = source.get_display_name();
    let (incoming, outgoing) = source.with_server(|server| {
        (
            host::bind_chat_type(server, chat_type::TEAM_MSG_COMMAND_INCOMING, source_name.clone()).with_target_name(team_name.clone()),
            host::bind_chat_type(server, chat_type::TEAM_MSG_COMMAND_OUTGOING, source_name.clone()).with_target_name(team_name.clone()),
        )
    });
    let tracked = OutgoingChatMessage::create(message);
    let sender: Option<PlayerHandle> = source.get_player().and_then(|p| p.player.clone());
    let mut was_fully_filtered = false;
    for team_player in receivers {
        let chat_type = if team_player.id == entity.id { &outgoing } else { &incoming };
        let filtered = sender.as_ref().is_some_and(|s| s.should_filter_message_to(team_player));
        team_player.send_chat_message(&tracked, filtered, chat_type);
        was_fully_filtered |= filtered && message.is_fully_filtered();
    }
    if was_fully_filtered {
        source.send_system_message(&crate::server::players::player_list::chat_filtered_full());
    }
}
