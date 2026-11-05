//! A port of `net.minecraft.util.parsing.packrat` specialised to SNBT (input = UTF-16 units,
//! values = [`Val`]). Kept structurally identical to vanilla (scope frames, cut/alternative
//! semantics, memo cache, longest-error collection) because the observable error message and
//! cursor depend on the exact order of `store` calls.

use crate::Tag;
use std::cell::Cell;
use std::collections::HashMap;

pub(crate) type Atom = usize;

/// Why a parse failed (`DelayedException`): lang key + optional `%s` argument, or a brigadier
/// `literalIncorrect` ("Expected literal x").
#[derive(Clone, Debug)]
pub(crate) enum Reason {
    Msg(&'static str, Option<String>),
    Literal(&'static str),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Sign {
    Plus,
    Minus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TypeSuffix {
    Float,
    Double,
    Byte,
    Short,
    Int,
    Long,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SignedPrefix {
    Signed,
    Unsigned,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct IntegerSuffix {
    pub signed: Option<SignedPrefix>,
    pub ty: Option<TypeSuffix>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Base {
    Binary,
    Decimal,
    Hex,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ArrayPrefix {
    Byte,
    Int,
    Long,
}

#[derive(Clone, Debug)]
pub(crate) struct IntegerLiteral {
    pub sign: Sign,
    pub base: Base,
    pub digits: String,
    pub suffix: IntegerSuffix,
}

/// A parsed value stored in a scope or returned by a rule.
#[derive(Clone, Debug)]
pub(crate) enum Val {
    Sign(Sign),
    IntSuffix(IntegerSuffix),
    TypeSuffix(TypeSuffix),
    ArrayPrefix(ArrayPrefix),
    Str(Vec<u16>),
    Signed(Sign, Vec<u16>),
    IntLit(IntegerLiteral),
    Tag(Tag),
    Entry(Vec<u16>, Tag),
    /// Result of a repeated term.
    List(Vec<Val>),
}

impl Val {
    /// Values holding tags are not memoised (cloning them per rule would be quadratic); a cache
    /// miss only re-runs the (deterministic, side-effect free) rule.
    fn is_heavy(&self) -> bool {
        match self {
            Val::Tag(_) | Val::Entry(..) => true,
            Val::List(v) => v.first().is_some_and(Val::is_heavy),
            _ => false,
        }
    }
}

// ---------------------------------------------------------------- Scope

#[derive(Clone, Copy, PartialEq, Eq)]
enum Key {
    /// Frame marker; payload = index of the previous marker.
    Frame(usize),
    Atom(Atom),
}

pub(crate) struct Scope {
    stack: Vec<(Key, Option<Val>)>,
    top_entry: usize,
    top_marker: usize,
}

impl Scope {
    fn new() -> Self {
        Scope { stack: vec![(Key::Frame(0), None)], top_entry: 0, top_marker: 0 }
    }

    fn set(&mut self, idx: usize, entry: (Key, Option<Val>)) {
        if idx == self.stack.len() {
            self.stack.push(entry);
        } else {
            self.stack[idx] = entry;
        }
    }

    fn value_index(&self, atom: Atom) -> Option<usize> {
        (self.top_marker + 1..=self.top_entry).rev().find(|&i| self.stack[i].0 == Key::Atom(atom))
    }

    fn value_index_for_any(&self, atoms: &[Atom]) -> Option<usize> {
        (self.top_marker + 1..=self.top_entry).rev().find(|&i| matches!(self.stack[i].0, Key::Atom(a) if atoms.contains(&a)))
    }

    fn setup_new_frame(&mut self) {
        self.top_entry += 1;
        let prev = self.top_marker;
        self.set(self.top_entry, (Key::Frame(prev), None));
        self.top_marker = self.top_entry;
    }

    pub fn push_frame(&mut self) {
        self.setup_new_frame();
    }

    pub fn pop_frame(&mut self) {
        let Key::Frame(prev) = self.stack[self.top_marker].0 else { unreachable!("marker slot") };
        self.top_entry = self.top_marker - 1;
        self.top_marker = prev;
    }

    pub fn split_frame(&mut self) {
        let current_marker = self.top_marker;
        let n = self.top_entry - self.top_marker;
        self.setup_new_frame();
        let mut target = self.top_entry;
        for source in (current_marker + 1..).take(n) {
            target += 1;
            let key = self.stack[source].0;
            self.set(target, (key, None));
        }
        self.top_entry = target;
    }

    pub fn clear_frame_values(&mut self) {
        for i in self.top_marker + 1..=self.top_entry {
            self.stack[i].1 = None;
        }
    }

    pub fn merge_frame(&mut self) {
        let Key::Frame(previous_marker) = self.stack[self.top_marker].0 else { unreachable!("marker slot") };
        let mut previous_cursor = previous_marker;
        let mut current_cursor = self.top_marker;
        while current_cursor < self.top_entry {
            previous_cursor += 1;
            current_cursor += 1;
            let new_key = self.stack[current_cursor].0;
            let new_value = self.stack[current_cursor].1.take();
            let old_key = self.stack[previous_cursor].0;
            if old_key != new_key {
                self.stack[previous_cursor] = (new_key, new_value);
            } else if new_value.is_some() {
                self.stack[previous_cursor].1 = new_value;
            }
        }
        self.top_entry = previous_cursor;
        self.top_marker = previous_marker;
    }

    pub fn put(&mut self, atom: Atom, value: Val) {
        if let Some(i) = self.value_index(atom) {
            self.stack[i].1 = Some(value);
        } else {
            self.top_entry += 1;
            self.set(self.top_entry, (Key::Atom(atom), Some(value)));
        }
    }

    /// `scope.get(atom)`: null when absent or cleared.
    pub fn get(&self, atom: Atom) -> Option<&Val> {
        self.value_index(atom).and_then(|i| self.stack[i].1.as_ref())
    }

    pub fn get_any(&self, atoms: &[Atom]) -> Option<&Val> {
        self.value_index_for_any(atoms).and_then(|i| self.stack[i].1.as_ref())
    }
}

// --------------------------------------------------------------- errors

#[derive(Default)]
struct ErrorCollector {
    /// Furthest cursor at which an error was stored (-1 initially).
    last_cursor: i64,
    /// First error stored at `last_cursor`; vanilla throws the first exception of the list.
    first: Option<Reason>,
    /// Number of errors stored at `last_cursor` (vanilla's entry list size).
    entries: usize,
    /// What the failed terminals at `last_cursor` would have accepted (`SuggestionSupplier.possibleValues`).
    suggestions: Vec<String>,
}

impl ErrorCollector {
    fn new() -> Self {
        ErrorCollector { last_cursor: -1, first: None, entries: 0, suggestions: Vec::new() }
    }

    /// `discardErrorsFromShorterParse`.
    fn discard_shorter(&mut self, cursor: i64) {
        if cursor > self.last_cursor {
            self.last_cursor = cursor;
            self.first = None;
            self.entries = 0;
            self.suggestions.clear();
        }
    }

    /// `finish(finalCursor)`, called when the top rule succeeded.
    fn finish(&mut self, cursor: usize) {
        self.discard_shorter(cursor as i64);
    }

    fn store(&mut self, cursor: usize, reason: Reason) {
        let cursor = cursor as i64;
        self.discard_shorter(cursor);
        if cursor == self.last_cursor {
            self.entries += 1;
            // `TerminalCharacters`: one suggestion per accepted character (the error text joins them with `|`)
            if let Reason::Literal(text) = &reason {
                self.suggestions.extend(text.split('|').filter(|c| !c.is_empty()).map(str::to_owned));
            }
            if self.first.is_none() {
                self.first = Some(reason);
            }
        }
    }
}

// ---------------------------------------------------------------- Terms

pub(crate) enum CharSet {
    One(u16),
    Two(u16, u16),
    /// `SnbtGrammar.canStartNumber`.
    NumberStart,
}

impl CharSet {
    fn accepts(&self, c: u16) -> bool {
        match *self {
            CharSet::One(a) => a == c,
            CharSet::Two(a, b) => a == c || b == c,
            CharSet::NumberStart => matches!(c, 0x2b | 0x2d | 0x2e | 0x30..=0x39),
        }
    }
}

pub(crate) enum Term {
    Marker(Atom, Val),
    Sequence(Vec<Term>),
    Alternative(Vec<Term>),
    Maybe(Box<Term>),
    Repeated { element: Atom, list: Atom, min: usize },
    RepeatedWithSeparator { element: Atom, list: Atom, separator: Box<Term>, min: usize, trailing: bool },
    LookAhead(Box<Term>, bool),
    Cut,
    Fail(Reason),
    /// `Dictionary.Reference`: parse `rule`, store the result under `store`.
    Reference { rule: Atom, store: Atom },
    /// `StringReaderTerms.TerminalCharacters` with its `literalIncorrect` error text.
    Chars(CharSet, &'static str),
}

pub(crate) type Action = fn(&mut Ps<'_>) -> Option<Val>;

pub(crate) enum Rule {
    Term { term: Term, action: Action },
    /// `NumberRunParseRule`.
    NumberRun { accept: fn(u16) -> bool, no_value: Reason, underscore: Reason },
    /// `GreedyPredicateParseRule` (and `GreedyPatternParseRule` for plain character classes).
    Greedy { accept: fn(u16) -> bool, min: usize, max: usize, error: Reason },
    /// `UnquotedStringParseRule` with min size 1.
    Unquoted { error: Reason },
}

pub(crate) struct Grammar {
    pub rules: Vec<Option<Rule>>,
    pub top: Atom,
}

enum CacheEntry {
    Negative,
    Positive(Val, usize),
}

/// `CachedParseState<StringReader>`: input, cursor, scope, error collector and memo cache.
pub(crate) struct Ps<'g> {
    grammar: &'g Grammar,
    input: &'g [u16],
    pub cursor: usize,
    pub scope: Scope,
    err: ErrorCollector,
    cache: HashMap<(usize, Atom), CacheEntry>,
}

/// `Character.isWhitespace(char)`.
pub(crate) fn is_whitespace(c: u16) -> bool {
    matches!(c, 0x09..=0x0D | 0x1C..=0x20 | 0x1680 | 0x2000..=0x2006 | 0x2008..=0x200A | 0x2028 | 0x2029 | 0x205F | 0x3000)
}

/// `StringReader.isAllowedInUnquotedString`.
pub(crate) fn is_allowed_in_unquoted_string(c: u16) -> bool {
    matches!(c, 0x30..=0x39 | 0x41..=0x5a | 0x61..=0x7a | 0x5f | 0x2d | 0x2e | 0x2b)
}

impl<'g> Ps<'g> {
    pub fn new(grammar: &'g Grammar, input: &'g [u16], cursor: usize) -> Self {
        Ps { grammar, input, cursor, scope: Scope::new(), err: ErrorCollector::new(), cache: HashMap::new() }
    }

    pub fn store(&mut self, cursor: usize, reason: Reason) {
        self.err.store(cursor, reason);
    }

    /// Stores `reason` at the current cursor (`errorCollector().store(state.mark(), reason)`).
    pub fn store_here(&mut self, reason: Reason) {
        self.err.store(self.cursor, reason);
    }

    pub fn skip_whitespace(&mut self) {
        while self.cursor < self.input.len() && is_whitespace(self.input[self.cursor]) {
            self.cursor += 1;
        }
    }

    pub fn failure(&self) -> Option<(usize, Reason)> {
        self.err.first.clone().map(|r| (self.err.last_cursor as usize, r))
    }

    /// `Grammar.parse` → `parseTopRule`.
    pub fn parse_top(&mut self) -> Option<Val> {
        let result = self.parse(self.grammar.top);
        if result.is_some() {
            self.err.finish(self.cursor);
        }
        result
    }

    /// `Grammar.parseForSuggestions` after the parse: the cursor of the longest failure and what its terminals would
    /// have accepted; `None` when no error was recorded there.
    pub fn suggestions(&self) -> Option<(usize, &[String])> {
        (self.err.entries > 0).then(|| (self.err.last_cursor as usize, self.err.suggestions.as_slice()))
    }

    /// `CachedParseState.parse(rule)`.
    pub fn parse(&mut self, rule: Atom) -> Option<Val> {
        let mark = self.cursor;
        match self.cache.get(&(mark, rule)) {
            Some(CacheEntry::Negative) => return None,
            Some(CacheEntry::Positive(v, after)) => {
                let (v, after) = (v.clone(), *after);
                self.cursor = after;
                return Some(v);
            }
            None => {}
        }
        let result = self.run_rule(rule);
        match &result {
            None => {
                self.cache.insert((mark, rule), CacheEntry::Negative);
            }
            Some(v) if !v.is_heavy() => {
                self.cache.insert((mark, rule), CacheEntry::Positive(v.clone(), self.cursor));
            }
            Some(_) => {}
        }
        result
    }

    fn run_rule(&mut self, atom: Atom) -> Option<Val> {
        let g = self.grammar;
        match g.rules[atom].as_ref().expect("unbound rule") {
            Rule::Term { term, action } => {
                self.scope.push_frame();
                let result = if term.parse(self, None, false) { action(self) } else { None };
                self.scope.pop_frame();
                result
            }
            Rule::NumberRun { accept, no_value, underscore } => {
                self.skip_whitespace();
                let start = self.cursor;
                let mut pos = start;
                while pos < self.input.len() && accept(self.input[pos]) {
                    pos += 1;
                }
                if pos == start {
                    self.store_here(no_value.clone());
                    None
                } else if self.input[start] != 0x5f && self.input[pos - 1] != 0x5f {
                    self.cursor = pos;
                    Some(Val::Str(self.input[start..pos].to_vec()))
                } else {
                    self.store_here(underscore.clone());
                    None
                }
            }
            Rule::Greedy { accept, min, max, error } => {
                let start = self.cursor;
                let mut pos = start;
                while pos < self.input.len() && accept(self.input[pos]) && pos - start < *max {
                    pos += 1;
                }
                if pos - start < *min {
                    self.store_here(error.clone());
                    None
                } else {
                    self.cursor = pos;
                    Some(Val::Str(self.input[start..pos].to_vec()))
                }
            }
            Rule::Unquoted { error } => {
                self.skip_whitespace();
                let cursor = self.cursor;
                let mut pos = cursor;
                while pos < self.input.len() && is_allowed_in_unquoted_string(self.input[pos]) {
                    pos += 1;
                }
                self.cursor = pos;
                if pos - cursor < 1 {
                    self.store(cursor, error.clone());
                    None
                } else {
                    Some(Val::Str(self.input[cursor..pos].to_vec()))
                }
            }
        }
    }
}

impl Term {
    /// `Term.parse(state, scope, control)`. `silent` mirrors `state.silent()` (lookahead):
    /// terminals then do not record errors.
    pub fn parse(&self, ps: &mut Ps<'_>, control: Option<&Cell<bool>>, silent: bool) -> bool {
        match self {
            Term::Marker(atom, value) => {
                ps.scope.put(*atom, value.clone());
                true
            }
            Term::Sequence(elements) => {
                let mark = ps.cursor;
                for e in elements {
                    if !e.parse(ps, control, silent) {
                        ps.cursor = mark;
                        return false;
                    }
                }
                true
            }
            Term::Alternative(elements) => {
                let cut = Cell::new(false);
                let mark = ps.cursor;
                ps.scope.split_frame();
                for e in elements {
                    if e.parse(ps, Some(&cut), silent) {
                        ps.scope.merge_frame();
                        return true;
                    }
                    ps.scope.clear_frame_values();
                    ps.cursor = mark;
                    if cut.get() {
                        break;
                    }
                }
                ps.scope.pop_frame();
                false
            }
            Term::Maybe(term) => {
                let mark = ps.cursor;
                if !term.parse(ps, control, silent) {
                    ps.cursor = mark;
                }
                true
            }
            Term::Repeated { element, list, min } => {
                let mark = ps.cursor;
                let mut elements = Vec::new();
                loop {
                    let entry_mark = ps.cursor;
                    match ps.parse(*element) {
                        None => {
                            ps.cursor = entry_mark;
                            if elements.len() < *min {
                                ps.cursor = mark;
                                return false;
                            }
                            ps.scope.put(*list, Val::List(elements));
                            return true;
                        }
                        Some(v) => elements.push(v),
                    }
                }
            }
            Term::RepeatedWithSeparator { element, list, separator, min, trailing } => {
                let list_mark = ps.cursor;
                let mut elements = Vec::new();
                let mut first = true;
                loop {
                    let mark_before_separator = ps.cursor;
                    if !first && !separator.parse(ps, control, silent) {
                        ps.cursor = mark_before_separator;
                        break;
                    }
                    let mark_after_separator = ps.cursor;
                    match ps.parse(*element) {
                        None => {
                            if first {
                                ps.cursor = mark_after_separator;
                            } else {
                                if !*trailing {
                                    ps.cursor = list_mark;
                                    return false;
                                }
                                ps.cursor = mark_after_separator;
                            }
                            break;
                        }
                        Some(v) => {
                            elements.push(v);
                            first = false;
                        }
                    }
                }
                if elements.len() < *min {
                    ps.cursor = list_mark;
                    return false;
                }
                ps.scope.put(*list, Val::List(elements));
                true
            }
            Term::LookAhead(term, positive) => {
                let mark = ps.cursor;
                let result = term.parse(ps, control, true);
                ps.cursor = mark;
                *positive == result
            }
            Term::Cut => {
                if let Some(c) = control {
                    c.set(true);
                }
                true
            }
            Term::Fail(reason) => {
                if !silent {
                    ps.store_here(reason.clone());
                }
                false
            }
            Term::Reference { rule, store } => match ps.parse(*rule) {
                None => false,
                Some(v) => {
                    ps.scope.put(*store, v);
                    true
                }
            },
            Term::Chars(set, text) => {
                ps.skip_whitespace();
                let cursor = ps.cursor;
                if cursor < ps.input.len() {
                    let c = ps.input[cursor];
                    ps.cursor += 1;
                    if set.accepts(c) {
                        return true;
                    }
                }
                if !silent {
                    ps.store(cursor, Reason::Literal(text));
                }
                false
            }
        }
    }
}

