//! Chat family of `ServerGamePacketListenerImpl`: `handleChat`, `handleChatCommand`, `handleSignedChatCommand`,
//! `handleChatAck`, `handleChatSessionUpdate`, `handleCustomCommandSuggestions` and the spam throttling.
//! The chat model (signatures, last-seen tracking, `PlayerChatMessage`) is `crate::server::chat`.
//!
//! Text filtering: the dedicated server's `TextFilter` is `TextFilter.DUMMY` unless a filtering service is
//! configured, which passes every message through unchanged; `filterTextPacket` is therefore synchronous here.

use super::PlayCtx;
use crate::commands::Commands;
use crate::commands::arguments::message_argument::MessageArgument;
use crate::commands::command_signing_context::CommandSigningContext;
use crate::server::chat::signed_message::DecodeException;
use crate::server::chat::{LastSeenMessages, PlayerChatMessage, SignedMessageBody};
use mc_brigadier::ImmutableStringReader;
use mc_brigadier::context::CommandContextBuilder;
use mc_brigadier::dispatcher::ParseResults;
use mc_protocol::packets::play::{
    ClientboundCommandSuggestionsEntry, ClientboundCommandSuggestionsPacket, ClientboundSystemChatPacket, LastSeenMessagesUpdate, ServerboundChatAckPacket,
    ServerboundChatCommandPacket, ServerboundChatCommandSignedPacket, ServerboundChatPacket, ServerboundChatSessionUpdatePacket,
    ServerboundCommandSuggestionPacket,
};
use mc_text::{ChatFormatting, Component};
use std::collections::HashMap;
use std::sync::Arc;

/// `StringUtil.isAllowedChatCharacter`.
fn is_allowed_chat_character(c: u16) -> bool {
    c != 167 && c >= u16::from(b' ') && c != 127
}

/// `isChatMessageIllegal`: iterates UTF-16 code units like `String.charAt`.
fn is_chat_message_illegal(message: &str) -> bool {
    message.encode_utf16().any(|c| !is_allowed_chat_character(c))
}

fn chat_validation_failed() -> Component {
    Component::translatable("multiplayer.disconnect.chat_validation_failed", vec![])
}

/// `INVALID_COMMAND_SIGNATURE`.
fn invalid_command_signature() -> Component {
    Component::translatable("chat.disabled.invalid_command_signature", vec![]).with_format(ChatFormatting::Red)
}

/// `SignableCommand.Argument`: a signed-message argument of a parsed command.
struct SignableArgument {
    name: String,
    value: String,
}

/// `SignableCommand.of(parse)`: the `MessageArgument`s of the parsed command chain.
fn signable_arguments(parse: &ParseResults<crate::commands::command_source_stack::CommandSourceStack>) -> Vec<SignableArgument> {
    fn collect(string: &str, context: &CommandContextBuilder<crate::commands::command_source_stack::CommandSourceStack>, out: &mut Vec<SignableArgument>) {
        for parsed in context.get_nodes() {
            let node = parsed.get_node();
            if node.get_type().is_some_and(|t| t.downcast_ref::<MessageArgument>().is_some()) {
                out.push(SignableArgument { name: node.get_name().to_owned(), value: parsed.get_range().get_str(string) });
            }
        }
    }
    let string = parse.get_reader().get_string();
    let context = parse.get_context();
    let mut out = Vec::new();
    collect(string, context, &mut out);
    let mut last = context;
    while let Some(child) = last.get_child() {
        if Arc::ptr_eq(child.get_root_node(), context.get_root_node()) {
            break;
        }
        collect(string, child, &mut out);
        last = child;
    }
    out
}

