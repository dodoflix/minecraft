//! Chat family of `ServerGamePacketListenerImpl`: `handleChat`, `handleChatCommand`, `handleSignedChatCommand`,
//! `handleChatAck`, `handleChatSessionUpdate`, `handleCustomCommandSuggestions` and the spam throttling.
//! The chat model (signatures, last-seen tracking, `PlayerChatMessage`) is `crate::server::chat`.
//!
//! Text filtering: the dedicated server's `TextFilter` is `TextFilter.DUMMY` unless a filtering service is
//! configured, which passes every message through unchanged; `filterTextPacket` is therefore synchronous here.

use super::PlayCtx;
use crate::server::chat::{ChatType, ChatTypeBound, DecodeException, LastSeenMessages, PlayerChatMessage, SignedMessageBody};
use mc_protocol::packets::play::{
    ClientboundCommandSuggestionsPacket, ClientboundSystemChatPacket, ServerboundChatAckPacket, ServerboundChatCommandPacket,
    ServerboundChatCommandSignedPacket, ServerboundChatPacket, ServerboundChatSessionUpdatePacket, ServerboundCommandSuggestionPacket,
};
use mc_text::{ChatFormatting, Component};

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

impl PlayCtx<'_> {
    /// `unpackAndApplyLastSeen`: `None` after disconnecting the player on a validation failure.
    fn unpack_and_apply_last_seen(&mut self, update: &crate::server::chat::LastSeenMessagesUpdate) -> Option<LastSeenMessages> {
        let result = self.lis().last_seen_messages.lock().apply_update(update);
        match result {
            Ok(last_seen) => Some(last_seen),
            Err(e) => {
                log::error!("Failed to validate message acknowledgements from {}: {e}", self.player.get_plain_text_name());
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
            let decorated = ctx.server.chat_decorator().decorate(&*ctx.player, signed_message.decorated_content());
            let filtered_message = signed_message.with_unsigned_content(decorated).filter_pass_through();
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

    /// `performUnsignedChatCommand`.
    fn perform_unsigned_chat_command(&mut self, command: &str) {
        let parsed = self.server.commands().parse(command, self.player.create_command_source_stack(self.level));
        if self.server.enforce_secure_profile() && parsed.has_signable_arguments() {
            log::error!("Received unsigned command packet from {}, but the command requires signable arguments: {}", self.player.get_profile().name, command);
            self.player.send_system_message(invalid_command_signature());
        } else {
            self.server.commands().perform_command(parsed, command);
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
        let mut command = self.server.commands().parse(&packet.command, self.player.create_command_source_stack(self.level));
        let signed_arguments = match self.collect_signed_arguments(packet, &command.signable_arguments(), last_seen) {
            Ok(a) => a,
            Err(e) => {
                self.handle_message_decode_failure(&e);
                return;
            }
        };
        command.map_source_with_signing_context(signed_arguments);
        self.server.commands().perform_command(command, &packet.command);
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
        parsed_arguments: &[crate::commands::SignableArgument],
        last_seen: LastSeenMessages,
    ) -> Result<Vec<(String, PlayerChatMessage)>, DecodeException> {
        let argument_signatures = &packet.argument_signatures.entries;
        let mismatch = |this: &mut Self| {
            let client_names = argument_signatures.iter().map(|e| e.name.as_str()).collect::<Vec<_>>().join(", ");
            let expected_names = parsed_arguments.iter().map(|a| a.name()).collect::<Vec<_>>().join(", ");
            let _ = this;
            log::error!(
                "Signed command mismatch between server and client ('{}'): got [{}] from client, but expected [{}]",
                packet.command,
                client_names,
                expected_names
            );
            DecodeException::new(invalid_command_signature())
        };
        if argument_signatures.is_empty() {
            let mut arguments = Vec::new();
            for parsed in parsed_arguments {
                let body = SignedMessageBody::unsigned(parsed.value());
                arguments.push((parsed.name().to_owned(), self.lis().signed_message_decoder.unpack(None, body)?));
            }
            return Ok(arguments);
        }
        let mut signed_arguments: Vec<(String, PlayerChatMessage)> = Vec::new();
        for client_argument in argument_signatures {
            let Some(expected) = parsed_arguments.iter().find(|a| a.name() == client_argument.name) else {
                self.lis().signed_message_decoder.set_chain_broken();
                return Err(mismatch(self));
            };
            let body = SignedMessageBody::new(expected.value().to_owned(), packet.time_stamp, packet.salt, last_seen.clone());
            let message = self.lis().signed_message_decoder.unpack(Some(&client_argument.signature), body)?;
            signed_arguments.retain(|(n, _)| n != expected.name());
            signed_arguments.push((expected.name().to_owned(), message));
        }
        for expected in parsed_arguments {
            if !signed_arguments.iter().any(|(n, _)| n == expected.name()) {
                return Err(mismatch(self));
            }
        }
        Ok(signed_arguments)
    }

    /// `getSignedMessage`.
    fn get_signed_message(&mut self, packet: &ServerboundChatPacket, last_seen: LastSeenMessages) -> Result<PlayerChatMessage, DecodeException> {
        let body = SignedMessageBody::new(packet.message.clone(), packet.timestamp, packet.salt, last_seen);
        self.lis().signed_message_decoder.unpack(packet.signature.as_ref(), body)
    }

    /// `broadcastChatMessage`.
    fn broadcast_chat_message(&mut self, message: PlayerChatMessage) {
        let bound = ChatTypeBound::bind(ChatType::Chat, &*self.player);
        crate::server::players::player_list::broadcast_chat_message(self.server, message, &*self.player, bound);
        self.detect_chat_rate_spam();
    }

    /// `detectRateSpam`.
    fn detect_rate_spam(&mut self, command: bool) {
        let lis = self.lis();
        let throttler = if command { &mut lis.command_spam_throttler } else { &mut lis.chat_spam_throttler };
        throttler.increment();
        let under = throttler.is_under_threshold();
        if !under && !self.server.player_list.is_op(&self.player.name_and_id()) && !self.server.is_singleplayer_owner(&self.player.name_and_id()) {
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
        let result = self.lis().last_seen_messages.lock().apply_offset(packet.offset);
        if let Err(e) = result {
            log::error!("Failed to validate message acknowledgement offset from {}: {e}", self.player.get_plain_text_name());
            self.disconnect(chat_validation_failed());
        }
    }

    /// `handleChatSessionUpdate`.
    pub fn handle_chat_session_update(&mut self, packet: &ServerboundChatSessionUpdatePacket) {
        let new_session = &packet.chat_session;
        let old_key = self.lis().chat_session.as_ref().map(|s| s.profile_public_key_data().clone());
        let new_key = new_session.profile_public_key.clone();
        if old_key.as_ref() == Some(&new_key) {
            return;
        }
        if let Some(old) = &old_key
            && new_key.expires_at() < old.expires_at()
        {
            self.disconnect(crate::server::chat::expired_profile_public_key());
            return;
        }
        let Some(validator) = self.server.services().profile_key_signature_validator() else {
            log::warn!("Ignoring chat session from {} due to missing Services public key", self.player.get_profile().name);
            return;
        };
        match new_session.validate(self.player.get_profile(), validator) {
            Ok(session) => self.reset_player_chat_state(session),
            Err(e) => {
                log::error!("Failed to validate profile key: {e}");
                self.disconnect(e.component());
            }
        }
    }

    /// `resetPlayerChatState`.
    fn reset_player_chat_state(&mut self, chat_session: crate::server::chat::RemoteChatSession) {
        let uuid = self.player.uuid();
        let lis = self.lis();
        lis.signed_message_decoder = chat_session.create_message_decoder(uuid);
        lis.chat_session = Some(chat_session.clone());
        // `chatMessageChain.append(() -> { player.setChatSession(..); broadcastAll(INITIALIZE_CHAT) })`
        self.player.set_chat_session(chat_session);
        let packet = crate::server::players::player_info_update(
            mc_protocol::packets::play::PlayerInfoAction::InitializeChat,
            &*self.player,
        );
        crate::server::players::player_list::broadcast_all(self.server, &packet);
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
        let command = command.strip_prefix('/').unwrap_or(&command);
        let suggestions = self.server.commands().get_completion_suggestions(command, self.player.create_command_source_stack(self.level));
        let suggestions = suggestions.limited_to(1000);
        self.send(&ClientboundCommandSuggestionsPacket::new(client_id, suggestions));
    }
}
