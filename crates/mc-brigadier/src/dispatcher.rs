//! `CommandDispatcher` and `ParseResults`.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

use crate::builder::{ArgumentBuilder, LiteralArgumentBuilder};
use crate::command::{AmbiguityConsumer, ResultConsumer};
use crate::context::{CommandContext, CommandContextBuilder, ContextChain};
use crate::exceptions::{CommandSyntaxException, built_in_exceptions};
use crate::string_reader::{ImmutableStringReader, StringReader};
use crate::suggestion::{Suggestions, SuggestionsBuilder};
use crate::tree::CommandNode;
use crate::utf16::{len16, substring};

pub const ARGUMENT_SEPARATOR: &str = " ";
pub const ARGUMENT_SEPARATOR_CHAR: char = ' ';

/// `ParseResults`. `exceptions` is vanilla's `Map<CommandNode, CommandSyntaxException>` in insertion order.
pub struct ParseResults<S> {
    context: CommandContextBuilder<S>,
    reader: StringReader,
    exceptions: Vec<(Arc<CommandNode<S>>, CommandSyntaxException)>,
}

impl<S> ParseResults<S> {
    pub fn new(
        context: CommandContextBuilder<S>,
        reader: StringReader,
        exceptions: Vec<(Arc<CommandNode<S>>, CommandSyntaxException)>,
    ) -> Self {
        Self {
            context,
            reader,
            exceptions,
        }
    }

    pub fn get_context(&self) -> &CommandContextBuilder<S> {
        &self.context
    }

    pub fn get_reader(&self) -> &StringReader {
        &self.reader
    }

    pub fn get_exceptions(&self) -> &[(Arc<CommandNode<S>>, CommandSyntaxException)] {
        &self.exceptions
    }
}

/// `CommandDispatcher<S>`.
pub struct CommandDispatcher<S> {
    root: Arc<CommandNode<S>>,
    consumer: ResultConsumer<S>,
}

impl<S> Drop for CommandDispatcher<S> {
    fn drop(&mut self) {
        self.root.dismantle(&mut std::collections::HashSet::new());
    }
}

impl<S: Clone + 'static> Default for CommandDispatcher<S> {
    fn default() -> Self {
        Self::new()
    }
}

