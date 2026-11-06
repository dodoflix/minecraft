//! `com.mojang.brigadier.suggestion` (synchronous: vanilla's `CompletableFuture`s are always
//! already completed in brigadier itself).

use std::cmp::Ordering;
use std::fmt;
use std::sync::{Arc, LazyLock};

use crate::context::{CommandContext, StringRange};
use crate::exceptions::CommandSyntaxException;
use crate::message::MessageRef;
use crate::utf16::{
    compare_to, compare_to_ignore_case, java_hash, len16, substring, substring_from,
    substring_from_clamped,
};

/// `Suggestion` and `IntegerSuggestion` (`int_value` is `Some` for the latter).
#[derive(Clone, Debug)]
pub struct Suggestion {
    range: StringRange,
    text: String,
    tooltip: Option<MessageRef>,
    int_value: Option<i32>,
}

impl Suggestion {
    pub fn new(range: StringRange, text: impl Into<String>) -> Self {
        Self {
            range,
            text: text.into(),
            tooltip: None,
            int_value: None,
        }
    }

    pub fn with_tooltip(
        range: StringRange,
        text: impl Into<String>,
        tooltip: Option<MessageRef>,
    ) -> Self {
        Self {
            range,
            text: text.into(),
            tooltip,
            int_value: None,
        }
    }

    /// `new IntegerSuggestion(range, value, tooltip)`.
    pub fn integer(range: StringRange, value: i32, tooltip: Option<MessageRef>) -> Self {
        Self {
            range,
            text: value.to_string(),
            tooltip,
            int_value: Some(value),
        }
    }

    pub fn get_range(&self) -> StringRange {
        self.range
    }

    pub fn get_text(&self) -> &str {
        &self.text
    }

    pub fn get_tooltip(&self) -> Option<&MessageRef> {
        self.tooltip.as_ref()
    }

    /// `IntegerSuggestion.getValue()`.
    pub fn get_value(&self) -> Option<i32> {
        self.int_value
    }

    pub fn apply(&self, input: &str) -> String {
        let len = len16(input);
        if self.range.get_start() == 0 && self.range.get_end() == len {
            return self.text.clone();
        }
        let mut result = String::new();
        if self.range.get_start() > 0 {
            result.push_str(&substring(input, 0, self.range.get_start()));
        }
        result.push_str(&self.text);
        if self.range.get_end() < len {
            result.push_str(&substring_from(input, self.range.get_end()));
        }
        result
    }

    pub fn compare_to(&self, other: &Suggestion) -> Ordering {
        match (self.int_value, other.int_value) {
            (Some(a), Some(b)) => a.cmp(&b),
            _ => compare_to(&self.text, &other.text),
        }
    }

    pub fn compare_to_ignore_case(&self, other: &Suggestion) -> Ordering {
        if self.int_value.is_some() {
            self.compare_to(other)
        } else {
            compare_to_ignore_case(&self.text, &other.text)
        }
    }

    pub fn expand(&self, command: &str, range: StringRange) -> Suggestion {
        if range == self.range {
            return self.clone();
        }
        let mut result = String::new();
        if range.get_start() < self.range.get_start() {
            result.push_str(&substring(
                command,
                range.get_start(),
                self.range.get_start(),
            ));
        }
        result.push_str(&self.text);
        if range.get_end() > self.range.get_end() {
            result.push_str(&substring(command, self.range.get_end(), range.get_end()));
        }
        Suggestion {
            range,
            text: result,
            tooltip: self.tooltip.clone(),
            int_value: None,
        }
    }

    /// Java `hashCode()`; only used to reproduce `HashSet` iteration order (see [`java_hash_set_order`]).
    fn java_hash(&self) -> i32 {
        let range = 31i32
            .wrapping_mul(31i32.wrapping_add(self.range.get_start() as i32))
            .wrapping_add(self.range.get_end() as i32);
        let tooltip = self
            .tooltip
            .as_ref()
            .map_or(0, |t| java_hash(&t.get_string()));
        let mut h = 1i32;
        for part in [range, java_hash(&self.text), tooltip] {
            h = h.wrapping_mul(31).wrapping_add(part);
        }
        match self.int_value {
            Some(v) => 31i32.wrapping_mul(31i32.wrapping_add(h)).wrapping_add(v),
            None => h,
        }
    }
}

