//! `MessageArgument`: the rest of the line, with embedded `@selectors` resolved to entity names.

use super::selector::entity_selector_parser::{
    ERROR_MISSING_SELECTOR_TYPE, ERROR_UNKNOWN_SELECTOR_TYPE,
};
use super::selector::{EntitySelector, EntitySelectorParser};
use crate::commands::CommandSourceStack;
use crate::commands::message::message;
use crate::server::chat::{FilteredText, PlayerChatMessage, chat_decorator};
use crate::server::permissions::permissions;
use crate::translatable;
use mc_brigadier::arguments::value;
use mc_brigadier::utf16::{len16, substring, substring_from};
use mc_brigadier::{
    ArgumentType, CommandContext, CommandSyntaxException, Dynamic2CommandExceptionType,
    ImmutableStringReader, ParsedValue, StringReader,
};
use mc_protocol::packets::play::ArgumentTypeTemplate;
use mc_text::Component;
use std::sync::LazyLock;

static TOO_LONG: LazyLock<Dynamic2CommandExceptionType<i32, i32>> = LazyLock::new(|| {
    Dynamic2CommandExceptionType::new(|length: i32, max_length: i32| {
        message(translatable!(
            "argument.message.too_long",
            length,
            max_length
        ))
    })
});

/// `MessageArgument.Part`.
#[derive(Clone, Debug)]
pub struct Part {
    pub start: usize,
    pub end: usize,
    pub selector: EntitySelector,
}

impl Part {
    pub fn to_component(
        &self,
        sender: &CommandSourceStack,
    ) -> Result<Component, CommandSyntaxException> {
        sender.with_server(|server| {
            let entities = self.selector.find_entities(sender, server)?;
            Ok(EntitySelector::join_names(server, &entities))
        })
    }
}

/// `MessageArgument.Message`.
#[derive(Clone, Debug)]
pub struct Message {
    pub text: String,
    pub parts: Vec<Part>,
}

impl Message {
    fn resolve_component(
        &self,
        sender: &CommandSourceStack,
    ) -> Result<Component, CommandSyntaxException> {
        self.to_component(
            sender,
            sender
                .permissions()
                .has_permission(&permissions::COMMANDS_ENTITY_SELECTORS),
        )
    }

    pub fn to_component(
        &self,
        sender: &CommandSourceStack,
        allow_selectors: bool,
    ) -> Result<Component, CommandSyntaxException> {
        if self.parts.is_empty() || !allow_selectors {
            return Ok(Component::literal(self.text.clone()));
        }
        let mut result = Component::literal(substring(&self.text, 0, self.parts[0].start));
        let mut read_to = self.parts[0].start;
        for part in &self.parts {
            let component = part.to_component(sender)?;
            if read_to < part.start {
                result = result.append_str(&substring(&self.text, read_to, part.start));
            }
            result = result.append(component);
            read_to = part.end;
        }
        if read_to < len16(&self.text) {
            result = result.append_str(&substring_from(&self.text, read_to));
        }
        Ok(result)
    }

    pub fn parse_text(
        reader: &mut StringReader,
        allow_selectors: bool,
    ) -> Result<Message, CommandSyntaxException> {
        if reader.get_remaining_length() > 256 {
            return Err(TOO_LONG.create(reader.get_remaining_length() as i32, 256));
        }
        let text = reader.get_remaining().to_owned();
        if !allow_selectors {
            reader.set_cursor(reader.get_total_length());
            return Ok(Message {
                text,
                parts: Vec::new(),
            });
        }
        let mut result = Vec::new();
        let offset = reader.get_cursor();
        loop {
            let (start, parse) = loop {
                if !reader.can_read() {
                    return Ok(Message {
                        text,
                        parts: result,
                    });
                }
                if reader.peek() == '@' {
                    let start = reader.get_cursor();
                    let parsed = EntitySelectorParser::new(reader, true).parse();
                    match parsed {
                        Ok(parse) => break (start, parse),
                        Err(e) => {
                            if e.get_type() != ERROR_MISSING_SELECTOR_TYPE.create().get_type()
                                && e.get_type()
                                    != ERROR_UNKNOWN_SELECTOR_TYPE.create(String::new()).get_type()
                            {
                                return Err(e);
                            }
                            reader.set_cursor(start + 1);
                        }
                    }
                } else {
                    reader.skip();
                }
            };
            result.push(Part {
                start: start - offset,
                end: reader.get_cursor() - offset,
                selector: parse,
            });
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct MessageArgument;

impl MessageArgument {
    pub fn message() -> Self {
        Self
    }

    /// `getMessage`: the message with selectors resolved.
    pub fn get_message(
        context: &CommandContext<CommandSourceStack>,
        name: &str,
    ) -> Result<Component, CommandSyntaxException> {
        context
            .get_argument::<Message>(name)
            .resolve_component(context.get_source())
    }

    /// `resolveChatMessage(context, name, task)`: builds the (possibly signed) `PlayerChatMessage` and hands it to `task`.
    pub fn resolve_chat_message(
        context: &CommandContext<CommandSourceStack>,
        name: &str,
        task: impl FnOnce(PlayerChatMessage),
    ) -> Result<(), CommandSyntaxException> {
        let message = context.get_argument::<Message>(name);
        let sender = context.get_source();
        let formatted = message.resolve_component(sender)?;
        match sender.get_signing_context().get_argument(name) {
            Some(signed) => {
                let signed = signed.with_unsigned_content(formatted);
                // `filterPlainText`: the dedicated server has no text filter, so the message passes through.
                let filtered = FilteredText::pass_through(signed.signed_content());
                let decorated = chat_decorator::plain(None, signed.decorated_content());
                task(
                    signed
                        .with_unsigned_content(decorated)
                        .filter_with_mask(filtered.mask),
                );
            }
            None => {
                let argument = PlayerChatMessage::system(message.text.clone())
                    .with_unsigned_content(formatted);
                let decorated = chat_decorator::plain(None, argument.decorated_content());
                task(argument.with_unsigned_content(decorated));
            }
        }
        Ok(())
    }
}

impl ArgumentType<CommandSourceStack> for MessageArgument {
    fn parse(&self, reader: &mut StringReader) -> Result<ParsedValue, CommandSyntaxException> {
        Message::parse_text(reader, true).map(value)
    }

    fn parse_with_source(
        &self,
        reader: &mut StringReader,
        source: &CommandSourceStack,
    ) -> Result<ParsedValue, CommandSyntaxException> {
        Message::parse_text(reader, EntitySelectorParser::allow_selectors(source)).map(value)
    }

    fn get_examples(&self) -> Vec<String> {
        ["Hello world!", "foo", "@e", "Hello @p :)"]
            .map(String::from)
            .to_vec()
    }

    fn describe(&self) -> String {
        "MessageArgument".to_owned()
    }
}

crate::register_argument_type!(MessageArgument, |_a| ArgumentTypeTemplate::Message);
