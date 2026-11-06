//! `com.mojang.brigadier.builder`. Builders are consumed by value so calls chain like vanilla's
//! fluent API (`literal("a").then(argument(..).executes(..))`).

use std::sync::Arc;

use crate::arguments::ArgumentType;
use crate::command::{Command, Predicate, RedirectModifier, SingleRedirectModifier};
use crate::context::CommandContext;
use crate::exceptions::CommandSyntaxException;
use crate::suggestion::{SuggestionProvider, Suggestions, SuggestionsBuilder};
use crate::tree::CommandNode;

/// State shared by every builder (the fields of vanilla's abstract `ArgumentBuilder`).
pub struct BuilderData<S> {
    /// Children are collected in a root node so same-named children merge like in a real tree.
    arguments: Arc<CommandNode<S>>,
    command: Option<Command<S>>,
    requirement: Predicate<S>,
    target: Option<Arc<CommandNode<S>>>,
    modifier: Option<RedirectModifier<S>>,
    forks: bool,
}

impl<S: Clone + 'static> BuilderData<S> {
    fn new() -> Self {
        Self {
            arguments: CommandNode::new_root(),
            command: None,
            requirement: Arc::new(|_| true),
            target: None,
            modifier: None,
            forks: false,
        }
    }
}

/// `ArgumentBuilder<S, T>`.
pub trait ArgumentBuilder<S: Clone + 'static>: Sized {
    fn data(&self) -> &BuilderData<S>;
    fn data_mut(&mut self) -> &mut BuilderData<S>;

    /// Builds a fresh node (can be called repeatedly).
    fn build(&self) -> Arc<CommandNode<S>>;

    /// Panics if this builder redirects (vanilla `IllegalStateException`).
    fn then<B: ArgumentBuilder<S>>(self, argument: B) -> Self {
        self.then_node(argument.build())
    }

    fn then_node(mut self, argument: Arc<CommandNode<S>>) -> Self {
        let data = self.data_mut();
        assert!(
            data.target.is_none(),
            "Cannot add children to a redirected node"
        );
        data.arguments.add_child(argument);
        self
    }

    fn get_arguments(&self) -> Vec<Arc<CommandNode<S>>> {
        self.data().arguments.get_children()
    }

    fn executes(
        self,
        command: impl Fn(&CommandContext<S>) -> Result<i32, CommandSyntaxException>
        + Send
        + Sync
        + 'static,
    ) -> Self {
        self.executes_command(Arc::new(command))
    }

    fn executes_command(mut self, command: Command<S>) -> Self {
        self.data_mut().command = Some(command);
        self
    }

    fn get_command(&self) -> Option<&Command<S>> {
        self.data().command.as_ref()
    }

    fn requires(self, requirement: impl Fn(&S) -> bool + Send + Sync + 'static) -> Self {
        self.requires_predicate(Arc::new(requirement))
    }

    fn requires_predicate(mut self, requirement: Predicate<S>) -> Self {
        self.data_mut().requirement = requirement;
        self
    }

    fn get_requirement(&self) -> &Predicate<S> {
        &self.data().requirement
    }

    fn redirect(self, target: Arc<CommandNode<S>>) -> Self {
        self.forward_shared(Some(target), None, false)
    }

    /// `redirect(target, SingleRedirectModifier)`.
    fn redirect_with(
        self,
        target: Arc<CommandNode<S>>,
        modifier: impl Fn(&CommandContext<S>) -> Result<S, CommandSyntaxException>
        + Send
        + Sync
        + 'static,
    ) -> Self {
        self.redirect_single(target, Some(Arc::new(modifier)))
    }

    fn redirect_single(
        self,
        target: Arc<CommandNode<S>>,
        modifier: Option<SingleRedirectModifier<S>>,
    ) -> Self {
        let modifier: Option<RedirectModifier<S>> = modifier.map(|m| {
            Arc::new(move |c: &CommandContext<S>| m(c).map(|s| vec![s])) as RedirectModifier<S>
        });
        self.forward_shared(Some(target), modifier, false)
    }

    fn fork(
        self,
        target: Arc<CommandNode<S>>,
        modifier: impl Fn(&CommandContext<S>) -> Result<Vec<S>, CommandSyntaxException>
        + Send
        + Sync
        + 'static,
    ) -> Self {
        self.forward_shared(Some(target), Some(Arc::new(modifier)), true)
    }

    /// `forward(target, modifier, fork)`. Panics if children were already added.
    fn forward(
        self,
        target: Arc<CommandNode<S>>,
        modifier: Option<RedirectModifier<S>>,
        fork: bool,
    ) -> Self {
        self.forward_shared(Some(target), modifier, fork)
    }

    /// Like [`forward`](Self::forward) but accepts "no target" (used when copying a node).
    fn forward_shared(
        mut self,
        target: Option<Arc<CommandNode<S>>>,
        modifier: Option<RedirectModifier<S>>,
        fork: bool,
    ) -> Self {
        let data = self.data_mut();
        assert!(
            !data.arguments.has_children(),
            "Cannot forward a node with children"
        );
        data.target = target;
        data.modifier = modifier;
        data.forks = fork;
        self
    }

    fn get_redirect(&self) -> Option<&Arc<CommandNode<S>>> {
        self.data().target.as_ref()
    }

    fn get_redirect_modifier(&self) -> Option<&RedirectModifier<S>> {
        self.data().modifier.as_ref()
    }

    fn is_fork(&self) -> bool {
        self.data().forks
    }
}

