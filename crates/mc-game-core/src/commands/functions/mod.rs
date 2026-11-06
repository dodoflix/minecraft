//! `net.minecraft.commands.functions`: mcfunction compilation (`CommandFunction.fromLines`) and macro functions.

mod function_builder;
mod macro_function;
mod string_template;

pub use function_builder::FunctionBuilder;
pub use macro_function::MacroFunction;
pub use string_template::StringTemplate;

use super::execution::UnboundEntryAction;
use super::execution::tasks::build_contexts::BuildContexts;
use super::{CommandSourceStack, Commands};
use mc_brigadier::{
    CommandDispatcher, CommandSyntaxException, ContextChain, ImmutableStringReader, StringReader,
};
use mc_core::ident::Identifier;
use mc_nbt::CompoundTag;
use mc_text::Component;
use std::sync::Arc;

/// `InstantiatedFunction` (`PlainTextFunction` is the same value).
pub struct InstantiatedFunction {
    pub id: Identifier,
    pub entries: Vec<Arc<dyn UnboundEntryAction>>,
}

/// `FunctionInstantiationException`.
#[derive(Clone, Debug)]
pub struct FunctionInstantiationException {
    pub message_component: Box<Component>,
}

impl FunctionInstantiationException {
    pub fn new(message_component: Component) -> Self {
        Self {
            message_component: Box::new(message_component),
        }
    }
}

impl std::fmt::Display for FunctionInstantiationException {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message_component.get_string())
    }
}

/// `CommandFunction<CommandSourceStack>`. Functions live on the game thread only.
pub trait CommandFunction: Send + Sync {
    fn id(&self) -> &Identifier;

    /// `function instanceof MacroFunction`.
    fn is_macro(&self) -> bool {
        false
    }

    fn instantiate(
        &self,
        arguments: Option<&CompoundTag>,
        dispatcher: &CommandDispatcher<CommandSourceStack>,
    ) -> Result<Arc<InstantiatedFunction>, FunctionInstantiationException>;
}

/// `PlainTextFunction`.
impl CommandFunction for Arc<InstantiatedFunction> {
    fn id(&self) -> &Identifier {
        &self.id
    }

    fn instantiate(
        &self,
        _arguments: Option<&CompoundTag>,
        _dispatcher: &CommandDispatcher<CommandSourceStack>,
    ) -> Result<Arc<InstantiatedFunction>, FunctionInstantiationException> {
        Ok(self.clone())
    }
}

const MAX_COMMAND_LENGTH: usize = 2_000_000;

fn should_concatenate_next_line(line: &str) -> bool {
    line.ends_with('\\')
}

/// `checkCommandLineLength` (`IllegalStateException`, here an `Err` message).
pub fn check_command_line_length(line: &str) -> Result<(), String> {
    let length = mc_brigadier::utf16::len16(line);
    if length > MAX_COMMAND_LENGTH {
        let truncated = mc_brigadier::utf16::substring(line, 0, length.min(512));
        return Err(format!(
            "Command too long: {length} characters, contents: {truncated}..."
        ));
    }
    Ok(())
}

/// Java `String.trim()`: removes everything <= U+0020 at both ends.
fn java_trim(s: &str) -> &str {
    s.trim_matches(|c: char| c <= ' ')
}

/// `CommandFunction.fromLines`: errors are vanilla's `IllegalArgumentException`/`IllegalStateException` messages.
pub fn from_lines(
    id: Identifier,
    dispatcher: &CommandDispatcher<CommandSourceStack>,
    compilation_context: &CommandSourceStack,
    lines: &[String],
) -> Result<Arc<dyn CommandFunction>, String> {
    let mut function_builder = FunctionBuilder::new();
    let mut i = 0;
    while i < lines.len() {
        let line_number = i + 1;
        let input_line = java_trim(&lines[i]).to_owned();
        let line = if should_concatenate_next_line(&input_line) {
            let mut builder = input_line;
            loop {
                i += 1;
                if i == lines.len() {
                    return Err("Line continuation at end of file".to_owned());
                }
                builder.pop();
                let inner_line = java_trim(&lines[i]);
                builder.push_str(inner_line);
                check_command_line_length(&builder)?;
                if !should_concatenate_next_line(&builder) {
                    break;
                }
            }
            builder
        } else {
            input_line
        };
        check_command_line_length(&line)?;
        let mut input = StringReader::new(line.clone());
        if input.can_read() && input.peek() != '#' {
            if input.peek() == '/' {
                input.skip();
                if input.peek() == '/' {
                    return Err(format!(
                        "Unknown or invalid command '{line}' on line {line_number} (if you intended to make a comment, use '#' not '//')"
                    ));
                }
                let name = input.read_unquoted_string();
                return Err(format!(
                    "Unknown or invalid command '{line}' on line {line_number} (did you mean '{name}'? Do not use a preceding forwards slash.)"
                ));
            }
            if input.peek() == '$' {
                function_builder.add_macro(&line[1..], line_number, compilation_context)?;
            } else {
                match parse_command(dispatcher, compilation_context, input) {
                    Ok(command) => function_builder.add_command(command),
                    Err(e) => {
                        return Err(format!(
                            "Whilst parsing command on line {line_number}: {}",
                            e.get_message()
                        ));
                    }
                }
            }
        }
        i += 1;
    }
    Ok(function_builder.build(id))
}

/// `CommandFunction.parseCommand`.
pub fn parse_command(
    dispatcher: &CommandDispatcher<CommandSourceStack>,
    compilation_context: &CommandSourceStack,
    input: StringReader,
) -> Result<Arc<dyn UnboundEntryAction>, CommandSyntaxException> {
    let string = input.get_string().to_owned();
    let parse = dispatcher.parse_reader(input, compilation_context.clone());
    Commands::validate_parse_results(&parse)?;
    match ContextChain::try_flatten(&parse.get_context().build(&string)) {
        Some(chain) => Ok(BuildContexts::unbound(&string, chain)),
        None => Err(CommandSyntaxException::built_in_exceptions()
            .dispatcher_unknown_command()
            .create_with_context(parse.get_reader())),
    }
}
