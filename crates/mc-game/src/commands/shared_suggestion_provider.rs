//! `net.minecraft.commands.SharedSuggestionProvider`: the static suggestion helpers (the instance side is implemented
//! by `CommandSourceStack`).

use mc_brigadier::{MessageRef, Suggestions, SuggestionsBuilder};
use mc_core::ident::Identifier;

/// `SharedSuggestionProvider.ElementSuggestionType`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ElementSuggestionType {
    Tags,
    Elements,
    All,
}

impl ElementSuggestionType {
    pub fn should_suggest_tags(self) -> bool {
        matches!(self, Self::Tags | Self::All)
    }

    pub fn should_suggest_elements(self) -> bool {
        matches!(self, Self::Elements | Self::All)
    }
}

/// `SharedSuggestionProvider.TextCoordinates`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct TextCoordinates {
    pub x: String,
    pub y: String,
    pub z: String,
}

impl TextCoordinates {
    pub fn new(x: impl Into<String>, y: impl Into<String>, z: impl Into<String>) -> Self {
        Self { x: x.into(), y: y.into(), z: z.into() }
    }

    pub fn default_local() -> Self {
        Self::new("^", "^", "^")
    }

    pub fn default_global() -> Self {
        Self::new("~", "~", "~")
    }
}

/// `MATCH_SPLITTER.indexIn(input, from)` (`CharMatcher.anyOf("._/")`), in UTF-16 units like Java (the inputs are
/// ASCII identifiers in practice; a char index is used for non-ASCII).
fn index_of_splitter(input: &[char], from: usize) -> Option<usize> {
    (from..input.len()).find(|&i| matches!(input[i], '.' | '_' | '/'))
}

/// `SharedSuggestionProvider.matchesSubStr`.
pub fn matches_sub_str(pattern: &str, input: &str) -> bool {
    let input: Vec<char> = input.chars().collect();
    let pattern: Vec<char> = pattern.chars().collect();
    let mut index = 0;
    while !(index <= input.len() && input[index..].starts_with(&pattern)) {
        match index_of_splitter(&input, index) {
            None => return false,
            Some(i) => index = i + 1,
        }
    }
    true
}

/// `filterResources(values, contents, converter, consumer)`.
pub fn filter_resources<T>(values: impl IntoIterator<Item = T>, contents: &str, converter: impl Fn(&T) -> Identifier, mut consumer: impl FnMut(T)) {
    let has_namespace = contents.contains(':');
    for value in values {
        let id = converter(&value);
        if has_namespace {
            if matches_sub_str(contents, &id.to_string()) {
                consumer(value);
            }
        } else if matches_sub_str(contents, id.namespace()) || matches_sub_str(contents, id.path()) {
            consumer(value);
        }
    }
}

/// Guava `Strings.commonPrefix`.
fn common_prefix<'a>(a: &'a str, b: &str) -> &'a str {
    let end = a.char_indices().zip(b.chars()).find(|((_, x), y)| x != y).map_or_else(|| a.len().min(b.len()), |((i, _), _)| i);
    // `end` is a char boundary of `a` by construction (or the length of the shorter string, which is one of a's boundaries when a is shorter)
    let mut end = end.min(a.len());
    while !a.is_char_boundary(end) {
        end -= 1;
    }
    &a[..end]
}

/// `filterResources(values, contents, prefix, converter, consumer)`.
pub fn filter_resources_prefixed<T>(
    values: impl IntoIterator<Item = T>,
    contents: &str,
    prefix: &str,
    converter: impl Fn(&T) -> Identifier,
    consumer: impl FnMut(T),
) {
    if contents.is_empty() {
        values.into_iter().for_each(consumer);
    } else {
        let common = common_prefix(contents, prefix);
        if !common.is_empty() {
            filter_resources(values, &contents[common.len()..], converter, consumer);
        }
    }
}

/// `suggestResource(values, builder, prefix)`.
pub fn suggest_resource_prefixed(values: impl IntoIterator<Item = Identifier>, mut builder: SuggestionsBuilder, prefix: &str) -> Suggestions {
    let contents = builder.get_remaining().to_lowercase();
    filter_resources_prefixed(values, &contents, prefix, |t| t.clone(), |v| {
        builder.suggest(format!("{prefix}{v}"));
    });
    builder.build()
}

/// `suggestResource(values, builder)`.
pub fn suggest_resource(values: impl IntoIterator<Item = Identifier>, mut builder: SuggestionsBuilder) -> Suggestions {
    let contents = builder.get_remaining().to_lowercase();
    filter_resources(values, &contents, |t| t.clone(), |v| {
        builder.suggest(v.to_string());
    });
    builder.build()
}