impl PlayCtx<'_> {
    /// `unpackAndApplyLastSeen`: `None` after disconnecting the player on a validation failure.
    fn unpack_and_apply_last_seen(&mut self, update: &LastSeenMessagesUpdate) -> Option<LastSeenMessages> {
        let result = self.lis().chat_out.lock().last_seen_messages.apply_update(update);
        match result {
            Ok(last_seen) => Some(last_seen),
            Err(e) => {
                log::error!("Failed to validate message acknowledgements from {}: {}", self.player.get_plain_text_name(), e.0);
                self.disconnect(chat_validation_failed());
                None
            }
        }
    }

    /// `tryHandleChat(message, isCommand, chatHandler)`: the handler is `server.execute`d, i.e. runs next on the game
    /// thread; here the whole queue is already on the game thread so it runs immediately.
    fn try_handle_chat(&mut self, message: &str, is_command: bool, chat_handler: impl FnOnce(&mut Self)) {
        if is_chat_message_illegal(message) {
            self.disconnect(Component::translatable("multiplayer.disconnect.illegal_characters", vec![]));
        } else if !is_command && self.player.get_chat_visibility() == mc_protocol::packets::common::ChatVisibility::Hidden {
            self.send(&ClientboundSystemChatPacket {
                content: Component::translatable("chat.disabled.options", vec![]).with_format(ChatFormatting::Red),
                overlay: false,
            });
        } else {
            self.player.reset_last_action_time();
            chat_handler(self);
        }
    }

    /// `handleChat`.
    pub fn handle_chat(&mut self, packet: &ServerboundChatPacket) {
        let Some(unpacked_last_seen) = self.unpack_and_apply_last_seen(&packet.last_seen_messages) else { return };
        self.try_handle_chat(&packet.message, false, |ctx| {
            let signed_message = match ctx.get_signed_message(packet, unpacked_last_seen) {
                Ok(m) => m,
                Err(e) => {
                    ctx.handle_message_decode_failure(&e);
                    return;
                }
            };
            // `filterTextPacket` (DUMMY filter: pass-through) throws CancellationException once disconnected
            if !ctx.lis().is_accepting_messages() {
                log::debug!("Ignoring packet due to disconnection");
                return;
            }
            // `ChatDecorator.PLAIN`: the decorated content is the signed content itself
            let decorated = signed_message.decorated_content();
            let filtered_message = signed_message.with_unsigned_content(decorated);
            ctx.broadcast_chat_message(filtered_message);
        });
    }

    /// `handleChatCommand`.
    pub fn handle_chat_command(&mut self, packet: &ServerboundChatCommandPacket) {
        self.try_handle_chat(&packet.command, true, |ctx| {
            ctx.perform_unsigned_chat_command(&packet.command);
            ctx.detect_command_rate_spam();
        });
    }

    /// `parseCommand(command)`.
    fn parse_command(&mut self, command: &str) -> Option<(Arc<Commands>, ParseResults<crate::commands::command_source_stack::CommandSourceStack>)> {
        let commands = self.server.commands.clone();
        
        let source = self.create_command_source_stack()?;
        let parsed = crate::commands::server_access::enter_server(self.server, || commands.get_dispatcher().parse(command, source));
        Some((commands, parsed))
    }

    /// `performUnsignedChatCommand`.
    fn perform_unsigned_chat_command(&mut self, command: &str) {
        let Some((commands, parsed)) = self.parse_command(command) else { return };
        if self.server.enforce_secure_profile() && !signable_arguments(&parsed).is_empty() {
            log::error!("Received unsigned command packet from {}, but the command requires signable arguments: {}", self.player.get_profile().name, command);
            self.player.send_system_message(invalid_command_signature());
        } else {
            let command = command.to_owned();
            self.lis().deferred_tasks.push(Box::new(move |server| commands.perform_command(server, parsed, &command)));
        }
    }

    /// `handleSignedChatCommand`.
    pub fn handle_signed_chat_command(&mut self, packet: &ServerboundChatCommandSignedPacket) {
        let Some(unpacked_last_seen) = self.unpack_and_apply_last_seen(&packet.last_seen_messages) else { return };
        self.try_handle_chat(&packet.command, true, |ctx| {
            ctx.perform_signed_chat_command(packet, unpacked_last_seen);
            ctx.detect_command_rate_spam();
        });
    }

    /// `performSignedChatCommand`.
    fn perform_signed_chat_command(&mut self, packet: &ServerboundChatCommandSignedPacket, last_seen: LastSeenMessages) {
        let Some((commands, parsed)) = self.parse_command(&packet.command) else { return };
        let arguments = signable_arguments(&parsed);
        let signed = match self.collect_signed_arguments(packet, &arguments, last_seen) {
            Ok(a) => a,
            Err(e) => {
                self.handle_message_decode_failure(&e);
                return;
            }
        };
        let context = CommandSigningContext::Signed(Arc::new(signed));
        let parsed = Commands::map_source(parsed, |source| source.with_signing_context(context.clone()));
        let command = packet.command.clone();
        self.lis().deferred_tasks.push(Box::new(move |server| commands.perform_command(server, parsed, &command)));
    }

    /// `handleMessageDecodeFailure`.
    fn handle_message_decode_failure(&mut self, e: &DecodeException) {
        log::warn!("Failed to update secure chat state for {}: '{}'", self.player.get_profile().name, e.component().get_string());
        self.player.send_system_message(e.component().copy().with_format(ChatFormatting::Red));
    }

    /// `collectSignedArguments` / `collectUnsignedArguments` / `createSignedArgumentMismatchException`.
    fn collect_signed_arguments(
        &mut self,
        packet: &ServerboundChatCommandSignedPacket,
        parsed_arguments: &[SignableArgument],
        last_seen: LastSeenMessages,
    ) -> Result<HashMap<String, PlayerChatMessage>, DecodeException> {
        let argument_signatures = &packet.argument_signatures.entries;
        let mismatch = || {
            let client_names = argument_signatures.iter().map(|e| e.name.as_str()).collect::<Vec<_>>().join(", ");
            let expected_names = parsed_arguments.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(", ");
            log::error!(
                "Signed command mismatch between server and client ('{}'): got [{}] from client, but expected [{}]",
                packet.command,
                client_names,
                expected_names
            );
            DecodeException::new(invalid_command_signature())
        };
        let mut arguments = HashMap::new();
        if argument_signatures.is_empty() {
            for parsed in parsed_arguments {
                let body = SignedMessageBody::unsigned(parsed.value.clone());
                arguments.insert(parsed.name.clone(), self.lis().signed_message_decoder.unpack(None, body)?);
            }
            return Ok(arguments);
        }
        for client_argument in argument_signatures {
            let Some(expected) = parsed_arguments.iter().find(|a| a.name == client_argument.name) else {
                self.lis().signed_message_decoder.set_chain_broken();
                return Err(mismatch());
            };
            let body = SignedMessageBody { content: expected.value.clone(), time_stamp: packet.time_stamp, salt: packet.salt, last_seen: last_seen.clone() };
            let message = self.lis().signed_message_decoder.unpack(Some(&client_argument.signature), body)?;
            arguments.insert(expected.name.clone(), message);
        }
        for expected in parsed_arguments {
            if !arguments.contains_key(&expected.name) {
                return Err(mismatch());
            }
        }
        Ok(arguments)
    }

    /// `getSignedMessage`.
    fn get_signed_message(&mut self, packet: &ServerboundChatPacket, last_seen: LastSeenMessages) -> Result<PlayerChatMessage, DecodeException> {
        let body = SignedMessageBody { content: packet.message.clone(), time_stamp: packet.time_stamp, salt: packet.salt, last_seen };
        self.lis().signed_message_decoder.unpack(packet.signature.as_ref(), body)
    }

    /// `broadcastChatMessage`: `playerList.broadcastChatMessage(message, player, ChatType.bind(ChatType.CHAT, player))`.
    fn broadcast_chat_message(&mut self, message: PlayerChatMessage) {
        let Some(handle) = self.player.handle.clone() else { return };
        let bound = crate::commands::host::bind_chat_type(self.server, "minecraft:chat", self.player.get_display_name());
        self.server.player_list.broadcast_chat_message_from_player(&message, &handle, &bound);
        self.detect_chat_rate_spam();
    }

    /// `detectRateSpam`.
    fn detect_rate_spam(&mut self, command: bool) {
        let lis = self.lis();
        let throttler = if command { &mut lis.command_spam_throttler } else { &mut lis.chat_spam_throttler };
        throttler.increment();
        let under = throttler.is_under_threshold();
        // `isSingleplayerOwner`: never true on a dedicated server
        if !under && !self.player_is_op() {
            self.disconnect(Component::translatable("disconnect.spam", vec![]));
        }
    }

    fn detect_command_rate_spam(&mut self) {
        self.detect_rate_spam(true);
    }

    fn detect_chat_rate_spam(&mut self) {
        self.detect_rate_spam(false);
    }

    /// `handleChatAck`.
    pub fn handle_chat_ack(&mut self, packet: &ServerboundChatAckPacket) {
        let result = self.lis().chat_out.lock().last_seen_messages.apply_offset(packet.offset);
        if let Err(e) = result {
            log::error!("Failed to validate message acknowledgement offset from {}: {}", self.player.get_plain_text_name(), e.0);
            self.disconnect(chat_validation_failed());
        }
    }

    /// `handleChatSessionUpdate`.
    pub fn handle_chat_session_update(&mut self, packet: &ServerboundChatSessionUpdatePacket) {
        let new_session = &packet.chat_session;
        let new_key = &new_session.profile_public_key;
        let old_key = self.lis().chat_session.as_ref().map(|s| s.as_data().profile_public_key);
        if old_key.as_ref() == Some(new_key) {
            return;
        }
        if let Some(old) = &old_key
            && new_key.expires_at < old.expires_at
        {
            self.disconnect(crate::server::chat::remote_chat_session::expired_profile_public_key());
            return;
        }
        let Some(validator) = super::services_keys::profile_key_signature_validator() else {
            log::warn!("Ignoring chat session from {} due to missing Services public key", self.player.get_profile().name);
            return;
        };
        let profile_id = self.player.get_profile().id;
        match crate::server::chat::RemoteChatSession::validate(new_session, profile_id, &*validator) {
            Ok(session) => self.reset_player_chat_state(session),
            Err(e) => {
                log::error!("Failed to validate profile key: {}", e.component().get_string());
                self.disconnect(e.component().clone());
            }
        }
    }

    /// `resetPlayerChatState`: new decoder, then (through the chat message chain, here immediately) the player's chat
    /// session is set and `INITIALIZE_CHAT` is broadcast.
    fn reset_player_chat_state(&mut self, chat_session: crate::server::chat::RemoteChatSession) {
        let uuid = crate::world::entity::Entity::uuid(&*self.player);
        let lis = self.lis();
        lis.signed_message_decoder = Box::new(chat_session.create_message_decoder(uuid));
        lis.chat_session = Some(chat_session.clone());
        let Some(handle) = self.player.handle.clone() else { return };
        handle.set_chat_session(Some(&chat_session));
        use mc_protocol::packets::play::ClientboundPlayerInfoUpdateAction;
        let update = crate::server::players::player_handle::create_info_update([ClientboundPlayerInfoUpdateAction::InitializeChat].into_iter().collect(), [&handle]);
        self.server.player_list.broadcast_all(&update);
    }

    /// `handleCustomCommandSuggestions` (`ServerCommandSuggestionsProvider.request`).
    pub fn handle_custom_command_suggestions(&mut self, packet: &ServerboundCommandSuggestionPacket) {
        let lis = self.lis();
        let old_request = lis.suggestion_request.replace((packet.id, packet.command.clone()));
        if old_request.is_none() && lis.suggestion_budget {
            self.try_process_suggestion_request();
        }
    }

    /// `ServerCommandSuggestionsProvider.tick()`.
    pub(super) fn command_suggestions_tick(&mut self) {
        self.lis().suggestion_budget = true;
        self.try_process_suggestion_request();
    }

    /// `ServerCommandSuggestionsProvider.tryProcessRequest()`.
    fn try_process_suggestion_request(&mut self) {
        let lis = self.lis();
        if !lis.suggestion_budget {
            return;
        }
        let Some((client_id, command)) = lis.suggestion_request.take() else { return };
        lis.suggestion_budget = false;
        let command = command.strip_prefix('/').unwrap_or(&command).to_owned();
        let commands = self.server.commands.clone();
        let Some(source) = self.create_command_source_stack() else { return };
        let suggestions = crate::commands::server_access::enter_server(self.server, || {
            let dispatcher = commands.get_dispatcher();
            let parse = dispatcher.parse(&command, source);
            dispatcher.get_completion_suggestions(&parse)
        });
        let range = suggestions.get_range();
        let limit = suggestions.get_list().len().min(1000);
        let entries = suggestions.get_list()[..limit]
            .iter()
            .map(|s| ClientboundCommandSuggestionsEntry { text: s.get_text().to_owned(), tooltip: s.get_tooltip().map(|m| crate::commands::message::from_message(&**m)) })
            .collect();
        // `limitSuggestionCount`: more than 1000 keep the first 1000 with the same range
        self.send(&ClientboundCommandSuggestionsPacket { id: client_id, start: range.get_start() as i32, length: range.get_length() as i32, suggestions: entries });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn illegal_chat_characters() {
        assert!(!is_chat_message_illegal("hello \u{e9}\u{1F600}"));
        assert!(is_chat_message_illegal("a\u{a7}b"));
        assert!(is_chat_message_illegal("a\u{7f}"));
        assert!(is_chat_message_illegal("tab\there"));
    }
}
