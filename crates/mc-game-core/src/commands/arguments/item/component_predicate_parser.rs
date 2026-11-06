//! `ComponentPredicateParser`: the packrat grammar behind `ItemPredicateArgument`
//! (`type[cond,cond|cond,!cond]`, `type` = `id | #tag | *`; `cond` = `component | component=value | predicate~value`).
//!
//! Hand-unrolled port of `createGrammar` + `Grammar.parseForCommands/parseForSuggestions`: the rules call
//! the error collector in the same order as the vanilla terms, so the thrown exception (first
//! `CommandSyntaxException` among the entries at the furthest cursor) and the suggestions (all entries at that cursor)
//! are the same. `CachedParseState` memoization is omitted: it only suppresses repeated identical `store` calls.

use crate::commands::arguments::nbt_arguments::parse_tag_as_argument;
use crate::commands::arguments::{ERROR_INVALID_ID, read_identifier_non_empty};
use crate::commands::shared_suggestion_provider::{matches_sub_str, suggest_resource_into};
use mc_brigadier::{
    CommandSyntaxException, ImmutableStringReader, StringReader, Suggestions, SuggestionsBuilder,
};
use mc_core::ident::Identifier;
use mc_nbt::Tag;

/// `ComponentPredicateParser.Context<T, C, P>`.
pub trait Context {
    type T;
    type C;
    type P;

    fn for_element_type(
        &self,
        reader: &dyn ImmutableStringReader,
        id: &Identifier,
    ) -> Result<Self::T, CommandSyntaxException>;
    fn list_element_types(&self) -> Vec<Identifier>;
    fn for_tag_type(
        &self,
        reader: &dyn ImmutableStringReader,
        id: &Identifier,
    ) -> Result<Self::T, CommandSyntaxException>;
    fn list_tag_types(&self) -> Vec<Identifier>;
    fn lookup_component_type(
        &self,
        reader: &dyn ImmutableStringReader,
        id: &Identifier,
    ) -> Result<Self::C, CommandSyntaxException>;
    fn list_component_types(&self) -> Vec<Identifier>;
    fn create_component_test(
        &self,
        reader: &dyn ImmutableStringReader,
        component_type: &Self::C,
        value: &Tag,
    ) -> Result<Self::T, CommandSyntaxException>;
    fn create_component_presence_test(
        &self,
        reader: &dyn ImmutableStringReader,
        component_type: &Self::C,
    ) -> Self::T;
    fn lookup_predicate_type(
        &self,
        reader: &dyn ImmutableStringReader,
        id: &Identifier,
    ) -> Result<Self::P, CommandSyntaxException>;
    fn list_predicate_types(&self) -> Vec<Identifier>;
    fn create_predicate_test(
        &self,
        reader: &dyn ImmutableStringReader,
        predicate_type: &Self::P,
        value: &Tag,
    ) -> Result<Self::T, CommandSyntaxException>;
    fn negate(&self, value: Self::T) -> Self::T;
    fn any_of(&self, alternatives: Vec<Self::T>) -> Self::T;
}

/// What the entry's `SuggestionSupplier` yields.
#[derive(Clone, Copy)]
enum Suggest {
    /// `SuggestionSupplier.empty()`.
    Nothing,
    /// A terminal character.
    Char(char),
    Element,
    Tag,
    Component,
    Predicate,
}

enum Reason {
    /// `TerminalCharacters.error` = `literalIncorrect(char)`.
    Literal(char),
    /// `ResourceLookupRule.error` = `Identifier.ERROR_INVALID`.
    InvalidId,
    Exception(CommandSyntaxException),
}

/// `ErrorCollector.LongestOnly`.
struct Errors {
    last_cursor: i64,
    entries: Vec<(Suggest, Reason)>,
}

impl Errors {
    fn store(&mut self, cursor: usize, suggest: Suggest, reason: Reason) {
        self.discard_shorter(cursor as i64);
        if cursor as i64 == self.last_cursor {
            self.entries.push((suggest, reason));
        }
    }

    fn discard_shorter(&mut self, cursor: i64) {
        if cursor > self.last_cursor {
            self.last_cursor = cursor;
            self.entries.clear();
        }
    }
}

struct Parser<'a, X: Context> {
    ctx: &'a X,
    reader: StringReader,
    errors: Errors,
}

/// One kind of `ResourceLookupRule`.
#[derive(Clone, Copy)]
enum Lookup {
    Element,
    Tag,
    Component,
    Predicate,
}