/// `suggestResource(values, builder, id, tooltip)`.
pub fn suggest_resource_with_tooltip<T>(
    values: impl IntoIterator<Item = T>,
    mut builder: SuggestionsBuilder,
    id: impl Fn(&T) -> Identifier,
    tooltip: impl Fn(&T) -> Option<MessageRef>,
) -> Suggestions {
    let contents = builder.get_remaining().to_lowercase();
    let mut matched = Vec::new();
    filter_resources(values, &contents, &id, |v| matched.push(v));
    for v in matched {
        builder.suggest_with_tooltip(id(&v).to_string(), tooltip(&v));
    }
    builder.build()
}

/// `suggest(values, builder)`.
pub fn suggest<S: AsRef<str>>(values: impl IntoIterator<Item = S>, mut builder: SuggestionsBuilder) -> Suggestions {
    let lower_prefix = builder.get_remaining().to_lowercase();
    for name in values {
        if matches_sub_str(&lower_prefix, &name.as_ref().to_lowercase()) {
            builder.suggest(name.as_ref());
        }
    }
    builder.build()
}

/// `suggest(values, builder, toString, tooltip)`.
pub fn suggest_with_tooltip<T>(
    values: impl IntoIterator<Item = T>,
    mut builder: SuggestionsBuilder,
    to_string: impl Fn(&T) -> String,
    tooltip: impl Fn(&T) -> Option<MessageRef>,
) -> Suggestions {
    let lower_prefix = builder.get_remaining().to_lowercase();
    for value in values {
        let name = to_string(&value);
        if matches_sub_str(&lower_prefix, &name.to_lowercase()) {
            builder.suggest_with_tooltip(name, tooltip(&value));
        }
    }
    builder.build()
}

/// Java `String.split(" ")`: trailing empty strings are removed (and an empty input gives `[""]`).
fn java_split_space(s: &str) -> Vec<&str> {
    let mut parts: Vec<&str> = s.split(' ').collect();
    while parts.len() > 1 && parts.last().is_some_and(|p| p.is_empty()) {
        parts.pop();
    }
    if parts.len() == 1 && parts[0].is_empty() && !s.is_empty() {
        parts.clear();
    }
    parts
}

/// `suggestCoordinates`.
pub fn suggest_coordinates(
    current_input: &str,
    all_suggestions: &[TextCoordinates],
    builder: SuggestionsBuilder,
    validator: impl Fn(&str) -> bool,
) -> Suggestions {
    let mut result: Vec<String> = Vec::new();
    if current_input.is_empty() {
        for c in all_suggestions {
            let full_value = format!("{} {} {}", c.x, c.y, c.z);
            if validator(&full_value) {
                result.push(c.x.clone());
                result.push(format!("{} {}", c.x, c.y));
                result.push(full_value);
            }
        }
    } else {
        let fields = java_split_space(current_input);
        if fields.len() == 1 {
            for c in all_suggestions {
                let full_value = format!("{} {} {}", fields[0], c.y, c.z);
                if validator(&full_value) {
                    result.push(format!("{} {}", fields[0], c.y));
                    result.push(full_value);
                }
            }
        } else if fields.len() == 2 {
            for c in all_suggestions {
                let full_value = format!("{} {} {}", fields[0], fields[1], c.z);
                if validator(&full_value) {
                    result.push(full_value);
                }
            }
        }
    }
    suggest(result, builder)
}

/// `suggest2DCoordinates`.
pub fn suggest_2d_coordinates(
    current_input: &str,
    all_suggestions: &[TextCoordinates],
    builder: SuggestionsBuilder,
    validator: impl Fn(&str) -> bool,
) -> Suggestions {
    let mut result: Vec<String> = Vec::new();
    if current_input.is_empty() {
        for c in all_suggestions {
            let full_value = format!("{} {}", c.x, c.z);
            if validator(&full_value) {
                result.push(c.x.clone());
                result.push(full_value);
            }
        }
    } else {
        let fields = java_split_space(current_input);
        if fields.len() == 1 {
            for c in all_suggestions {
                let full_value = format!("{} {}", fields[0], c.z);
                if validator(&full_value) {
                    result.push(full_value);
                }
            }
        }
    }
    suggest(result, builder)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_sub_str_splits_on_separators() {
        assert!(matches_sub_str("st", "stone"));
        assert!(matches_sub_str("br", "minecraft:stone_bricks".split(':').nth(1).map(|_| "stone_bricks").unwrap()));
        assert!(matches_sub_str("br", "stone_bricks"));
        assert!(!matches_sub_str("one", "stone"));
        assert!(matches_sub_str("", "x"));
    }

    #[test]
    fn java_split_semantics() {
        assert_eq!(java_split_space("1 2"), vec!["1", "2"]);
        assert_eq!(java_split_space("1 "), vec!["1"]);
        assert_eq!(java_split_space(" "), Vec::<&str>::new());
    }
}
