//! Legacy `§` formatting helpers: `StringDecomposer.iterateFormatted` and conversions between
//! components and legacy-coded strings (used for console / RCON style output).

use crate::chat_formatting::{ChatFormatting, PREFIX_CODE};
use crate::component::Component;
use crate::style::Style;
use crate::text_color::TextColor;

/// `StringDecomposer.iterateFormatted(string, 0, current, reset, sink)`: feeds every visible char with
/// the style in effect after applying `§` codes. `sink(position, style, char)` returns `false` to stop.
/// Positions are in UTF-16 code units, like vanilla. Returns `false` if the sink stopped iteration.
pub fn iterate_formatted(string: &str, current_style: &Style, reset_style: &Style, sink: &mut dyn FnMut(usize, &Style, char) -> bool) -> bool {
    let units: Vec<char> = string.chars().collect();
    let mut style = current_style.clone();
    let mut pos = 0usize;
    let mut i = 0usize;
    while i < units.len() {
        let ch = units[i];
        if ch == PREFIX_CODE {
            if i + 1 >= units.len() {
                break;
            }
            let code = units[i + 1];
            if let Some(f) = ChatFormatting::get_by_code(code) {
                style = if f == ChatFormatting::Reset { reset_style.clone() } else { style.apply_legacy_format(f) };
            }
            pos += ch.len_utf16() + code.len_utf16();
            i += 2;
            continue;
        }
        if !sink(pos, &style, ch) {
            return false;
        }
        pos += ch.len_utf16();
        i += 1;
    }
    true
}

/// `StringDecomposer.getPlainText(component)`: the visible text with `§` codes interpreted (removed).
pub fn get_plain_text(component: &Component) -> String {
    let mut out = String::new();
    component.visit_styled::<()>(
        &mut |style, contents| {
            iterate_formatted(contents, style, style, &mut |_, _, c| {
                out.push(c);
                true
            });
            None
        },
        &Style::EMPTY,
    );
    out
}

/// Parses a legacy `§`-coded string into a component (siblings are runs of identical style), the same
/// style semantics as vanilla's `applyLegacyFormat`.
pub fn from_legacy_string(text: &str) -> Component {
    let mut root = Component::empty();
    let mut run = String::new();
    let mut run_style = Style::EMPTY;
    iterate_formatted(text, &Style::EMPTY, &Style::EMPTY, &mut |_, style, c| {
        if *style != run_style && !run.is_empty() {
            root.push_sibling(Component::literal(std::mem::take(&mut run)).set_style(run_style.clone()));
        }
        run_style = style.clone();
        run.push(c);
        true
    });
    if !run.is_empty() {
        root.push_sibling(Component::literal(run).set_style(run_style));
    }
    root
}

fn named_color_formatting(c: TextColor) -> Option<ChatFormatting> {
    ChatFormatting::ALL.into_iter().take(16).find(|f| TextColor::from_legacy_format(*f) == Some(c))
}

/// Renders a component with `§` codes (colour, then `§l §o §n §m §k`), resetting with `§r` whenever a
/// run's style drops a flag. Hex colours use the `§x§R§R§G§G§B§B` extension. Translatables are resolved
/// with the current [`crate::Language`].
pub fn to_legacy_string(component: &Component) -> String {
    let mut out = String::new();
    let mut last = Style::EMPTY;
    component.visit_styled::<()>(
        &mut |style, contents| {
            if contents.is_empty() {
                return None;
            }
            let drops = (last.is_bold() && !style.is_bold())
                || (last.is_italic() && !style.is_italic())
                || (last.is_underlined() && !style.is_underlined())
                || (last.is_strikethrough() && !style.is_strikethrough())
                || (last.is_obfuscated() && !style.is_obfuscated())
                || (last.color().is_some() && style.color().is_none());
            let mut emitted = last.clone();
            if drops {
                out.push(PREFIX_CODE);
                out.push('r');
                emitted = Style::EMPTY;
            }
            if let Some(c) = style.color().filter(|c| Some(*c) != emitted.color()) {
                match named_color_formatting(c) {
                    Some(f) => out.push_str(&f.to_string()),
                    None => {
                        out.push(PREFIX_CODE);
                        out.push('x');
                        for d in format!("{:06x}", c.value()).chars() {
                            out.push(PREFIX_CODE);
                            out.push(d);
                        }
                    }
                }
            }
            for (on, was, f) in [
                (style.is_bold(), emitted.is_bold(), ChatFormatting::Bold),
                (style.is_italic(), emitted.is_italic(), ChatFormatting::Italic),
                (style.is_underlined(), emitted.is_underlined(), ChatFormatting::Underline),
                (style.is_strikethrough(), emitted.is_strikethrough(), ChatFormatting::Strikethrough),
                (style.is_obfuscated(), emitted.is_obfuscated(), ChatFormatting::Obfuscated),
            ] {
                if on && !was {
                    out.push_str(&f.to_string());
                }
            }
            out.push_str(contents);
            last = style.clone();
            None
        },
        &Style::EMPTY,
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_roundtrip() {
        let c = Component::literal("a").with_format(ChatFormatting::Red).with_format(ChatFormatting::Bold).append(Component::literal("b"));
        assert_eq!(to_legacy_string(&c), "\u{a7}c\u{a7}lab");
        let back = from_legacy_string("\u{a7}cx\u{a7}ly");
        assert_eq!(back.get_string(), "xy");
        assert_eq!(back.siblings().len(), 2);
        assert_eq!(get_plain_text(&Component::literal("\u{a7}ahi")), "hi");
    }
}
