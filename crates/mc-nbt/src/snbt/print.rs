//! `StringTagVisitor` (compact) and `SnbtPrinterTagVisitor` (pretty) printers, plus
//! `StringTag.quoteAndEscape`.

use crate::Tag;
use std::cmp::Ordering;

// ---------------------------------------------------------------- Java number text

/// Scientific form (`d.ddde<exp>`) Java's `Double/Float.toString` would use (JDK 19+): the
/// shortest decimal that round-trips, at least two digits, closest to the exact value (ties to
/// an even last digit). Rust's `{:e}` gives the shortest digits; re-rounding the exact value to
/// `max(n, 2)` digits (`{:.*e}`, round-half-even) yields the closest decimal of that length.
fn java_sci<T>(abs: T) -> String
where
    T: std::fmt::LowerExp + std::str::FromStr + PartialEq + Copy,
{
    let shortest = format!("{abs:e}");
    let n = shortest
        .split('e')
        .next()
        .unwrap()
        .chars()
        .filter(|c| c.is_ascii_digit())
        .count();
    let rounded = format!("{:.*e}", n.max(2) - 1, abs);
    if rounded.parse::<T>().is_ok_and(|v| v == abs) {
        rounded
    } else {
        shortest
    }
}

fn java_float_string(neg: bool, sci: &str, abs_is_zero: bool, in_plain_range: bool) -> String {
    let mut out = String::new();
    if neg {
        out.push('-');
    }
    if abs_is_zero {
        out.push_str("0.0");
        return out;
    }
    let (mantissa, exp) = sci.split_once('e').expect("{:e} output");
    let exp: i32 = exp.parse().expect("exponent");
    let mut digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    if digits.len() == 2 && digits.ends_with('0') {
        digits.pop();
    }
    if in_plain_range {
        if exp >= 0 {
            let int_len = exp as usize + 1;
            if digits.len() <= int_len {
                out.push_str(&digits);
                out.extend(std::iter::repeat_n('0', int_len - digits.len()));
                out.push_str(".0");
            } else {
                out.push_str(&digits[..int_len]);
                out.push('.');
                out.push_str(&digits[int_len..]);
            }
        } else {
            out.push_str("0.");
            out.extend(std::iter::repeat_n('0', (-exp - 1) as usize));
            out.push_str(&digits);
        }
    } else {
        out.push_str(&digits[..1]);
        out.push('.');
        out.push_str(if digits.len() > 1 { &digits[1..] } else { "0" });
        out.push('E');
        out.push_str(&exp.to_string());
    }
    out
}

pub(crate) fn double_to_string(v: f64) -> String {
    if v.is_nan() {
        return "NaN".into();
    }
    if v.is_infinite() {
        return if v < 0.0 {
            "-Infinity".into()
        } else {
            "Infinity".into()
        };
    }
    let abs = v.abs();
    java_float_string(
        v.is_sign_negative(),
        &java_sci(abs),
        abs == 0.0,
        (1e-3..1e7).contains(&abs),
    )
}

pub(crate) fn float_to_string(v: f32) -> String {
    if v.is_nan() {
        return "NaN".into();
    }
    if v.is_infinite() {
        return if v < 0.0 {
            "-Infinity".into()
        } else {
            "Infinity".into()
        };
    }
    let abs = v.abs();
    java_float_string(
        v.is_sign_negative(),
        &java_sci(abs),
        abs == 0.0,
        (1e-3f32..1e7f32).contains(&abs),
    )
}

// ------------------------------------------------------------------- strings

/// `SnbtGrammar.escapeControlCharacters`.
fn escape_control_characters(c: char) -> Option<String> {
    match c {
        '\u{8}' => Some("b".into()),
        '\t' => Some("t".into()),
        '\n' => Some("n".into()),
        '\u{c}' => Some("f".into()),
        '\r' => Some("r".into()),
        c if c < ' ' => Some(format!("x{:02X}", c as u32)),
        _ => None,
    }
}

/// `StringTag.quoteAndEscape`.
pub fn quote_and_escape(input: &str) -> String {
    let mut result = String::with_capacity(input.len() + 2);
    quote_and_escape_into(input, &mut result);
    result
}

