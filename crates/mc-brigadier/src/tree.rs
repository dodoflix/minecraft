//! `com.mojang.brigadier.tree`. Vanilla's `RootCommandNode`/`LiteralCommandNode`/`ArgumentCommandNode`
//! subclasses are one struct here, distinguished by [`NodeKind`].
//!
//! Nodes are shared `Arc`s and the children/command are interior-mutable (`add_child` merges into
//! existing nodes like vanilla). Redirects hold strong references, so a tree with a redirect back into
//! itself (e.g. `execute ... run` -> root) is a reference cycle and is never freed; command trees live
//! for the whole server lifetime / until reload.

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::{Arc, RwLock};

use crate::arguments::ArgumentType;
use crate::builder::{
    ArgumentBuilder, LiteralArgumentBuilder, NodeBuilder, RequiredArgumentBuilder,
};
use crate::command::{AmbiguityConsumer, Command, Predicate, RedirectModifier};
use crate::context::{CommandContext, CommandContextBuilder, StringRange};
use crate::exceptions::{CommandSyntaxException, built_in_exceptions};
use crate::string_reader::{ImmutableStringReader, StringReader};
use crate::suggestion::{SuggestionProvider, Suggestions, SuggestionsBuilder};
use crate::utf16::{compare_to, len16, substring};

pub enum NodeKind<S> {
    Root,
    Literal {
        literal: String,
        literal_lower_case: String,
    },
    Argument {
        name: String,
        type_: Arc<dyn ArgumentType<S>>,
        custom_suggestions: Option<SuggestionProvider<S>>,
    },
}

struct State<S> {
    command: Option<Command<S>>,
    children: Vec<Arc<CommandNode<S>>>,
    by_name: HashMap<String, usize>,
    literals: HashMap<String, Arc<CommandNode<S>>>,
    arguments: Vec<Arc<CommandNode<S>>>,
}

pub struct CommandNode<S> {
    kind: NodeKind<S>,
    requirement: Predicate<S>,
    redirect: Option<Arc<CommandNode<S>>>,
    modifier: Option<RedirectModifier<S>>,
    forks: bool,
    state: RwLock<State<S>>,
}

impl<S: Clone + 'static> CommandNode<S> {
    /// `new RootCommandNode<>()`.
    pub fn new_root() -> Arc<Self> {
        Self::make(
            NodeKind::Root,
            None,
            Arc::new(|_| true),
            None,
            Some(Arc::new(|c: &CommandContext<S>| {
                Ok(vec![c.get_source().clone()])
            })),
            false,
        )
    }
}

impl<S: 'static> CommandNode<S> {
    fn make(
        kind: NodeKind<S>,
        command: Option<Command<S>>,
        requirement: Predicate<S>,
        redirect: Option<Arc<CommandNode<S>>>,
        modifier: Option<RedirectModifier<S>>,
        forks: bool,
    ) -> Arc<Self> {
        Arc::new(Self {
            kind,
            requirement,
            redirect,
            modifier,
            forks,
            state: RwLock::new(State {
                command,
                children: Vec::new(),
                by_name: HashMap::new(),
                literals: HashMap::new(),
                arguments: Vec::new(),
            }),
        })
    }

    /// `new LiteralCommandNode<>(...)`.
    pub fn new_literal(
        literal: impl Into<String>,
        command: Option<Command<S>>,
        requirement: Predicate<S>,
        redirect: Option<Arc<CommandNode<S>>>,
        modifier: Option<RedirectModifier<S>>,
        forks: bool,
    ) -> Arc<Self> {
        let literal = literal.into();
        let literal_lower_case = literal.to_lowercase();
        Self::make(
            NodeKind::Literal {
                literal,
                literal_lower_case,
            },
            command,
            requirement,
            redirect,
            modifier,
            forks,
        )
    }

    /// `new ArgumentCommandNode<>(...)`.
    #[allow(clippy::too_many_arguments)]
    pub fn new_argument(
        name: impl Into<String>,
        type_: Arc<dyn ArgumentType<S>>,
        command: Option<Command<S>>,
        requirement: Predicate<S>,
        redirect: Option<Arc<CommandNode<S>>>,
        modifier: Option<RedirectModifier<S>>,
        forks: bool,
        custom_suggestions: Option<SuggestionProvider<S>>,
    ) -> Arc<Self> {
        Self::make(
            NodeKind::Argument {
                name: name.into(),
                type_,
                custom_suggestions,
            },
            command,
            requirement,
            redirect,
            modifier,
            forks,
        )
    }
}

