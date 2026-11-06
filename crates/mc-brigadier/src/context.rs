//! `com.mojang.brigadier.context`.

use std::any::Any;
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use crate::arguments::ParsedValue;
use crate::command::{Command, RedirectModifier, ResultConsumer};
use crate::exceptions::CommandSyntaxException;
use crate::string_reader::ImmutableStringReader;
use crate::tree::CommandNode;
use crate::utf16::substring;

/// `StringRange`: a half-open range of UTF-16 unit indices.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StringRange {
    start: usize,
    end: usize,
}

impl StringRange {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    pub fn at(pos: usize) -> Self {
        Self::new(pos, pos)
    }

    pub fn between(start: usize, end: usize) -> Self {
        Self::new(start, end)
    }

    pub fn encompassing(a: Self, b: Self) -> Self {
        Self::new(a.start.min(b.start), a.end.max(b.end))
    }

    pub fn get_start(&self) -> usize {
        self.start
    }

    pub fn get_end(&self) -> usize {
        self.end
    }

    pub fn get(&self, reader: &dyn ImmutableStringReader) -> String {
        self.get_str(reader.get_string())
    }

    pub fn get_str(&self, string: &str) -> String {
        substring(string, self.start, self.end)
    }

    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }

    pub fn get_length(&self) -> usize {
        self.end - self.start
    }
}

impl fmt::Display for StringRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "StringRange{{start={}, end={}}}", self.start, self.end)
    }
}

/// `ParsedArgument` with its type-erased result.
#[derive(Clone)]
pub struct ParsedArgument {
    range: StringRange,
    result: ParsedValue,
}

impl ParsedArgument {
    pub fn new(start: usize, end: usize, result: ParsedValue) -> Self {
        Self {
            range: StringRange::between(start, end),
            result,
        }
    }

    pub fn get_range(&self) -> StringRange {
        self.range
    }

    pub fn get_result(&self) -> &ParsedValue {
        &self.result
    }
}

/// `ParsedCommandNode`.
pub struct ParsedCommandNode<S> {
    node: Arc<CommandNode<S>>,
    range: StringRange,
}

impl<S: 'static> Clone for ParsedCommandNode<S> {
    fn clone(&self) -> Self {
        Self {
            node: self.node.clone(),
            range: self.range,
        }
    }
}

impl<S: 'static> ParsedCommandNode<S> {
    pub fn new(node: Arc<CommandNode<S>>, range: StringRange) -> Self {
        Self { node, range }
    }

    pub fn get_node(&self) -> &Arc<CommandNode<S>> {
        &self.node
    }

    pub fn get_range(&self) -> StringRange {
        self.range
    }
}

impl<S: 'static> fmt::Display for ParsedCommandNode<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}@{}", self.node, self.range)
    }
}

/// Vanilla throws `IllegalArgumentException` from `getArgument`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IllegalArgumentException(pub String);

impl fmt::Display for IllegalArgumentException {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for IllegalArgumentException {}

/// `CommandContext`. Cheap to clone (shared arguments/nodes/children).
pub struct CommandContext<S> {
    source: S,
    input: Arc<str>,
    arguments: Arc<HashMap<String, ParsedArgument>>,
    command: Option<Command<S>>,
    root_node: Arc<CommandNode<S>>,
    nodes: Arc<Vec<ParsedCommandNode<S>>>,
    range: StringRange,
    child: Option<Arc<CommandContext<S>>>,
    modifier: Option<RedirectModifier<S>>,
    forks: bool,
}

impl<S: Clone + 'static> Clone for CommandContext<S> {
    fn clone(&self) -> Self {
        self.copy_for(self.source.clone())
    }
}