fn quote_and_escape_into(input: &str, out: &mut String) {
    let start = out.len();
    out.push(' ');
    let mut quote = '\0';
    for c in input.chars() {
        if c == '\\' {
            out.push_str("\\\\");
        } else if c != '"' && c != '\'' {
            match escape_control_characters(c) {
                Some(e) => {
                    out.push('\\');
                    out.push_str(&e);
                }
                None => out.push(c),
            }
        } else {
            if quote == '\0' {
                quote = if c == '"' { '\'' } else { '"' };
            }
            if quote == c {
                out.push('\\');
            }
            out.push(c);
        }
    }
    if quote == '\0' {
        quote = '"';
    }
    out.replace_range(start..start + 1, &quote.to_string());
    out.push(quote);
}

/// `StringTag.escapeWithoutQuotes`.
pub fn escape_without_quotes(input: &str) -> String {
    let mut out = String::new();
    for c in input.chars() {
        match c {
            '"' | '\'' | '\\' => {
                out.push('\\');
                out.push(c);
            }
            c => match escape_control_characters(c) {
                Some(e) => {
                    out.push('\\');
                    out.push_str(&e);
                }
                None => out.push(c),
            },
        }
    }
    out
}

/// `String.compareTo` (UTF-16 code unit order).
fn java_cmp(a: &str, b: &str) -> Ordering {
    a.encode_utf16().cmp(b.encode_utf16())
}

// ------------------------------------------------------------------- compact

/// `[A-Za-z._]+[A-Za-z0-9._+-]*` and not `true`/`false` (any case).
fn is_unquoted_key(key: &str) -> bool {
    let mut chars = key.chars();
    let ok_first = chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '.' || c == '_');
    ok_first
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '+' | '-'))
        && !key.eq_ignore_ascii_case("true")
        && !key.eq_ignore_ascii_case("false")
}

/// `StringTagVisitor` output (`Tag.toString()`).
pub fn print_compact(tag: &Tag) -> String {
    let mut out = String::new();
    compact(tag, &mut out);
    out
}

fn compact(tag: &Tag, out: &mut String) {
    use std::fmt::Write;
    match tag {
        Tag::End => out.push_str("END"),
        Tag::Byte(v) => {
            let _ = write!(out, "{v}b");
        }
        Tag::Short(v) => {
            let _ = write!(out, "{v}s");
        }
        Tag::Int(v) => {
            let _ = write!(out, "{v}");
        }
        Tag::Long(v) => {
            let _ = write!(out, "{v}L");
        }
        Tag::Float(v) => {
            out.push_str(&float_to_string(*v));
            out.push('f');
        }
        Tag::Double(v) => {
            out.push_str(&double_to_string(*v));
            out.push('d');
        }
        Tag::String(s) => quote_and_escape_into(s, out),
        Tag::ByteArray(v) => {
            out.push_str("[B;");
            for (i, b) in v.iter().enumerate() {
                if i != 0 {
                    out.push(',');
                }
                let _ = write!(out, "{b}B");
            }
            out.push(']');
        }
        Tag::IntArray(v) => {
            out.push_str("[I;");
            for (i, b) in v.iter().enumerate() {
                if i != 0 {
                    out.push(',');
                }
                let _ = write!(out, "{b}");
            }
            out.push(']');
        }
        Tag::LongArray(v) => {
            out.push_str("[L;");
            for (i, b) in v.iter().enumerate() {
                if i != 0 {
                    out.push(',');
                }
                let _ = write!(out, "{b}L");
            }
            out.push(']');
        }
        Tag::List(l) => {
            out.push('[');
            for (i, e) in l.iter().enumerate() {
                if i != 0 {
                    out.push(',');
                }
                compact(e, out);
            }
            out.push(']');
        }
        Tag::Compound(c) => {
            out.push('{');
            let mut entries: Vec<(&String, &Tag)> = c.iter().collect();
            entries.sort_by(|a, b| java_cmp(a.0, b.0));
            for (i, (k, v)) in entries.into_iter().enumerate() {
                if i != 0 {
                    out.push(',');
                }
                if is_unquoted_key(k) {
                    out.push_str(k);
                } else {
                    quote_and_escape_into(k, out);
                }
                out.push(':');
                compact(v, out);
            }
            out.push('}');
        }
    }
}

// -------------------------------------------------------------------- pretty

const KEY_ORDER: &[(&str, &[&str])] = &[
    (
        "{}",
        &[
            "DataVersion",
            "author",
            "size",
            "data",
            "entities",
            "palette",
            "palettes",
        ],
    ),
    ("{}.data.[].{}", &["pos", "state", "nbt"]),
    ("{}.entities.[].{}", &["blockPos", "pos"]),
];
const NO_INDENTATION: &[&str] = &[
    "{}.size.[]",
    "{}.data.[].{}",
    "{}.palette.[].{}",
    "{}.entities.[].{}",
];