impl<S: 'static> CommandNode<S> {
    fn read(&self) -> std::sync::RwLockReadGuard<'_, State<S>> {
        self.state.read().unwrap_or_else(|e| e.into_inner())
    }

    pub fn kind(&self) -> &NodeKind<S> {
        &self.kind
    }

    pub fn is_root(&self) -> bool {
        matches!(self.kind, NodeKind::Root)
    }

    /// `LiteralCommandNode.getLiteral()`.
    pub fn get_literal(&self) -> Option<&str> {
        match &self.kind {
            NodeKind::Literal { literal, .. } => Some(literal),
            _ => None,
        }
    }

    /// `ArgumentCommandNode.getType()`.
    pub fn get_type(&self) -> Option<&Arc<dyn ArgumentType<S>>> {
        match &self.kind {
            NodeKind::Argument { type_, .. } => Some(type_),
            _ => None,
        }
    }

    /// `ArgumentCommandNode.getCustomSuggestions()`.
    pub fn get_custom_suggestions(&self) -> Option<&SuggestionProvider<S>> {
        match &self.kind {
            NodeKind::Argument {
                custom_suggestions, ..
            } => custom_suggestions.as_ref(),
            _ => None,
        }
    }

    pub fn get_command(&self) -> Option<Command<S>> {
        self.read().command.clone()
    }

    /// Children in insertion order.
    pub fn get_children(&self) -> Vec<Arc<CommandNode<S>>> {
        self.read().children.clone()
    }

    pub fn has_children(&self) -> bool {
        !self.read().children.is_empty()
    }

    pub fn get_child(&self, name: &str) -> Option<Arc<CommandNode<S>>> {
        let state = self.read();
        state.by_name.get(name).map(|&i| state.children[i].clone())
    }

    pub fn get_redirect(&self) -> Option<&Arc<CommandNode<S>>> {
        self.redirect.as_ref()
    }

    pub fn get_redirect_modifier(&self) -> Option<RedirectModifier<S>> {
        self.modifier.clone()
    }

    pub fn can_use(&self, source: &S) -> bool {
        (self.requirement)(source)
    }

    pub fn get_requirement(&self) -> &Predicate<S> {
        &self.requirement
    }

    pub fn is_fork(&self) -> bool {
        self.forks
    }

    pub fn get_name(&self) -> &str {
        match &self.kind {
            NodeKind::Root => "",
            NodeKind::Literal { literal, .. } => literal,
            NodeKind::Argument { name, .. } => name,
        }
    }

    pub fn get_usage_text(&self) -> String {
        match &self.kind {
            NodeKind::Root => String::new(),
            NodeKind::Literal { literal, .. } => literal.clone(),
            NodeKind::Argument { name, .. } => format!("<{name}>"),
        }
    }

    fn get_sorted_key(&self) -> &str {
        self.get_name()
    }

    pub fn get_examples(&self) -> Vec<String> {
        match &self.kind {
            NodeKind::Root => Vec::new(),
            NodeKind::Literal { literal, .. } => vec![literal.clone()],
            NodeKind::Argument { type_, .. } => type_.get_examples(),
        }
    }

    /// `compareTo`: literals sort before arguments, then by name (UTF-16 order).
    pub fn compare_to(&self, other: &CommandNode<S>) -> Ordering {
        let is_literal = |n: &CommandNode<S>| matches!(n.kind, NodeKind::Literal { .. });
        match (is_literal(self), is_literal(other)) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            _ => compare_to(self.get_sorted_key(), other.get_sorted_key()),
        }
    }

    /// `getRelevantNodes`: the matching literal if the next word is one, else all argument children.
    pub fn get_relevant_nodes(&self, input: &StringReader) -> Vec<Arc<CommandNode<S>>> {
        let state = self.read();
        if state.literals.is_empty() {
            return state.arguments.clone();
        }
        let mut len = 0;
        while input.can_read_n(len + 1) && input.peek_at(len) != ' ' {
            len += 1;
        }
        let cursor = input.get_cursor();
        let text = substring(input.get_string(), cursor, cursor + len);
        match state.literals.get(&text) {
            Some(literal) => vec![literal.clone()],
            None => state.arguments.clone(),
        }
    }

    /// `isValidInput`.
    pub fn is_valid_input(&self, input: &str) -> bool {
        match &self.kind {
            NodeKind::Root => false,
            NodeKind::Literal { literal, .. } => {
                Self::parse_literal(literal, &mut StringReader::new(input)).is_some()
            }
            NodeKind::Argument { type_, .. } => {
                let mut reader = StringReader::new(input);
                type_.parse(&mut reader).is_ok() && (!reader.can_read() || reader.peek() == ' ')
            }
        }
    }

    fn parse_literal(literal: &str, reader: &mut StringReader) -> Option<usize> {
        let start = reader.get_cursor();
        let len = len16(literal);
        if reader.can_read_n(len) {
            let end = start + len;
            if substring(reader.get_string(), start, end) == literal {
                reader.set_cursor(end);
                if !reader.can_read() || reader.peek() == ' ' {
                    return Some(end);
                }
                reader.set_cursor(start);
            }
        }
        None
    }

    /// `listSuggestions`.
    pub fn list_suggestions(
        &self,
        context: &CommandContext<S>,
        mut builder: SuggestionsBuilder,
    ) -> Result<Suggestions, CommandSyntaxException> {
        match &self.kind {
            NodeKind::Root => Ok(Suggestions::empty()),
            NodeKind::Literal {
                literal,
                literal_lower_case,
            } => {
                if literal_lower_case.starts_with(builder.get_remaining_lower_case()) {
                    Ok(builder.suggest(literal.as_str()).build())
                } else {
                    Ok(Suggestions::empty())
                }
            }
            NodeKind::Argument {
                type_,
                custom_suggestions,
                ..
            } => match custom_suggestions {
                None => Ok(type_.list_suggestions(context, builder)),
                Some(provider) => provider(context, builder),
            },
        }
    }
}

