//! `FunctionArgument`.

use super::read_identifier;
use crate::commands::CommandSourceStack;
use crate::commands::functions::CommandFunction;
use crate::commands::message::dynamic_translatable;
use mc_brigadier::arguments::value;
use mc_brigadier::{
    ArgumentType, CommandContext, CommandSyntaxException, DynamicCommandExceptionType,
    ImmutableStringReader, ParsedValue, StringReader,
};
use mc_core::ident::Identifier;
use mc_protocol::packets::play::ArgumentTypeTemplate;
use std::sync::{Arc, LazyLock};

static ERROR_UNKNOWN_FUNCTION: LazyLock<DynamicCommandExceptionType<String>> =
    LazyLock::new(|| dynamic_translatable("arguments.function.unknown"));

/// `FunctionArgument.Result`: a function or a function tag, resolved against the server's function manager when used.
#[derive(Clone, PartialEq, Debug)]
pub enum FunctionResult {
    Function(Identifier),
    Tag(Identifier),
}

/// The functions a result resolved to, with the id it was given (`unwrap`).
#[derive(Clone)]
pub enum FunctionOrTag {
    Function(Arc<dyn CommandFunction>),
    Tag(Vec<Arc<dyn CommandFunction>>),
}

impl FunctionResult {
    fn id(&self) -> &Identifier {
        match self {
            FunctionResult::Function(id) | FunctionResult::Tag(id) => id,
        }
    }

    /// `create(context)`.
    pub fn create(
        &self,
        context: &CommandContext<CommandSourceStack>,
    ) -> Result<Vec<Arc<dyn CommandFunction>>, CommandSyntaxException> {
        match self.unwrap(context)?.1 {
            FunctionOrTag::Function(f) => Ok(vec![f]),
            FunctionOrTag::Tag(t) => Ok(t),
        }
    }

    /// `unwrap(context)`.
    pub fn unwrap(
        &self,
        context: &CommandContext<CommandSourceStack>,
    ) -> Result<(Identifier, FunctionOrTag), CommandSyntaxException> {
        let id = self.id().clone();
        let resolved = context.get_source().with_server(|server| match self {
            FunctionResult::Function(id) => server
                .functions()
                .get(id)
                .map(FunctionOrTag::Function)
                .ok_or_else(|| ERROR_UNKNOWN_FUNCTION.create(id.to_string())),
            // `getTag` of a missing tag is the empty list, never `null`.
            FunctionResult::Tag(id) => Ok(FunctionOrTag::Tag(server.functions().get_tag(id))),
        })?;
        Ok((id, resolved))
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct FunctionArgument;

impl FunctionArgument {
    pub fn functions() -> Self {
        Self
    }

    pub fn get_functions(
        context: &CommandContext<CommandSourceStack>,
        name: &str,
    ) -> Result<Vec<Arc<dyn CommandFunction>>, CommandSyntaxException> {
        context.get_argument::<FunctionResult>(name).create(context)
    }

    pub fn get_function_or_tag(
        context: &CommandContext<CommandSourceStack>,
        name: &str,
    ) -> Result<(Identifier, FunctionOrTag), CommandSyntaxException> {
        context.get_argument::<FunctionResult>(name).unwrap(context)
    }

    pub fn get_function_collection(
        context: &CommandContext<CommandSourceStack>,
        name: &str,
    ) -> Result<(Identifier, Vec<Arc<dyn CommandFunction>>), CommandSyntaxException> {
        let (id, resolved) = Self::get_function_or_tag(context, name)?;
        Ok((
            id,
            match resolved {
                FunctionOrTag::Function(f) => vec![f],
                FunctionOrTag::Tag(t) => t,
            },
        ))
    }
}

impl ArgumentType<CommandSourceStack> for FunctionArgument {
    fn parse(&self, reader: &mut StringReader) -> Result<ParsedValue, CommandSyntaxException> {
        if reader.can_read() && reader.peek() == '#' {
            reader.skip();
            Ok(value(FunctionResult::Tag(read_identifier(reader)?)))
        } else {
            Ok(value(FunctionResult::Function(read_identifier(reader)?)))
        }
    }

    fn get_examples(&self) -> Vec<String> {
        ["foo", "foo:bar", "#foo"].map(String::from).to_vec()
    }

    fn describe(&self) -> String {
        "FunctionArgument".to_owned()
    }
}

crate::register_argument_type!(FunctionArgument, |_a| ArgumentTypeTemplate::Function);
