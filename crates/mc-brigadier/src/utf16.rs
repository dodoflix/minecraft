//! Java `String` semantics. Brigadier indexes strings in UTF-16 code units (cursor positions,
//! suggestion ranges and error positions are all visible on the wire), so every index in this crate
//! is a UTF-16 unit index, never a byte or `char` index.

use std::cmp::Ordering;

/// `String.length()`.
pub fn len16(s: &str) -> usize {
    if s.is_ascii() {
        s.len()
    } else {
        s.encode_utf16().count()
    }
}

/// `String.substring(start, end)`; panics like `StringIndexOutOfBoundsException` when out of range.
pub fn substring(s: &str, start: usize, end: usize) -> String {
    if s.is_ascii() {
        return s[start..end].to_owned();
    }
    let units: Vec<u16> = s.encode_utf16().collect();
    String::from_utf16_lossy(&units[start..end])
}

/// `String.substring(start)`.
pub fn substring_from(s: &str, start: usize) -> String {
    substring(s, start, len16(s))
}

/// Like [`substring_from`] but clamps `start` instead of panicking.
pub(crate) fn substring_from_clamped(s: &str, start: usize) -> String {
    substring_from(s, start.min(len16(s)))
}

/// `String.hashCode()`.
pub fn java_hash(s: &str) -> i32 {
    s.encode_utf16()
        .fold(0i32, |h, u| h.wrapping_mul(31).wrapping_add(i32::from(u)))
}

/// `String.compareTo` (lexicographic over UTF-16 units).
pub fn compare_to(a: &str, b: &str) -> Ordering {
    a.encode_utf16().cmp(b.encode_utf16())
}

/// `Character.toUpperCase(int)` / `toLowerCase(int)`: simple (single-char) case mappings only.
fn simple_upper(c: char) -> char {
    let mut it = c.to_uppercase();
    match (it.next(), it.next()) {
        (Some(x), None) => x,
        _ => c,
    }
}

fn simple_lower(c: char) -> char {
    let mut it = c.to_lowercase();
    match (it.next(), it.next()) {
        (Some(x), None) => x,
        _ => c,
    }
}

/// `String.compareToIgnoreCase` (JDK 16+: per code point, compare, then upper-cased, then lower-cased).
pub fn compare_to_ignore_case(a: &str, b: &str) -> Ordering {
    let mut ai = a.chars();
    let mut bi = b.chars();
    loop {
        match (ai.next(), bi.next()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) if x == y => {}
            (Some(x), Some(y)) => {
                let (ux, uy) = (simple_upper(x), simple_upper(y));
                if ux != uy {
                    let (lx, ly) = (simple_lower(ux), simple_lower(uy));
                    if lx != ly {
                        return lx.cmp(&ly);
                    }
                }
            }
        }
    }
}

/// `Character.isWhitespace(char)` (differs from `char::is_whitespace`: no NBSP/U+0085).
pub fn is_java_whitespace(u: u16) -> bool {
    matches!(u, 0x09..=0x0D | 0x1C..=0x1F | 0x20 | 0x1680 | 0x2000..=0x2006 | 0x2008..=0x200A | 0x2028 | 0x2029 | 0x205F | 0x3000)
}