impl<S: Clone + 'static> CommandNode<S> {
    /// `addChild`: merges into an existing child of the same name. Panics when adding a root node
    /// (vanilla `UnsupportedOperationException`).
    pub fn add_child(&self, node: Arc<CommandNode<S>>) {
        assert!(
            !node.is_root(),
            "Cannot add a RootCommandNode as a child to any other CommandNode"
        );
        let existing = {
            let mut state = self.state.write().unwrap_or_else(|e| e.into_inner());
            match state
                .by_name
                .get(node.get_name())
                .map(|&i| state.children[i].clone())
            {
                Some(child) => child,
                None => {
                    let index = state.children.len();
                    state.by_name.insert(node.get_name().to_owned(), index);
                    state.children.push(node.clone());
                    match &node.kind {
                        NodeKind::Literal { literal, .. } => {
                            state.literals.insert(literal.clone(), node.clone());
                        }
                        NodeKind::Argument { .. } => state.arguments.push(node.clone()),
                        NodeKind::Root => {}
                    }
                    return;
                }
            }
        };
        if let Some(command) = node.get_command() {
            existing
                .state
                .write()
                .unwrap_or_else(|e| e.into_inner())
                .command = Some(command);
        }
        for grandchild in node.get_children() {
            existing.add_child(grandchild);
        }
    }

    /// `parse`: consumes this node's input and records it in `context_builder`.
    pub fn parse(
        self: &Arc<Self>,
        reader: &mut StringReader,
        context_builder: &mut CommandContextBuilder<S>,
    ) -> Result<(), CommandSyntaxException> {
        match &self.kind {
            NodeKind::Root => Ok(()),
            NodeKind::Literal { literal, .. } => {
                let start = reader.get_cursor();
                match Self::parse_literal(literal, reader) {
                    Some(end) => {
                        context_builder.with_node(self.clone(), StringRange::between(start, end));
                        Ok(())
                    }
                    None => Err(built_in_exceptions()
                        .literal_incorrect()
                        .create_with_context(reader, literal.clone())),
                }
            }
            NodeKind::Argument { name, type_, .. } => {
                let start = reader.get_cursor();
                let result = type_.parse_with_source(reader, context_builder.get_source())?;
                let parsed =
                    crate::context::ParsedArgument::new(start, reader.get_cursor(), result);
                let range = parsed.get_range();
                context_builder.with_argument(name.clone(), parsed);
                context_builder.with_node(self.clone(), range);
                Ok(())
            }
        }
    }

    /// `createBuilder`: a builder equivalent to this node (without children). Panics for the root,
    /// like vanilla's `IllegalStateException`.
    pub fn create_builder(&self) -> NodeBuilder<S> {
        let command = self.get_command();
        match &self.kind {
            NodeKind::Root => panic!("Cannot convert root into a builder"),
            NodeKind::Literal { literal, .. } => {
                let mut b = LiteralArgumentBuilder::literal(literal.clone())
                    .requires_predicate(self.requirement.clone());
                b = b.forward_shared(self.redirect.clone(), self.modifier.clone(), self.forks);
                if let Some(c) = command {
                    b = b.executes_command(c);
                }
                NodeBuilder::Literal(b)
            }
            NodeKind::Argument {
                name,
                type_,
                custom_suggestions,
            } => {
                let mut b = RequiredArgumentBuilder::argument_shared(name.clone(), type_.clone())
                    .requires_predicate(self.requirement.clone());
                b = b.forward_shared(self.redirect.clone(), self.modifier.clone(), self.forks);
                b = b.suggests_provider(custom_suggestions.clone());
                if let Some(c) = command {
                    b = b.executes_command(c);
                }
                NodeBuilder::Argument(b)
            }
        }
    }

    /// `findAmbiguities`: reports sibling pairs that accept the same example input.
    pub fn find_ambiguities(self: &Arc<Self>, consumer: AmbiguityConsumer<'_, S>) {
        let children = self.get_children();
        let mut matches: HashSet<String> = HashSet::new();
        for child in &children {
            for sibling in &children {
                if !Arc::ptr_eq(child, sibling) {
                    for input in child.get_examples() {
                        if sibling.is_valid_input(&input) {
                            matches.insert(input);
                        }
                    }
                    if !matches.is_empty() {
                        consumer(self, child, sibling, &matches);
                        matches = HashSet::new();
                    }
                }
            }
            child.find_ambiguities(&mut *consumer);
        }
    }
}