impl<'a, X: Context> Parser<'a, X> {
    fn new_state(ctx: &'a X, input: &str, cursor: usize) -> Parser<'a, X> {
        let mut reader = StringReader::new(input);
        reader.set_cursor(cursor);
        Parser {
            ctx,
            reader,
            errors: Errors {
                last_cursor: -1,
                entries: Vec::new(),
            },
        }
    }

    fn mark(&self) -> usize {
        self.reader.get_cursor()
    }

    /// `StringReaderTerms.character(c)`.
    fn ch(&mut self, c: char) -> bool {
        self.reader.skip_whitespace();
        let cursor = self.mark();
        if self.reader.can_read() && self.reader.read() == c {
            return true;
        }
        self.errors
            .store(cursor, Suggest::Char(c), Reason::Literal(c));
        false
    }

    /// `rules.put(id, IdentifierParseRule.INSTANCE)`.
    fn id(&mut self) -> Option<Identifier> {
        self.reader.skip_whitespace();
        read_identifier_non_empty(&mut self.reader).ok()
    }

    /// `ResourceLookupRule.parse`.
    fn lookup<R>(
        &mut self,
        kind: Lookup,
        validate: impl FnOnce(&X, &StringReader, &Identifier) -> Result<R, CommandSyntaxException>,
    ) -> Option<R> {
        self.reader.skip_whitespace();
        let mark = self.mark();
        let suggest = match kind {
            Lookup::Element => Suggest::Element,
            Lookup::Tag => Suggest::Tag,
            Lookup::Component => Suggest::Component,
            Lookup::Predicate => Suggest::Predicate,
        };
        match self.id() {
            Some(id) => match validate(self.ctx, &self.reader, &id) {
                Ok(v) => Some(v),
                Err(e) => {
                    self.errors.store(mark, suggest, Reason::Exception(e));
                    None
                }
            },
            None => {
                self.errors.store(mark, suggest, Reason::InvalidId);
                None
            }
        }
    }

    /// `TagParseRule`.
    fn tag(&mut self) -> Option<Tag> {
        self.reader.skip_whitespace();
        let mark = self.mark();
        match parse_tag_as_argument(&mut self.reader) {
            Ok(tag) => Some(tag),
            Err(e) => {
                self.errors
                    .store(mark, Suggest::Nothing, Reason::Exception(e));
                None
            }
        }
    }

    /// `top`: `type '[' cut conditions? ']' | type`.
    fn top(&mut self) -> Option<Vec<X::T>> {
        let mark = self.mark();
        let mut cut = false;
        'alt1: {
            let Some(ty) = self.type_rule() else {
                break 'alt1;
            };
            if !self.ch('[') {
                break 'alt1;
            }
            cut = true;
            let m = self.mark();
            let conditions = match self.conditions() {
                Some(c) => c,
                None => {
                    self.reader.set_cursor(m);
                    Vec::new()
                }
            };
            if !self.ch(']') {
                break 'alt1;
            }
            return Some(ty.into_iter().chain(conditions).collect());
        }
        self.reader.set_cursor(mark);
        if cut {
            return None;
        }
        let ty = self.type_rule()?;
        Some(ty.into_iter().collect())
    }

    /// `type`: `element_type | '#' cut tag_type | any_type`; `Some(None)` is `*`.
    fn type_rule(&mut self) -> Option<Option<X::T>> {
        let mark = self.mark();
        if let Some(t) = self.lookup(Lookup::Element, |c, r, id| c.for_element_type(r, id)) {
            return Some(Some(t));
        }
        self.reader.set_cursor(mark);
        let mut cut = false;
        if self.ch('#') {
            cut = true;
            if let Some(t) = self.lookup(Lookup::Tag, |c, r, id| c.for_tag_type(r, id)) {
                return Some(Some(t));
            }
        }
        self.reader.set_cursor(mark);
        if cut {
            return None;
        }
        if self.ch('*') {
            return Some(None);
        }
        self.reader.set_cursor(mark);
        None
    }

    /// `conditions`: `alternatives (',' conditions)?`.
    fn conditions(&mut self) -> Option<Vec<X::T>> {
        let mark = self.mark();
        let Some(alternatives) = self.alternatives() else {
            self.reader.set_cursor(mark);
            return None;
        };
        let parsed = self.ctx.any_of(alternatives);
        let m = self.mark();
        let mut rest = None;
        if self.ch(',') {
            rest = self.conditions();
        }
        if rest.is_none() {
            self.reader.set_cursor(m);
        }
        let mut result = vec![parsed];
        result.extend(rest.unwrap_or_default());
        Some(result)
    }

    /// `alternatives`: `term ('|' alternatives)?`.
    fn alternatives(&mut self) -> Option<Vec<X::T>> {
        let mark = self.mark();
        let Some(alternative) = self.term() else {
            self.reader.set_cursor(mark);
            return None;
        };
        let m = self.mark();
        let mut rest = None;
        if self.ch('|') {
            rest = self.alternatives();
        }
        if rest.is_none() {
            self.reader.set_cursor(m);
        }
        let mut result = vec![alternative];
        result.extend(rest.unwrap_or_default());
        Some(result)
    }

    /// `term`: `test | '!' negation`.
    fn term(&mut self) -> Option<X::T> {
        let mark = self.mark();
        if let Some(t) = self.test() {
            return Some(t);
        }
        self.reader.set_cursor(mark);
        if self.ch('!') {
            if let Some(t) = self.test() {
                return Some(self.ctx.negate(t));
            }
        }
        self.reader.set_cursor(mark);
        None
    }