impl<S: Clone + 'static> CommandDispatcher<S> {
    pub fn new() -> Self {
        Self::with_root(CommandNode::new_root())
    }

    pub fn with_root(root: Arc<CommandNode<S>>) -> Self {
        Self {
            root,
            consumer: Arc::new(|_, _, _| {}),
        }
    }

    pub fn register(&self, command: LiteralArgumentBuilder<S>) -> Arc<CommandNode<S>> {
        let build = command.build();
        self.root.add_child(build.clone());
        build
    }

    pub fn set_consumer(
        &mut self,
        consumer: impl Fn(&CommandContext<S>, bool, i32) + Send + Sync + 'static,
    ) {
        self.consumer = Arc::new(consumer);
    }

    pub fn get_root(&self) -> &Arc<CommandNode<S>> {
        &self.root
    }

    pub fn execute(&self, input: &str, source: S) -> Result<i32, CommandSyntaxException> {
        self.execute_reader(StringReader::new(input), source)
    }

    pub fn execute_reader(
        &self,
        input: StringReader,
        source: S,
    ) -> Result<i32, CommandSyntaxException> {
        let parse = self.parse_reader(input, source);
        self.execute_parsed(&parse)
    }

    pub fn execute_parsed(&self, parse: &ParseResults<S>) -> Result<i32, CommandSyntaxException> {
        if parse.reader.can_read() {
            if parse.exceptions.len() == 1 {
                return Err(parse.exceptions[0].1.clone());
            } else if parse.context.get_range().is_empty() {
                return Err(built_in_exceptions()
                    .dispatcher_unknown_command()
                    .create_with_context(&parse.reader));
            } else {
                return Err(built_in_exceptions()
                    .dispatcher_unknown_argument()
                    .create_with_context(&parse.reader));
            }
        }
        let command = parse.reader.get_string();
        let original = parse.context.build(command);
        match ContextChain::try_flatten(&original) {
            None => {
                (self.consumer)(&original, false, 0);
                Err(built_in_exceptions()
                    .dispatcher_unknown_command()
                    .create_with_context(&parse.reader))
            }
            Some(chain) => chain.execute_all(original.get_source().clone(), &self.consumer),
        }
    }

    pub fn parse(&self, command: &str, source: S) -> ParseResults<S> {
        self.parse_reader(StringReader::new(command), source)
    }

    pub fn parse_reader(&self, command: StringReader, source: S) -> ParseResults<S> {
        let context = CommandContextBuilder::new(source, self.root.clone(), command.get_cursor());
        self.parse_nodes(&self.root, &command, context)
    }

    fn parse_nodes(
        &self,
        node: &Arc<CommandNode<S>>,
        original_reader: &StringReader,
        context_so_far: CommandContextBuilder<S>,
    ) -> ParseResults<S> {
        let mut errors: Vec<(Arc<CommandNode<S>>, CommandSyntaxException)> = Vec::new();
        let mut potentials: Vec<ParseResults<S>> = Vec::new();
        let _cursor = original_reader.get_cursor();

        for child in node.get_relevant_nodes(original_reader) {
            if !child.can_use(context_so_far.get_source()) {
                continue;
            }
            let mut context = context_so_far.copy();
            let mut reader = original_reader.clone();

            let parsed =
                match catch_unwind(AssertUnwindSafe(|| child.parse(&mut reader, &mut context))) {
                    Ok(result) => result,
                    // Vanilla wraps any RuntimeException thrown by an argument type.
                    Err(payload) => {
                        let message = payload
                            .downcast_ref::<String>()
                            .cloned()
                            .or_else(|| payload.downcast_ref::<&str>().map(|s| (*s).to_owned()))
                            .unwrap_or_else(|| "null".to_owned());
                        Err(built_in_exceptions()
                            .dispatcher_parse_exception()
                            .create_with_context(&reader, message))
                    }
                };
            let parsed = parsed.and_then(|()| {
                if reader.can_read() && reader.peek() != ' ' {
                    Err(built_in_exceptions()
                        .dispatcher_expected_argument_separator()
                        .create_with_context(&reader))
                } else {
                    Ok(())
                }
            });
            if let Err(e) = parsed {
                errors.push((child, e));
                continue;
            }

            context.with_command(child.get_command());
            if reader.can_read_n(if child.get_redirect().is_none() { 2 } else { 1 }) {
                reader.skip();
                if let Some(redirect) = child.get_redirect() {
                    let child_context = CommandContextBuilder::new(
                        context.get_source().clone(),
                        redirect.clone(),
                        reader.get_cursor(),
                    );
                    let parse = self.parse_nodes(redirect, &reader, child_context);
                    context.with_child(parse.context);
                    return ParseResults::new(context, parse.reader, parse.exceptions);
                }
                let parse = self.parse_nodes(&child, &reader, context);
                potentials.push(parse);
            } else {
                potentials.push(ParseResults::new(context, reader, Vec::new()));
            }
        }

        if potentials.is_empty() {
            return ParseResults::new(context_so_far, original_reader.clone(), errors);
        }
        // Stable sort by (cannot read more first, no exceptions first) == the first minimal element.
        let best = (0..potentials.len())
            .min_by_key(|&i| {
                (
                    potentials[i].reader.can_read(),
                    !potentials[i].exceptions.is_empty(),
                )
            })
            .unwrap_or(0);
        potentials.swap_remove(best)
    }

    pub fn get_all_usage(
        &self,
        node: &Arc<CommandNode<S>>,
        source: &S,
        restricted: bool,
    ) -> Vec<String> {
        let mut result = Vec::new();
        self.collect_all_usage(node, source, &mut result, "", restricted);
        result
    }

    fn redirect_usage(&self, node: &CommandNode<S>) -> Option<String> {
        node.get_redirect().map(|r| {
            if Arc::ptr_eq(r, &self.root) {
                "...".to_owned()
            } else {
                format!("-> {}", r.get_usage_text())
            }
        })
    }

    fn collect_all_usage(
        &self,
        node: &Arc<CommandNode<S>>,
        source: &S,
        result: &mut Vec<String>,
        prefix: &str,
        restricted: bool,
    ) {
        if restricted && !node.can_use(source) {
            return;
        }
        if node.get_command().is_some() {
            result.push(prefix.to_owned());
        }
        if let Some(redirect) = self.redirect_usage(node) {
            result.push(if prefix.is_empty() {
                format!("{} {redirect}", node.get_usage_text())
            } else {
                format!("{prefix} {redirect}")
            });
        } else {
            for child in node.get_children() {
                let usage = child.get_usage_text();
                let next = if prefix.is_empty() {
                    usage
                } else {
                    format!("{prefix} {usage}")
                };
                self.collect_all_usage(&child, source, result, &next, restricted);
            }
        }
    }

    /// `getSmartUsage`: `child -> usage` for each usable child of `node`.
    pub fn get_smart_usage(
        &self,
        node: &Arc<CommandNode<S>>,
        source: &S,
    ) -> Vec<(Arc<CommandNode<S>>, String)> {
        let optional = node.get_command().is_some();
        node.get_children()
            .into_iter()
            .filter_map(|child| {
                self.smart_usage(&child, source, optional, false)
                    .map(|u| (child, u))
            })
            .collect()
    }

    fn smart_usage(
        &self,
        node: &Arc<CommandNode<S>>,
        source: &S,
        optional: bool,
        deep: bool,
    ) -> Option<String> {
        if !node.can_use(source) {
            return None;
        }
        let self_text = if optional {
            format!("[{}]", node.get_usage_text())
        } else {
            node.get_usage_text()
        };
        let child_optional = node.get_command().is_some();
        let (open, close) = if child_optional {
            ("[", "]")
        } else {
            ("(", ")")
        };
        if !deep {
            if let Some(redirect) = self.redirect_usage(node) {
                return Some(format!("{self_text} {redirect}"));
            }
            let children: Vec<_> = node
                .get_children()
                .into_iter()
                .filter(|c| c.can_use(source))
                .collect();
            if children.len() == 1 {
                if let Some(usage) =
                    self.smart_usage(&children[0], source, child_optional, child_optional)
                {
                    return Some(format!("{self_text} {usage}"));
                }
            } else if children.len() > 1 {
                let mut child_usage: Vec<String> = Vec::new();
                for child in &children {
                    if let Some(usage) = self.smart_usage(child, source, child_optional, true)
                        && !child_usage.contains(&usage)
                    {
                        child_usage.push(usage);
                    }
                }
                if child_usage.len() == 1 {
                    let usage = &child_usage[0];
                    return Some(format!(
                        "{self_text} {}",
                        if child_optional {
                            format!("[{usage}]")
                        } else {
                            usage.clone()
                        }
                    ));
                }
                if child_usage.len() > 1 {
                    let names: Vec<String> = children.iter().map(|c| c.get_usage_text()).collect();
                    return Some(format!("{self_text} {open}{}{close}", names.join("|")));
                }
            }
        }
        Some(self_text)
    }

    pub fn get_completion_suggestions(&self, parse: &ParseResults<S>) -> Suggestions {
        self.get_completion_suggestions_at(parse, parse.reader.get_total_length())
    }

    pub fn get_completion_suggestions_at(
        &self,
        parse: &ParseResults<S>,
        cursor: usize,
    ) -> Suggestions {
        let context = &parse.context;
        let node_before_cursor = context.find_suggestion_context(cursor);
        let parent = &node_before_cursor.parent;
        let start = node_before_cursor.start_pos.min(cursor);
        let full_input = parse.reader.get_string();
        let truncated_input = substring(full_input, 0, cursor.min(len16(full_input)));
        let truncated_lower = truncated_input.to_lowercase();
        let built = node_before_cursor.context.build(&truncated_input);

        let suggestions = parent
            .get_children()
            .iter()
            .map(|node| {
                let builder = SuggestionsBuilder::with_lower_case(
                    truncated_input.clone(),
                    truncated_lower.clone(),
                    start,
                );
                node.list_suggestions(&built, builder)
                    .unwrap_or_else(|_| Suggestions::empty())
            })
            .collect();
        Suggestions::merge(full_input, suggestions)
    }

    /// `getPath`: names from the root to `target`, empty if `target` is not in the tree.
    pub fn get_path(&self, target: &Arc<CommandNode<S>>) -> Vec<String> {
        fn walk<S: 'static>(
            node: &Arc<CommandNode<S>>,
            target: &Arc<CommandNode<S>>,
            path: &mut Vec<String>,
        ) -> bool {
            if Arc::ptr_eq(node, target) {
                return true;
            }
            for child in node.get_children() {
                path.push(child.get_name().to_owned());
                if walk(&child, target, path) {
                    return true;
                }
                path.pop();
            }
            false
        }
        let mut path = Vec::new();
        if walk(&self.root, target, &mut path) {
            path
        } else {
            Vec::new()
        }
    }

    pub fn find_node<'a>(
        &self,
        path: impl IntoIterator<Item = &'a str>,
    ) -> Option<Arc<CommandNode<S>>> {
        let mut node = self.root.clone();
        for name in path {
            node = node.get_child(name)?;
        }
        Some(node)
    }

    pub fn find_ambiguities(&self, consumer: AmbiguityConsumer<'_, S>) {
        self.root.find_ambiguities(consumer);
    }
}