impl PartialEq for Suggestion {
    fn eq(&self, other: &Self) -> bool {
        self.range == other.range
            && self.text == other.text
            && self.int_value == other.int_value
            && self.tooltip.as_ref().map(|t| t.get_string())
                == other.tooltip.as_ref().map(|t| t.get_string())
    }
}

impl Eq for Suggestion {}

impl fmt::Display for Suggestion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let tooltip = self
            .tooltip
            .as_ref()
            .map_or("null".to_owned(), |t| t.get_string());
        match self.int_value {
            Some(v) => write!(
                f,
                "IntegerSuggestion{{value={v}, range={}, text='{}', tooltip='{tooltip}'}}",
                self.range, self.text
            ),
            None => write!(
                f,
                "Suggestion{{range={}, text='{}', tooltip='{tooltip}'}}",
                self.range, self.text
            ),
        }
    }
}

/// Collects `items` into a `java.util.HashSet` and returns them in its iteration order
/// (bucket index, then insertion order). Ties under the case-insensitive sort in
/// [`Suggestions::create`] are resolved by this order in vanilla, so it is reproduced here.
fn java_hash_set_order(items: impl IntoIterator<Item = Suggestion>) -> Vec<Suggestion> {
    let mut seen = std::collections::HashSet::new();
    let mut unique: Vec<Suggestion> = Vec::new();
    for item in items {
        if seen.insert(item.clone()) {
            unique.push(item);
        }
    }
    let mut capacity = 16usize;
    while unique.len() > capacity / 4 * 3 {
        capacity *= 2;
    }
    let bucket = |s: &Suggestion| {
        let h = s.java_hash() as u32;
        ((h ^ (h >> 16)) as usize) & (capacity - 1)
    };
    stable_sort_by(&mut unique, |a, b| bucket(a).cmp(&bucket(b)));
    unique
}

impl std::hash::Hash for Suggestion {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.java_hash().hash(state);
    }
}

/// Stable merge sort that tolerates inconsistent comparators (mixed integer/text suggestions)
/// without panicking, unlike `slice::sort_by`.
fn stable_sort_by<T>(items: &mut Vec<T>, mut cmp: impl FnMut(&T, &T) -> Ordering) {
    fn sort<T>(v: Vec<T>, cmp: &mut impl FnMut(&T, &T) -> Ordering) -> Vec<T> {
        if v.len() <= 1 {
            return v;
        }
        let mut left = v;
        let right = left.split_off(left.len() / 2);
        let (left, right) = (sort(left, cmp), sort(right, cmp));
        let mut out = Vec::with_capacity(left.len() + right.len());
        let (mut l, mut r) = (left.into_iter().peekable(), right.into_iter().peekable());
        while let (Some(a), Some(b)) = (l.peek(), r.peek()) {
            // `b` goes first only when strictly smaller, keeping the sort stable.
            if cmp(b, a) == Ordering::Less {
                out.extend(r.next())
            } else {
                out.extend(l.next())
            }
        }
        out.extend(l);
        out.extend(r);
        out
    }
    *items = sort(std::mem::take(items), &mut cmp);
}

/// `Suggestions`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Suggestions {
    range: StringRange,
    suggestions: Vec<Suggestion>,
}

static EMPTY: LazyLock<Suggestions> = LazyLock::new(|| Suggestions {
    range: StringRange::at(0),
    suggestions: Vec::new(),
});

impl Suggestions {
    pub fn new(range: StringRange, suggestions: Vec<Suggestion>) -> Self {
        Self { range, suggestions }
    }

    pub fn empty() -> Self {
        EMPTY.clone()
    }

    pub fn get_range(&self) -> StringRange {
        self.range
    }

    pub fn get_list(&self) -> &[Suggestion] {
        &self.suggestions
    }

    pub fn is_empty(&self) -> bool {
        self.suggestions.is_empty()
    }

    pub fn merge(command: &str, input: Vec<Suggestions>) -> Suggestions {
        match input.len() {
            0 => Self::empty(),
            1 => input.into_iter().next().unwrap_or_else(Self::empty),
            _ => Self::create(
                command,
                java_hash_set_order(input.into_iter().flat_map(|s| s.suggestions)),
            ),
        }
    }