    /// `test`: `component '=' cut tag | predicate '~' cut tag | component`.
    fn test(&mut self) -> Option<X::T> {
        enum Matched<C, P> {
            Component(C, Option<Tag>),
            Predicate(P, Tag),
        }
        let mark = self.mark();
        let mut matched = None;
        let mut cut = false;
        // alternative 1
        'alt1: {
            let Some(component) =
                self.lookup(Lookup::Component, |c, r, id| c.lookup_component_type(r, id))
            else {
                break 'alt1;
            };
            if !self.ch('=') {
                break 'alt1;
            }
            cut = true;
            let Some(tag) = self.tag() else { break 'alt1 };
            matched = Some(Matched::Component(component, Some(tag)));
        }
        if matched.is_none() {
            self.reader.set_cursor(mark);
            if !cut {
                // alternative 2
                'alt2: {
                    let Some(predicate) =
                        self.lookup(Lookup::Predicate, |c, r, id| c.lookup_predicate_type(r, id))
                    else {
                        break 'alt2;
                    };
                    if !self.ch('~') {
                        break 'alt2;
                    }
                    cut = true;
                    let Some(tag) = self.tag() else { break 'alt2 };
                    matched = Some(Matched::Predicate(predicate, tag));
                }
                if matched.is_none() {
                    self.reader.set_cursor(mark);
                    if !cut {
                        // alternative 3
                        if let Some(component) = self
                            .lookup(Lookup::Component, |c, r, id| c.lookup_component_type(r, id))
                        {
                            matched = Some(Matched::Component(component, None));
                        } else {
                            self.reader.set_cursor(mark);
                        }
                    }
                }
            }
        }
        let matched = matched?;
        let result = match &matched {
            Matched::Predicate(p, value) => self.ctx.create_predicate_test(&self.reader, p, value),
            Matched::Component(c, Some(value)) => {
                self.ctx.create_component_test(&self.reader, c, value)
            }
            Matched::Component(c, None) => {
                Ok(self.ctx.create_component_presence_test(&self.reader, c))
            }
        };
        match result {
            Ok(t) => Some(t),
            Err(e) => {
                let cursor = self.mark();
                self.errors
                    .store(cursor, Suggest::Nothing, Reason::Exception(e));
                None
            }
        }
    }

    fn run_top(&mut self) -> Option<Vec<X::T>> {
        let result = self.top();
        if result.is_some() {
            let cursor = self.mark();
            self.errors.discard_shorter(cursor as i64);
        }
        result
    }
}

/// `Grammar.parseForCommands(reader)`.
pub fn parse_for_commands<X: Context>(
    ctx: &X,
    reader: &mut StringReader,
) -> Result<Vec<X::T>, CommandSyntaxException> {
    let mut parser = Parser::new_state(ctx, reader.get_string(), reader.get_cursor());
    let result = parser.run_top();
    reader.set_cursor(parser.mark());
    if let Some(r) = result {
        return Ok(r);
    }
    let cursor = parser.errors.last_cursor.max(0) as usize;
    for (_, reason) in parser.errors.entries {
        match reason {
            Reason::Exception(e) => return Err(e),
            Reason::Literal(c) => {
                let mut r = StringReader::new(reader.get_string());
                r.set_cursor(cursor);
                return Err(CommandSyntaxException::built_in_exceptions()
                    .literal_incorrect()
                    .create_with_context(&r, c.to_string()));
            }
            Reason::InvalidId => {
                let mut r = StringReader::new(reader.get_string());
                r.set_cursor(cursor);
                return Err(ERROR_INVALID_ID.create_with_context(&r));
            }
        }
    }
    unreachable!("Failed to parse: no error entries")
}

/// `Grammar.parseForSuggestions(builder)`.
pub fn parse_for_suggestions<X: Context>(ctx: &X, builder: SuggestionsBuilder) -> Suggestions {
    let mut parser = Parser::new_state(ctx, builder.get_input(), builder.get_start());
    parser.run_top();
    if parser.errors.entries.is_empty() {
        return builder.build();
    }
    let mut offset = builder.create_offset(parser.errors.last_cursor as usize);
    for (suggest, _) in &parser.errors.entries {
        match suggest {
            Suggest::Nothing => {}
            Suggest::Char(c) => {
                let lower_prefix = offset.get_remaining().to_lowercase();
                let name = c.to_string();
                if matches_sub_str(&lower_prefix, &name.to_lowercase()) {
                    offset.suggest(name);
                }
            }
            Suggest::Element => suggest_resource_into(ctx.list_element_types(), &mut offset),
            Suggest::Tag => suggest_resource_into(ctx.list_tag_types(), &mut offset),
            Suggest::Component => suggest_resource_into(ctx.list_component_types(), &mut offset),
            Suggest::Predicate => suggest_resource_into(ctx.list_predicate_types(), &mut offset),
        }
    }
    offset.build()
}