impl<S: 'static> CommandContext<S> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        source: S,
        input: Arc<str>,
        arguments: Arc<HashMap<String, ParsedArgument>>,
        command: Option<Command<S>>,
        root_node: Arc<CommandNode<S>>,
        nodes: Arc<Vec<ParsedCommandNode<S>>>,
        range: StringRange,
        child: Option<Arc<CommandContext<S>>>,
        modifier: Option<RedirectModifier<S>>,
        forks: bool,
    ) -> Self {
        Self {
            source,
            input,
            arguments,
            command,
            root_node,
            nodes,
            range,
            child,
            modifier,
            forks,
        }
    }

    pub fn copy_for(&self, source: S) -> CommandContext<S> {
        CommandContext {
            source,
            input: self.input.clone(),
            arguments: self.arguments.clone(),
            command: self.command.clone(),
            root_node: self.root_node.clone(),
            nodes: self.nodes.clone(),
            range: self.range,
            child: self.child.clone(),
            modifier: self.modifier.clone(),
            forks: self.forks,
        }
    }

    pub fn get_child(&self) -> Option<&Arc<CommandContext<S>>> {
        self.child.as_ref()
    }

    pub fn get_last_child(&self) -> &CommandContext<S> {
        let mut result = self;
        while let Some(child) = &result.child {
            result = child;
        }
        result
    }

    pub fn get_command(&self) -> Option<&Command<S>> {
        self.command.as_ref()
    }

    pub fn get_source(&self) -> &S {
        &self.source
    }

    /// The type-erased value parsed for argument `name`.
    pub fn get_argument_value(&self, name: &str) -> Option<&ParsedValue> {
        self.arguments.get(name).map(ParsedArgument::get_result)
    }

    pub fn get_argument_range(&self, name: &str) -> Option<StringRange> {
        self.arguments.get(name).map(ParsedArgument::get_range)
    }

    /// `getArgument(name, Class)`.
    pub fn try_get_argument<T: Any>(&self, name: &str) -> Result<&T, IllegalArgumentException> {
        let argument = self.arguments.get(name).ok_or_else(|| {
            IllegalArgumentException(format!("No such argument '{name}' exists on this command"))
        })?;
        (&*argument.result as &dyn Any)
            .downcast_ref::<T>()
            .ok_or_else(|| {
                IllegalArgumentException(format!(
                    "Argument '{name}' is not of the requested type {}",
                    std::any::type_name::<T>()
                ))
            })
    }

    /// Like [`try_get_argument`](Self::try_get_argument) but panics, as vanilla's unchecked
    /// `IllegalArgumentException` does (a wrong name/type is a programming error).
    pub fn get_argument<T: Any>(&self, name: &str) -> &T {
        match self.try_get_argument(name) {
            Ok(v) => v,
            Err(e) => panic!("{e}"),
        }
    }

    pub fn get_redirect_modifier(&self) -> Option<&RedirectModifier<S>> {
        self.modifier.as_ref()
    }

    pub fn get_range(&self) -> StringRange {
        self.range
    }

    pub fn get_input(&self) -> &str {
        &self.input
    }

    pub fn get_root_node(&self) -> &Arc<CommandNode<S>> {
        &self.root_node
    }

    pub fn get_nodes(&self) -> &[ParsedCommandNode<S>] {
        &self.nodes
    }

    pub fn has_nodes(&self) -> bool {
        !self.nodes.is_empty()
    }

    pub fn is_forked(&self) -> bool {
        self.forks
    }
}

/// `SuggestionContext`.
pub struct SuggestionContext<'a, S> {
    pub context: &'a CommandContextBuilder<S>,
    pub parent: Arc<CommandNode<S>>,
    pub start_pos: usize,
}

/// `CommandContextBuilder` (vanilla's `getDispatcher()` back-reference is not kept).
pub struct CommandContextBuilder<S> {
    arguments: HashMap<String, ParsedArgument>,
    root_node: Arc<CommandNode<S>>,
    nodes: Vec<ParsedCommandNode<S>>,
    source: S,
    command: Option<Command<S>>,
    child: Option<Box<CommandContextBuilder<S>>>,
    range: StringRange,
    modifier: Option<RedirectModifier<S>>,
    forks: bool,
}

impl<S: Clone + 'static> Clone for CommandContextBuilder<S> {
    fn clone(&self) -> Self {
        Self {
            arguments: self.arguments.clone(),
            root_node: self.root_node.clone(),
            nodes: self.nodes.clone(),
            source: self.source.clone(),
            command: self.command.clone(),
            child: self.child.clone(),
            range: self.range,
            modifier: self.modifier.clone(),
            forks: self.forks,
        }
    }
}

