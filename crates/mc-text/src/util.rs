//! `ComponentUtils` / `CommonComponents` helpers and `GsonHelper.encodesLongerThan`.

use crate::chat_formatting::ChatFormatting;
use crate::component::{Component, TranslatableArg};
use crate::events::{ClickEvent, HoverEvent};
use crate::style::Style;
use serde_json::Value;

/// `GsonHelper.encodesLongerThan`: is the compact text a Gson `JsonWriter` (no HTML escaping) would
/// write for `json` longer than `max` UTF-16 code units?
pub fn encodes_longer_than(json: &Value, max: usize) -> bool {
    gson_text_len(json) > max
}

/// Length in UTF-16 code units of Gson's compact output (numbers print like `Double.toString`).
fn gson_text_len(v: &Value) -> usize {
    match v {
        Value::Null => 4,
        Value::Bool(b) => if *b { 4 } else { 5 },
        Value::Number(n) => match (n.as_i64(), n.as_u64()) {
            (Some(i), _) => i.to_string().len(),
            (None, Some(u)) => u.to_string().len(),
            _ => crate::value::java_double_to_string(n.as_f64().unwrap_or(0.0)).len(),
        },
        Value::String(s) => gson_string_len(s),
        Value::Array(a) => 2 + a.len().saturating_sub(1) + a.iter().map(gson_text_len).sum::<usize>(),
        Value::Object(o) => 2 + o.len().saturating_sub(1) + o.iter().map(|(k, v)| gson_string_len(k) + 1 + gson_text_len(v)).sum::<usize>(),
    }
}

fn gson_string_len(s: &str) -> usize {
    2 + s
        .chars()
        .map(|c| match c {
            '"' | '\\' | '\u{8}' | '\u{c}' | '\n' | '\r' | '\t' => 2,
            c if (c as u32) < 0x20 || matches!(c, '\u{2028}' | '\u{2029}') => 6,
            c => c.len_utf16(),
        })
        .sum::<usize>()
}

/// `ComponentUtils.DEFAULT_SEPARATOR` (`", "` in gray).
pub fn default_separator() -> Component {
    Component::literal(", ").with_format(ChatFormatting::Gray)
}

/// `ComponentUtils.DEFAULT_NO_STYLE_SEPARATOR`.
pub fn default_no_style_separator() -> Component {
    Component::literal(", ")
}

/// `ComponentUtils.mergeStyles`.
pub fn merge_styles(component: Component, style: &Style) -> Component {
    if style.is_empty() {
        return component;
    }
    let inner = component.style().clone();
    if inner.is_empty() {
        component.set_style(style.clone())
    } else if inner == *style {
        component
    } else {
        component.set_style(inner.apply_to(style))
    }
}

/// `ComponentUtils.formatList(values, separator, formatter)`.
pub fn format_list<T>(values: impl IntoIterator<Item = T>, separator: &Component, formatter: impl Fn(T) -> Component) -> Component {
    let mut it = values.into_iter();
    let Some(first) = it.next() else { return Component::empty() };
    let first = formatter(first);
    let Some(second) = it.next() else { return first };
    let mut result = Component::empty().append(first).append(separator.clone()).append(formatter(second));
    for v in it {
        result.push_sibling(separator.clone());
        result.push_sibling(formatter(v));
    }
    result
}

/// `ComponentUtils.formatList(Collection<String>)`: green, sorted.
pub fn format_string_list(values: &[String]) -> Component {
    let mut sorted: Vec<&String> = values.iter().collect();
    sorted.sort();
    match sorted.len() {
        0 => Component::empty(),
        1 => Component::literal(sorted[0].clone()).with_format(ChatFormatting::Green),
        _ => format_list(sorted, &default_separator(), |v| Component::literal(v.clone()).with_format(ChatFormatting::Green)),
    }
}

/// `ComponentUtils.wrapInSquareBrackets`.
pub fn wrap_in_square_brackets(inner: Component) -> Component {
    Component::translatable("chat.square_brackets", vec![TranslatableArg::Component(inner)])
}

/// `ComponentUtils.copyOnClickText`.
pub fn copy_on_click_text(text: &str) -> Component {
    wrap_in_square_brackets(Component::literal(text).with_style_fn(|s| {
        s.with_color_formatting(Some(ChatFormatting::Green))
            .with_click_event(Some(ClickEvent::CopyToClipboard(text.to_owned())))
            .with_hover_event(Some(HoverEvent::show_text(Component::translatable("chat.copy.click", vec![]))))
            .with_insertion(Some(text.to_owned()))
    }))
}

/// `CommonComponents.space()`.
pub fn space() -> Component {
    Component::literal(" ")
}

/// `CommonComponents.NEW_LINE`.
pub fn new_line() -> Component {
    Component::literal("\n")
}

/// `CommonComponents.ELLIPSIS`.
pub fn ellipsis() -> Component {
    Component::literal("...")
}

/// `CommonComponents.joinLines`.
pub fn join_lines(lines: &[Component]) -> Component {
    format_list(lines.iter().cloned(), &new_line(), |c| c)
}