impl<S: 'static> PartialEq for CommandNode<S> {
    /// Vanilla `equals`: same kind/name (and argument type), same command instance and equal children.
    fn eq(&self, other: &Self) -> bool {
        if std::ptr::eq(self, other) {
            return true;
        }
        let kind_eq = match (&self.kind, &other.kind) {
            (NodeKind::Root, NodeKind::Root) => true,
            (NodeKind::Literal { literal: a, .. }, NodeKind::Literal { literal: b, .. }) => a == b,
            (
                NodeKind::Argument {
                    name: a, type_: ta, ..
                },
                NodeKind::Argument {
                    name: b, type_: tb, ..
                },
            ) => a == b && ta.equals(&**tb),
            _ => false,
        };
        if !kind_eq {
            return false;
        }
        let (a, b) = (self.read(), other.read());
        let command_eq = match (&a.command, &b.command) {
            (None, None) => true,
            (Some(x), Some(y)) => std::ptr::addr_eq(Arc::as_ptr(x), Arc::as_ptr(y)),
            _ => false,
        };
        command_eq
            && a.children.len() == b.children.len()
            && a.children.iter().all(|c| {
                b.by_name
                    .get(c.get_name())
                    .is_some_and(|&i| **c == *b.children[i])
            })
    }
}

impl<S: 'static> fmt::Display for CommandNode<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            NodeKind::Root => f.write_str("<root>"),
            NodeKind::Literal { literal, .. } => write!(f, "<literal {literal}>"),
            NodeKind::Argument { name, type_, .. } => {
                write!(f, "<argument {name}:{}>", type_.describe())
            }
        }
    }
}

impl<S: 'static> fmt::Debug for CommandNode<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}