/// `LiteralArgumentBuilder`.
pub struct LiteralArgumentBuilder<S> {
    data: BuilderData<S>,
    literal: String,
}

impl<S: Clone + 'static> LiteralArgumentBuilder<S> {
    pub fn literal(name: impl Into<String>) -> Self {
        Self {
            data: BuilderData::new(),
            literal: name.into(),
        }
    }

    pub fn get_literal(&self) -> &str {
        &self.literal
    }
}

impl<S: Clone + 'static> ArgumentBuilder<S> for LiteralArgumentBuilder<S> {
    fn data(&self) -> &BuilderData<S> {
        &self.data
    }

    fn data_mut(&mut self) -> &mut BuilderData<S> {
        &mut self.data
    }

    fn build(&self) -> Arc<CommandNode<S>> {
        let d = &self.data;
        let result = CommandNode::new_literal(
            self.literal.clone(),
            d.command.clone(),
            d.requirement.clone(),
            d.target.clone(),
            d.modifier.clone(),
            d.forks,
        );
        for argument in d.arguments.get_children() {
            result.add_child(argument);
        }
        result
    }
}

/// `RequiredArgumentBuilder`.
pub struct RequiredArgumentBuilder<S> {
    data: BuilderData<S>,
    name: String,
    type_: Arc<dyn ArgumentType<S>>,
    suggestions_provider: Option<SuggestionProvider<S>>,
}

impl<S: Clone + 'static> RequiredArgumentBuilder<S> {
    pub fn argument(name: impl Into<String>, type_: impl ArgumentType<S>) -> Self {
        Self::argument_shared(name, Arc::new(type_))
    }

    pub fn argument_shared(name: impl Into<String>, type_: Arc<dyn ArgumentType<S>>) -> Self {
        Self {
            data: BuilderData::new(),
            name: name.into(),
            type_,
            suggestions_provider: None,
        }
    }

    pub fn suggests(
        self,
        provider: impl Fn(
            &CommandContext<S>,
            SuggestionsBuilder,
        ) -> Result<Suggestions, CommandSyntaxException>
        + Send
        + Sync
        + 'static,
    ) -> Self {
        self.suggests_provider(Some(Arc::new(provider)))
    }

    pub fn suggests_provider(mut self, provider: Option<SuggestionProvider<S>>) -> Self {
        self.suggestions_provider = provider;
        self
    }

    pub fn get_suggestions_provider(&self) -> Option<&SuggestionProvider<S>> {
        self.suggestions_provider.as_ref()
    }

    pub fn get_type(&self) -> &Arc<dyn ArgumentType<S>> {
        &self.type_
    }

    pub fn get_name(&self) -> &str {
        &self.name
    }
}

impl<S: Clone + 'static> ArgumentBuilder<S> for RequiredArgumentBuilder<S> {
    fn data(&self) -> &BuilderData<S> {
        &self.data
    }

    fn data_mut(&mut self) -> &mut BuilderData<S> {
        &mut self.data
    }

    fn build(&self) -> Arc<CommandNode<S>> {
        let d = &self.data;
        let result = CommandNode::new_argument(
            self.name.clone(),
            self.type_.clone(),
            d.command.clone(),
            d.requirement.clone(),
            d.target.clone(),
            d.modifier.clone(),
            d.forks,
            self.suggestions_provider.clone(),
        );
        for argument in d.arguments.get_children() {
            result.add_child(argument);
        }
        result
    }
}

/// Either builder kind, as returned by [`CommandNode::create_builder`].
pub enum NodeBuilder<S> {
    Literal(LiteralArgumentBuilder<S>),
    Argument(RequiredArgumentBuilder<S>),
}

impl<S: Clone + 'static> ArgumentBuilder<S> for NodeBuilder<S> {
    fn data(&self) -> &BuilderData<S> {
        match self {
            NodeBuilder::Literal(b) => b.data(),
            NodeBuilder::Argument(b) => b.data(),
        }
    }

    fn data_mut(&mut self) -> &mut BuilderData<S> {
        match self {
            NodeBuilder::Literal(b) => b.data_mut(),
            NodeBuilder::Argument(b) => b.data_mut(),
        }
    }

    fn build(&self) -> Arc<CommandNode<S>> {
        match self {
            NodeBuilder::Literal(b) => b.build(),
            NodeBuilder::Argument(b) => b.build(),
        }
    }
}

/// `LiteralArgumentBuilder.literal(name)`.
pub fn literal<S: Clone + 'static>(name: impl Into<String>) -> LiteralArgumentBuilder<S> {
    LiteralArgumentBuilder::literal(name)
}

/// `RequiredArgumentBuilder.argument(name, type)`.
pub fn argument<S: Clone + 'static>(
    name: impl Into<String>,
    type_: impl ArgumentType<S>,
) -> RequiredArgumentBuilder<S> {
    RequiredArgumentBuilder::argument(name, type_)
}