impl<S: 'static> CommandContextBuilder<S> {
    pub fn new(source: S, root_node: Arc<CommandNode<S>>, start: usize) -> Self {
        Self {
            arguments: HashMap::new(),
            root_node,
            nodes: Vec::new(),
            source,
            command: None,
            child: None,
            range: StringRange::at(start),
            modifier: None,
            forks: false,
        }
    }

    pub fn with_source(&mut self, source: S) -> &mut Self {
        self.source = source;
        self
    }

    pub fn get_source(&self) -> &S {
        &self.source
    }

    pub fn get_root_node(&self) -> &Arc<CommandNode<S>> {
        &self.root_node
    }

    pub fn with_argument(
        &mut self,
        name: impl Into<String>,
        argument: ParsedArgument,
    ) -> &mut Self {
        self.arguments.insert(name.into(), argument);
        self
    }

    pub fn get_arguments(&self) -> &HashMap<String, ParsedArgument> {
        &self.arguments
    }

    pub fn with_command(&mut self, command: Option<Command<S>>) -> &mut Self {
        self.command = command;
        self
    }

    pub fn with_node(&mut self, node: Arc<CommandNode<S>>, range: StringRange) -> &mut Self {
        self.range = StringRange::encompassing(self.range, range);
        self.modifier = node.get_redirect_modifier();
        self.forks = node.is_fork();
        self.nodes.push(ParsedCommandNode::new(node, range));
        self
    }

    pub fn with_child(&mut self, child: CommandContextBuilder<S>) -> &mut Self {
        self.child = Some(Box::new(child));
        self
    }

    pub fn get_child(&self) -> Option<&CommandContextBuilder<S>> {
        self.child.as_deref()
    }

    pub fn get_last_child(&self) -> &CommandContextBuilder<S> {
        let mut result = self;
        while let Some(child) = result.get_child() {
            result = child;
        }
        result
    }

    pub fn get_command(&self) -> Option<&Command<S>> {
        self.command.as_ref()
    }

    pub fn get_nodes(&self) -> &[ParsedCommandNode<S>] {
        &self.nodes
    }

    pub fn get_range(&self) -> StringRange {
        self.range
    }

    /// `findSuggestionContext`; panics where vanilla throws `IllegalStateException`.
    pub fn find_suggestion_context(&self, cursor: usize) -> SuggestionContext<'_, S> {
        assert!(
            self.range.get_start() <= cursor,
            "Can't find node before cursor"
        );
        if self.range.get_end() < cursor {
            if let Some(child) = &self.child {
                child.find_suggestion_context(cursor)
            } else if let Some(last) = self.nodes.last() {
                SuggestionContext {
                    context: self,
                    parent: last.node.clone(),
                    start_pos: last.range.get_end() + 1,
                }
            } else {
                SuggestionContext {
                    context: self,
                    parent: self.root_node.clone(),
                    start_pos: self.range.get_start(),
                }
            }
        } else {
            let mut prev = self.root_node.clone();
            for node in &self.nodes {
                let range = node.range;
                if range.get_start() <= cursor && cursor <= range.get_end() {
                    return SuggestionContext {
                        context: self,
                        parent: prev,
                        start_pos: range.get_start(),
                    };
                }
                prev = node.node.clone();
            }
            SuggestionContext {
                context: self,
                parent: prev,
                start_pos: self.range.get_start(),
            }
        }
    }
}

impl<S: Clone + 'static> CommandContextBuilder<S> {
    /// `copy()`. Like vanilla, the redirect modifier is *not* copied (every `with_node` sets it).
    pub fn copy(&self) -> Self {
        Self {
            arguments: self.arguments.clone(),
            root_node: self.root_node.clone(),
            nodes: self.nodes.clone(),
            source: self.source.clone(),
            command: self.command.clone(),
            child: self.child.clone(),
            range: self.range,
            modifier: None,
            forks: self.forks,
        }
    }

    pub fn build(&self, input: &str) -> CommandContext<S> {
        self.build_shared(&Arc::from(input))
    }

    fn build_shared(&self, input: &Arc<str>) -> CommandContext<S> {
        CommandContext {
            source: self.source.clone(),
            input: input.clone(),
            arguments: Arc::new(self.arguments.clone()),
            command: self.command.clone(),
            root_node: self.root_node.clone(),
            nodes: Arc::new(self.nodes.clone()),
            range: self.range,
            child: self.child.as_ref().map(|c| Arc::new(c.build_shared(input))),
            modifier: self.modifier.clone(),
            forks: self.forks,
        }
    }
}