    pub fn create(command: &str, suggestions: impl IntoIterator<Item = Suggestion>) -> Suggestions {
        let suggestions: Vec<Suggestion> = suggestions.into_iter().collect();
        if suggestions.is_empty() {
            return Self::empty();
        }
        let start = suggestions
            .iter()
            .map(|s| s.range.get_start())
            .min()
            .unwrap_or(0);
        let end = suggestions
            .iter()
            .map(|s| s.range.get_end())
            .max()
            .unwrap_or(0);
        let range = StringRange::between(start, end);
        let mut sorted = java_hash_set_order(suggestions.iter().map(|s| s.expand(command, range)));
        stable_sort_by(&mut sorted, |a, b| a.compare_to_ignore_case(b));
        Suggestions {
            range,
            suggestions: sorted,
        }
    }
}

impl fmt::Display for Suggestions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Suggestions{{range={}, suggestions=[", self.range)?;
        for (i, s) in self.suggestions.iter().enumerate() {
            if i > 0 {
                f.write_str(", ")?;
            }
            write!(f, "{s}")?;
        }
        f.write_str("]}")
    }
}

/// `SuggestionsBuilder`.
#[derive(Clone, Debug)]
pub struct SuggestionsBuilder {
    input: String,
    input_lower_case: String,
    start: usize,
    remaining: String,
    remaining_lower_case: String,
    result: Vec<Suggestion>,
}

impl SuggestionsBuilder {
    pub fn new(input: impl Into<String>, start: usize) -> Self {
        let input = input.into();
        let lower = input.to_lowercase();
        Self::with_lower_case(input, lower, start)
    }

    pub fn with_lower_case(input: String, input_lower_case: String, start: usize) -> Self {
        let remaining = substring_from(&input, start);
        let remaining_lower_case = substring_from_clamped(&input_lower_case, start);
        Self {
            input,
            input_lower_case,
            start,
            remaining,
            remaining_lower_case,
            result: Vec::new(),
        }
    }

    pub fn get_input(&self) -> &str {
        &self.input
    }

    pub fn get_start(&self) -> usize {
        self.start
    }

    pub fn get_remaining(&self) -> &str {
        &self.remaining
    }

    pub fn get_remaining_lower_case(&self) -> &str {
        &self.remaining_lower_case
    }

    pub fn build(&self) -> Suggestions {
        Suggestions::create(&self.input, self.result.iter().cloned())
    }

    fn full_range(&self) -> StringRange {
        StringRange::between(self.start, len16(&self.input))
    }

    pub fn suggest(&mut self, text: impl Into<String>) -> &mut Self {
        let text = text.into();
        if text != self.remaining {
            self.result.push(Suggestion::new(self.full_range(), text));
        }
        self
    }

    pub fn suggest_with_tooltip(
        &mut self,
        text: impl Into<String>,
        tooltip: Option<MessageRef>,
    ) -> &mut Self {
        let text = text.into();
        if text != self.remaining {
            self.result
                .push(Suggestion::with_tooltip(self.full_range(), text, tooltip));
        }
        self
    }

    pub fn suggest_int(&mut self, value: i32) -> &mut Self {
        self.result
            .push(Suggestion::integer(self.full_range(), value, None));
        self
    }

    pub fn suggest_int_with_tooltip(
        &mut self,
        value: i32,
        tooltip: Option<MessageRef>,
    ) -> &mut Self {
        self.result
            .push(Suggestion::integer(self.full_range(), value, tooltip));
        self
    }

    pub fn add(&mut self, other: &SuggestionsBuilder) -> &mut Self {
        self.result.extend(other.result.iter().cloned());
        self
    }

    pub fn create_offset(&self, start: usize) -> SuggestionsBuilder {
        Self::with_lower_case(self.input.clone(), self.input_lower_case.clone(), start)
    }

    pub fn restart(&self) -> SuggestionsBuilder {
        self.create_offset(self.start)
    }
}

/// `SuggestionProvider`.
pub type SuggestionProvider<S> = Arc<
    dyn Fn(&CommandContext<S>, SuggestionsBuilder) -> Result<Suggestions, CommandSyntaxException>
        + Send
        + Sync,
>;