fn is_simple_value(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '+' | '-'))
}

/// `SnbtPrinterTagVisitor` with the default four-space indentation.
pub fn print_pretty(tag: &Tag) -> String {
    print_pretty_with(tag, "    ", 0, &mut Vec::new())
}

/// `new SnbtPrinterTagVisitor(indentation, depth, path).visit(tag)`.
pub fn print_pretty_with(
    tag: &Tag,
    indentation: &str,
    depth: usize,
    path: &mut Vec<String>,
) -> String {
    match tag {
        Tag::End => String::new(),
        Tag::String(s) => quote_and_escape(s),
        Tag::Byte(v) => format!("{v}b"),
        Tag::Short(v) => format!("{v}s"),
        Tag::Int(v) => v.to_string(),
        Tag::Long(v) => format!("{v}L"),
        Tag::Float(v) => format!("{}f", float_to_string(*v)),
        Tag::Double(v) => format!("{}d", double_to_string(*v)),
        Tag::ByteArray(v) => array("B", v.iter().map(|b| format!("{b}B"))),
        Tag::IntArray(v) => array("I", v.iter().map(|b| b.to_string())),
        Tag::LongArray(v) => array("L", v.iter().map(|b| format!("{b}L"))),
        Tag::List(l) => {
            if l.is_empty() {
                return "[]".into();
            }
            let mut out = String::from("[");
            path.push("[]".into());
            let indentation = if NO_INDENTATION.contains(&path.join(".").as_str()) {
                ""
            } else {
                indentation
            };
            if !indentation.is_empty() {
                out.push('\n');
            }
            for (i, e) in l.iter().enumerate() {
                out.push_str(&indentation.repeat(depth + 1));
                out.push_str(&print_pretty_with(e, indentation, depth + 1, path));
                if i != l.len() - 1 {
                    out.push(',');
                    out.push_str(if indentation.is_empty() { " " } else { "\n" });
                }
            }
            if !indentation.is_empty() {
                out.push('\n');
                out.push_str(&indentation.repeat(depth));
            }
            out.push(']');
            path.pop();
            out
        }
        Tag::Compound(c) => {
            if c.is_empty() {
                return "{}".into();
            }
            let mut out = String::from("{");
            path.push("{}".into());
            let path_string = path.join(".");
            let indentation = if NO_INDENTATION.contains(&path_string.as_str()) {
                ""
            } else {
                indentation
            };
            if !indentation.is_empty() {
                out.push('\n');
            }
            let mut keys: Vec<&str> = c.keys().map(String::as_str).collect();
            let mut ordered: Vec<&str> = Vec::with_capacity(keys.len());
            if let Some((_, order)) = KEY_ORDER.iter().find(|(p, _)| *p == path_string) {
                for k in *order {
                    if let Some(pos) = keys.iter().position(|x| x == k) {
                        ordered.push(keys.swap_remove(pos));
                    }
                }
                keys.sort_by(|a, b| java_cmp(a, b));
                ordered.extend(keys);
            } else {
                keys.sort_by(|a, b| java_cmp(a, b));
                ordered = keys;
            }
            let n = ordered.len();
            for (i, key) in ordered.into_iter().enumerate() {
                let value = c.get(key).expect("key present");
                path.push(key.to_string());
                out.push_str(&indentation.repeat(depth + 1));
                out.push_str(&if is_simple_value(key) {
                    key.to_string()
                } else {
                    quote_and_escape(key)
                });
                out.push(':');
                out.push(' ');
                out.push_str(&print_pretty_with(value, indentation, depth + 1, path));
                path.pop();
                if i + 1 != n {
                    out.push(',');
                    out.push_str(if indentation.is_empty() { " " } else { "\n" });
                }
            }
            if !indentation.is_empty() {
                out.push('\n');
                out.push_str(&indentation.repeat(depth));
            }
            out.push('}');
            path.pop();
            out
        }
    }
}

fn array(prefix: &str, items: impl ExactSizeIterator<Item = String>) -> String {
    let n = items.len();
    let mut out = format!("[{prefix};");
    for (i, item) in items.enumerate() {
        out.push(' ');
        out.push_str(&item);
        if i != n - 1 {
            out.push(',');
        }
    }
    out.push(']');
    out
}