/// `ContextChain.Stage`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Modify,
    Execute,
}

/// `ContextChain`: a context tree flattened into modifier stages plus the executable context.
pub struct ContextChain<S> {
    modifiers: Vec<Arc<CommandContext<S>>>,
    executable: Arc<CommandContext<S>>,
}

impl<S: 'static> Clone for ContextChain<S> {
    fn clone(&self) -> Self {
        Self {
            modifiers: self.modifiers.clone(),
            executable: self.executable.clone(),
        }
    }
}

impl<S: Clone + 'static> ContextChain<S> {
    /// Panics if `executable` has no command (vanilla `IllegalArgumentException`).
    pub fn new(modifiers: Vec<Arc<CommandContext<S>>>, executable: Arc<CommandContext<S>>) -> Self {
        assert!(
            executable.get_command().is_some(),
            "Last command in chain must be executable"
        );
        Self {
            modifiers,
            executable,
        }
    }

    pub fn try_flatten(root_context: &CommandContext<S>) -> Option<ContextChain<S>> {
        let mut modifiers = Vec::new();
        let mut current = Arc::new(root_context.clone());
        loop {
            match current.get_child().cloned() {
                None => {
                    return current.get_command().is_some().then(|| ContextChain {
                        modifiers,
                        executable: current,
                    });
                }
                Some(child) => {
                    modifiers.push(current);
                    current = child;
                }
            }
        }
    }

    pub fn run_modifier(
        modifier: &CommandContext<S>,
        source: S,
        result_consumer: &ResultConsumer<S>,
        forked_mode: bool,
    ) -> Result<Vec<S>, CommandSyntaxException> {
        let Some(source_modifier) = modifier.get_redirect_modifier() else {
            return Ok(vec![source]);
        };
        let context_to_use = modifier.copy_for(source);
        match source_modifier(&context_to_use) {
            Ok(sources) => Ok(sources),
            Err(e) => {
                result_consumer(&context_to_use, false, 0);
                if forked_mode { Ok(Vec::new()) } else { Err(e) }
            }
        }
    }

    pub fn run_executable(
        executable: &CommandContext<S>,
        source: S,
        result_consumer: &ResultConsumer<S>,
        forked_mode: bool,
    ) -> Result<i32, CommandSyntaxException> {
        let context_to_use = executable.copy_for(source);
        let command = executable
            .get_command()
            .expect("executable context has a command");
        match command(&context_to_use) {
            Ok(result) => {
                result_consumer(&context_to_use, true, result);
                Ok(if forked_mode { 1 } else { result })
            }
            Err(e) => {
                result_consumer(&context_to_use, false, 0);
                if forked_mode { Ok(0) } else { Err(e) }
            }
        }
    }

    pub fn execute_all(
        &self,
        source: S,
        result_consumer: &ResultConsumer<S>,
    ) -> Result<i32, CommandSyntaxException> {
        if self.modifiers.is_empty() {
            return Self::run_executable(&self.executable, source, result_consumer, false);
        }
        let mut forked_mode = false;
        let mut current_sources = vec![source];
        for modifier in &self.modifiers {
            forked_mode |= modifier.is_forked();
            let mut next_sources = Vec::new();
            for source_to_run in current_sources {
                next_sources.extend(Self::run_modifier(
                    modifier,
                    source_to_run,
                    result_consumer,
                    forked_mode,
                )?);
            }
            if next_sources.is_empty() {
                return Ok(0);
            }
            current_sources = next_sources;
        }
        let mut result = 0i32;
        for execution_source in current_sources {
            result = result.wrapping_add(Self::run_executable(
                &self.executable,
                execution_source,
                result_consumer,
                forked_mode,
            )?);
        }
        Ok(result)
    }

    pub fn get_stage(&self) -> Stage {
        if self.modifiers.is_empty() {
            Stage::Execute
        } else {
            Stage::Modify
        }
    }

    pub fn get_top_context(&self) -> &CommandContext<S> {
        self.modifiers.first().unwrap_or(&self.executable)
    }

    pub fn next_stage(&self) -> Option<ContextChain<S>> {
        if self.modifiers.is_empty() {
            None
        } else {
            Some(ContextChain {
                modifiers: self.modifiers[1..].to_vec(),
                executable: self.executable.clone(),
            })
        }
    }
}
