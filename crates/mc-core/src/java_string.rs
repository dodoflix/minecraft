//! Java `String` semantics that differ from Rust's `String`: UTF-16 strings that may hold lone surrogates
//! ([`JavaString`]) and the JDK's UTF-8 decoding.

use std::borrow::Cow;
use std::cmp::Ordering;
use std::fmt;
use std::hash::{Hash, Hasher};

/// A Java `String`: a sequence of UTF-16 code units, which unlike a Rust `String` may hold lone surrogates.
///
/// The common case is a plain Rust string. A string that holds lone surrogates additionally keeps its exact
/// units; its text then shows each lone surrogate as U+FFFD, one character per unit, so lengths and unit offsets
/// of the text and of the Java string agree. Code that only needs the characters reads the text
/// ([`as_lossy_str`](Self::as_lossy_str)); code that writes, prints, compares or measures a string uses the exact methods
/// ([`encode_utf16`](Self::encode_utf16), equality, [`to_java_utf8`](Self::to_java_utf8)).
#[derive(Clone, Default)]
pub struct JavaString {
    text: String,
    /// The exact units; only when the text lost a lone surrogate.
    units: Option<Box<Units>>,
}

/// Boxed so that a string without lone surrogates stays small, a `Vec` so that appending stays cheap.
#[derive(Clone, PartialEq, Eq, Hash)]
struct Units(Vec<u16>);

/// The units of a [`JavaString`], like `str::encode_utf16`.
#[derive(Clone)]
pub enum Utf16Units<'a> {
    Text(std::str::EncodeUtf16<'a>),
    Exact(std::iter::Copied<std::slice::Iter<'a, u16>>),
}

impl Iterator for Utf16Units<'_> {
    type Item = u16;
    fn next(&mut self) -> Option<u16> {
        match self {
            Utf16Units::Text(i) => i.next(),
            Utf16Units::Exact(i) => i.next(),
        }
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        match self {
            Utf16Units::Text(i) => i.size_hint(),
            Utf16Units::Exact(i) => i.size_hint(),
        }
    }
}

impl JavaString {
    pub const fn new() -> Self {
        JavaString {
            text: String::new(),
            units: None,
        }
    }

    /// The string of exactly these UTF-16 units; a valid pair becomes one character, a lone surrogate is kept.
    pub fn from_utf16_units(units: &[u16]) -> Self {
        let mut text = String::with_capacity(units.len());
        let mut lone = false;
        for r in char::decode_utf16(units.iter().copied()) {
            match r {
                Ok(c) => text.push(c),
                Err(_) => {
                    lone = true;
                    text.push('\u{FFFD}');
                }
            }
        }
        JavaString {
            text,
            units: lone.then(|| Box::new(Units(units.to_vec()))),
        }
    }

    /// The text, each lone surrogate shown as U+FFFD (lossy: never for output, comparison or measuring).
    pub fn as_lossy_str(&self) -> &str {
        &self.text
    }

    /// The text as a `String`, each lone surrogate shown as U+FFFD.
    pub fn into_string_lossy(self) -> String {
        self.text
    }

    /// `f` applied to the string, each lone surrogate standing as a private-use character in `f`'s text and coming back
    /// as the same unit from its result: a rename, `replace`, `toLowerCase` or a split and join keeps every lone
    /// surrogate where Java does, and a string that no rule matches passes through untouched. See
    /// [`with_stand_ins`](Self::with_stand_ins) for what `f` may rely on.
    pub fn map_lossless(&self, f: impl FnOnce(&str) -> String) -> JavaString {
        self.with_stand_ins(|text, back| back(&f(text)))
    }

    /// `f` called with the text of the string, each lone surrogate standing as a private-use character, and the function
    /// that turns text made from it back into a Java string (the stand-ins become the same units again): for a rewrite
    /// that builds several strings from one. The stand-ins are one unit long like the surrogate and come from a
    /// 2048-character block of U+E000..U+F7FF that the string does not use (`f` may only copy or drop them), so the text
    /// measures and splits like the Java string; it sorts differently. Without a free block the lone surrogates show as
    /// U+FFFD, like [`as_lossy_str`](Self::as_lossy_str).
    pub fn with_stand_ins<R>(&self, f: impl FnOnce(&str, &dyn Fn(&str) -> JavaString) -> R) -> R {
        let plain = |s: &str| JavaString::from(s);
        let Some(units) = &self.units else {
            return f(&self.text, &plain);
        };
        let used = |base: u32| {
            self.text
                .chars()
                .any(|c| (base..base + 0x800).contains(&(c as u32)))
        };
        let Some(base) = [0xE000, 0xE800, 0xF000].into_iter().find(|&b| !used(b)) else {
            return f(&self.text, &plain);
        };
        let standing: String = char::decode_utf16(units.0.iter().copied())
            .map(|r| match r {
                Ok(c) => c,
                Err(e) => char::from_u32(base + u32::from(e.unpaired_surrogate()) - 0xD800)
                    .expect("a private-use character"),
            })
            .collect();
        let back = |s: &str| {
            if !s
                .chars()
                .any(|c| (base..base + 0x800).contains(&(c as u32)))
            {
                return JavaString::from(s);
            }
            let mut out = Vec::with_capacity(s.len());
            for c in s.chars() {
                match c as u32 {
                    u if (base..base + 0x800).contains(&u) => out.push((u - base + 0xD800) as u16),
                    _ => out.extend(c.encode_utf16(&mut [0; 2]).iter()),
                }
            }
            JavaString::from_utf16_units(&out)
        };
        f(&standing, &back)
    }

    /// The text, only when the string holds no lone surrogate (so it can equal a Rust string).
    pub fn exact_str(&self) -> Option<&str> {
        self.units.is_none().then_some(self.text.as_str())
    }

    pub fn has_lone_surrogate(&self) -> bool {
        self.units.is_some()
    }

    /// `String.length()`.
    pub fn utf16_len(&self) -> usize {
        match &self.units {
            Some(u) => u.0.len(),
            None if self.text.is_ascii() => self.text.len(),
            None => self.text.encode_utf16().count(),
        }
    }

    /// The exact UTF-16 units (`String.charAt` for each index).
    pub fn encode_utf16(&self) -> Utf16Units<'_> {
        match &self.units {
            Some(u) => Utf16Units::Exact(u.0.iter().copied()),
            None => Utf16Units::Text(self.text.encode_utf16()),
        }
    }

    /// `String.substring(start, end)` by unit index. Panics when out of range, like Java throws.
    pub fn substring(&self, start: usize, end: usize) -> JavaString {
        match &self.units {
            Some(u) => JavaString::from_utf16_units(&u.0[start..end]),
            None if self.text.is_ascii() => JavaString::from(&self.text[start..end]),
            None => {
                let u: Vec<u16> = self.text.encode_utf16().collect();
                JavaString::from_utf16_units(&u[start..end])
            }
        }
    }

    pub fn push(&mut self, c: char) {
        self.text.push(c);
        if let Some(u) = &mut self.units {
            u.0.extend(c.encode_utf16(&mut [0; 2]).iter());
        }
    }

    /// Appends text; valid text cannot complete a lone surrogate, so no unit merges.
    pub fn push_str(&mut self, s: &str) {
        self.text.push_str(s);
        if let Some(u) = &mut self.units {
            u.0.extend(s.encode_utf16());
        }
    }

    /// `StringBuilder.append(String)`: a lone high surrogate at the end and a lone low surrogate at the start
    /// join into one character.
    pub fn push_java(&mut self, other: &JavaString) {
        let Some(other_units) = &other.units else {
            self.push_str(&other.text);
            return;
        };
        let high_last = self
            .units
            .as_ref()
            .and_then(|u| u.0.last())
            .is_some_and(|u| (0xD800..0xDC00).contains(u));
        if high_last && (0xDC00..0xE000).contains(&other_units.0[0]) {
            let mut u: Vec<u16> = self.encode_utf16().collect();
            u.extend(&other_units.0);
            *self = JavaString::from_utf16_units(&u);
            return;
        }
        let units = self
            .units
            .get_or_insert_with(|| Box::new(Units(self.text.encode_utf16().collect())));
        units.0.extend(&other_units.0);
        self.text.push_str(&other.text);
    }

    /// Appends one UTF-16 unit, which may be a surrogate (see [`push_java`](Self::push_java) for joining).
    pub fn push_utf16_unit(&mut self, unit: u16) {
        match char::from_u32(u32::from(unit)) {
            Some(c) => self.push(c),
            None => self.push_java(&JavaString::from_utf16_units(&[unit])),
        }
    }

    /// `String.hashCode()`.
    pub fn java_hash_code(&self) -> i32 {
        self.encode_utf16()
            .fold(0i32, |h, u| h.wrapping_mul(31).wrapping_add(i32::from(u)))
    }

    /// `String.compareTo`: UTF-16 code unit order.
    pub fn java_cmp(&self, other: &JavaString) -> Ordering {
        self.encode_utf16().cmp(other.encode_utf16())
    }

    /// The string as UTF-8 text: each lone surrogate becomes `?`, which is what encoding a Java string to UTF-8 does
    /// (`String.getBytes(UTF_8)`, Netty's `ByteBufUtil.writeUtf8`, the writers of files and sockets).
    pub fn to_utf8_string(&self) -> Cow<'_, str> {
        match &self.units {
            None => Cow::Borrowed(&self.text),
            Some(u) => Cow::Owned(
                char::decode_utf16(u.0.iter().copied())
                    .map(|r| r.unwrap_or('?'))
                    .collect(),
            ),
        }
    }

    /// The bytes of [`to_utf8_string`](Self::to_utf8_string).
    pub fn to_java_utf8(&self) -> Cow<'_, [u8]> {
        match self.to_utf8_string() {
            Cow::Borrowed(s) => Cow::Borrowed(s.as_bytes()),
            Cow::Owned(s) => Cow::Owned(s.into_bytes()),
        }
    }
}

impl fmt::Write for JavaString {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.push_str(s);
        Ok(())
    }
}

impl PartialEq for JavaString {
    fn eq(&self, other: &Self) -> bool {
        match (&self.units, &other.units) {
            (None, None) => self.text == other.text,
            (Some(a), Some(b)) => a == b,
            _ => false,
        }
    }
}

impl Eq for JavaString {}

impl PartialEq<str> for JavaString {
    fn eq(&self, other: &str) -> bool {
        self.units.is_none() && self.text == other
    }
}

impl PartialEq<&str> for JavaString {
    fn eq(&self, other: &&str) -> bool {
        *self == **other
    }
}

impl PartialEq<String> for JavaString {
    fn eq(&self, other: &String) -> bool {
        *self == **other
    }
}

impl PartialEq<JavaString> for str {
    fn eq(&self, other: &JavaString) -> bool {
        *other == *self
    }
}

impl PartialEq<JavaString> for &str {
    fn eq(&self, other: &JavaString) -> bool {
        *other == **self
    }
}

impl PartialEq<JavaString> for String {
    fn eq(&self, other: &JavaString) -> bool {
        *other == **self
    }
}

/// A string without lone surrogates hashes exactly like its `str`.
impl Hash for JavaString {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.text.hash(state);
        if let Some(u) = &self.units {
            u.hash(state);
        }
    }
}

/// Like `str`'s `Debug`; a lone surrogate shows as `\u{d800}`.
impl fmt::Debug for JavaString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Some(u) = &self.units else {
            return fmt::Debug::fmt(&self.text, f);
        };
        f.write_str("\"")?;
        for r in char::decode_utf16(u.0.iter().copied()) {
            match r {
                Ok(c) => write!(f, "{}", c.escape_debug())?,
                Err(e) => write!(f, "\\u{{{:x}}}", e.unpaired_surrogate())?,
            }
        }
        f.write_str("\"")
    }
}

impl From<&str> for JavaString {
    fn from(s: &str) -> Self {
        JavaString {
            text: s.to_owned(),
            units: None,
        }
    }
}

impl From<String> for JavaString {
    fn from(text: String) -> Self {
        JavaString { text, units: None }
    }
}

impl From<&String> for JavaString {
    fn from(s: &String) -> Self {
        JavaString::from(s.as_str())
    }
}

impl From<Cow<'_, str>> for JavaString {
    fn from(s: Cow<'_, str>) -> Self {
        JavaString::from(s.into_owned())
    }
}

impl From<char> for JavaString {
    fn from(c: char) -> Self {
        JavaString::from(c.to_string())
    }
}

impl From<&JavaString> for JavaString {
    fn from(s: &JavaString) -> Self {
        s.clone()
    }
}

/// A value that [`jformat!`] can put in a Java string: a Java string exactly, anything with a `Display` by its text.
pub trait JavaDisplay {
    fn write_java(&self, out: &mut JavaString);
}

impl<T: fmt::Display + ?Sized> JavaDisplay for T {
    fn write_java(&self, out: &mut JavaString) {
        use fmt::Write;
        let _ = write!(out, "{self}");
    }
}

impl JavaDisplay for JavaString {
    fn write_java(&self, out: &mut JavaString) {
        out.push_java(self);
    }
}

/// [`jformat!`]: `template` with each `{}` replaced by the next argument (`{{` and `}}` are braces).
pub fn format_java(template: &str, args: &[&dyn JavaDisplay]) -> JavaString {
    let mut out = JavaString::new();
    let mut args = args.iter();
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        match (c, chars.peek()) {
            ('{', Some('{')) | ('}', Some('}')) => {
                out.push(c);
                chars.next();
            }
            ('{', Some('}')) => {
                chars.next();
                args.next()
                    .expect("a format argument for every `{}`")
                    .write_java(&mut out);
            }
            _ => out.push(c),
        }
    }
    assert!(args.next().is_none(), "an unused format argument");
    out
}

/// `format!` for Java strings: only plain `{}` placeholders, whose arguments may be [`JavaString`]s that keep their lone
/// surrogates.
#[macro_export]
macro_rules! jformat {
    ($template:literal $(, $arg:expr)* $(,)?) => {
        $crate::java_string::format_java($template, &[$(&$arg as &dyn $crate::java_string::JavaDisplay),*])
    };
}

/// `new String(bytes, StandardCharsets.UTF_8)` (what Netty's `ByteBuf.toString(UTF_8)` does): malformed input becomes
/// U+FFFD, but Java and Rust disagree on how many replacement characters a malformed sequence yields (Java replaces
/// an encoded surrogate `ed a0 80` by one, Rust by three). The result never holds a lone surrogate.
pub fn decode_utf8(bytes: &[u8]) -> String {
    // Well-formed UTF-8 decodes identically.
    if let Ok(s) = std::str::from_utf8(bytes) {
        return s.to_owned();
    }
    let n = bytes.len();
    let mut out = String::with_capacity(n);
    let cont = |b: u8| b & 0xC0 == 0x80;
    let mut sp = 0;
    while sp < n {
        let b1 = bytes[sp];
        sp += 1;
        if b1 < 0x80 {
            out.push(b1 as char);
        } else if b1 >> 5 == 0b110 && b1 & 0x1e != 0 {
            match bytes.get(sp) {
                Some(&b2) if cont(b2) => {
                    out.push(
                        char::from_u32(u32::from(b1 & 0x1f) << 6 | u32::from(b2 & 0x3f)).unwrap(),
                    );
                    sp += 1;
                }
                Some(_) => out.push('\u{FFFD}'),
                None => {
                    out.push('\u{FFFD}');
                    break;
                }
            }
        } else if b1 >> 4 == 0b1110 {
            if sp + 1 < n {
                let (b2, b3) = (bytes[sp], bytes[sp + 1]);
                if (b1 == 0xE0 && b2 & 0xE0 == 0x80) || !cont(b2) || !cont(b3) {
                    out.push('\u{FFFD}');
                    // JDK `malformed3`: skip 1 byte when the second byte cannot continue, else 2
                    sp += if (b1 == 0xE0 && b2 & 0xE0 == 0x80) || !cont(b2) {
                        0
                    } else {
                        1
                    };
                } else {
                    let c = u32::from(b1 & 0x0f) << 12
                        | u32::from(b2 & 0x3f) << 6
                        | u32::from(b3 & 0x3f);
                    // an encoded surrogate is one malformed sequence of three bytes
                    out.push(char::from_u32(c).unwrap_or('\u{FFFD}'));
                    sp += 2;
                }
            } else if sp < n && ((b1 == 0xE0 && bytes[sp] & 0xE0 == 0x80) || !cont(bytes[sp])) {
                out.push('\u{FFFD}');
            } else {
                out.push('\u{FFFD}');
                break;
            }
        } else if b1 >> 3 == 0b11110 {
            if sp + 2 < n {
                let (b2, b3, b4) = (bytes[sp], bytes[sp + 1], bytes[sp + 2]);
                let uc = u32::from(b1 & 0x07) << 18
                    | u32::from(b2 & 0x3f) << 12
                    | u32::from(b3 & 0x3f) << 6
                    | u32::from(b4 & 0x3f);
                if !cont(b2) || !cont(b3) || !cont(b4) || !(0x10000..=0x10FFFF).contains(&uc) {
                    out.push('\u{FFFD}');
                    // JDK `malformed4`
                    sp += if b1 > 0xF4
                        || (b1 == 0xF0 && !(0x90..=0xBF).contains(&b2))
                        || (b1 == 0xF4 && b2 & 0xF0 != 0x80)
                        || !cont(b2)
                    {
                        0
                    } else if !cont(b3) {
                        1
                    } else {
                        2
                    };
                } else {
                    out.push(char::from_u32(uc).unwrap());
                    sp += 3;
                }
            } else if b1 > 0xF4
                || bytes.get(sp).is_some_and(|&b2| {
                    (b1 == 0xF0 && !(0x90..=0xBF).contains(&b2))
                        || (b1 == 0xF4 && b2 & 0xF0 != 0x80)
                        || !cont(b2)
                })
            {
                out.push('\u{FFFD}');
            } else {
                out.push('\u{FFFD}');
                sp += 1;
                if bytes.get(sp).is_some_and(|&b3| !cont(b3)) {
                    continue;
                }
                break;
            }
        } else {
            out.push('\u{FFFD}');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lone() -> JavaString {
        JavaString::from_utf16_units(&[0xD800])
    }

    #[test]
    fn jformat_keeps_lone_surrogates() {
        let s = crate::jformat!("a {} b {{{}}} {}", lone(), 5, "c");
        assert_eq!(
            s.encode_utf16().collect::<Vec<_>>(),
            "a \u{0} b {5} c"
                .encode_utf16()
                .map(|u| if u == 0 { 0xD800 } else { u })
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn map_lossless_keeps_lone_surrogates_through_a_rewrite() {
        let units = |s: &JavaString| s.encode_utf16().collect::<Vec<_>>();
        // `Ab` + lone high + `Cd` + a private-use character of the first stand-in block
        let s = JavaString::from_utf16_units(&[0x41, 0x62, 0xD800, 0x43, 0x64, 0xE000]);
        let lower = s.map_lossless(str::to_lowercase);
        assert_eq!(units(&lower), [0x61, 0x62, 0xD800, 0x63, 0x64, 0xE000]);
        // the text `f` sees measures like the Java string
        let n = std::cell::Cell::new(0);
        let same = s.map_lossless(|t| {
            n.set(t.encode_utf16().count());
            t.to_owned()
        });
        assert_eq!((n.get(), same == s), (6, true));
        // text around a lone surrogate is rewritten, a lone surrogate may be dropped or moved
        let dropped = lone().map_lossless(|_| String::new());
        assert_eq!(dropped.utf16_len(), 0);
        let replaced = JavaString::from_utf16_units(&[0x61, 0xDC00, 0x61])
            .map_lossless(|t| t.replace('a', "bb"));
        assert_eq!(units(&replaced), [0x62, 0x62, 0xDC00, 0x62, 0x62]);
        // a plain string is just mapped
        assert_eq!(JavaString::from("Ab").map_lossless(str::to_lowercase), "ab");
    }

    #[test]
    fn lone_surrogates_are_kept_and_distinct_from_fffd() {
        let (a, b) = (lone(), JavaString::from("\u{FFFD}"));
        assert_eq!(a.as_lossy_str(), b.as_lossy_str());
        assert_ne!(a, b);
        assert!(a.has_lone_surrogate());
        assert_eq!(a.utf16_len(), 1);
        assert_eq!(a.encode_utf16().collect::<Vec<_>>(), [0xD800]);
        assert_eq!(&*a.to_java_utf8(), b"?");
        assert_eq!(&*b.to_java_utf8(), "\u{FFFD}".as_bytes());
        assert_eq!(format!("{a:?}"), "\"\\u{d800}\"");
    }

    #[test]
    fn plain_strings_compare_and_hash_like_str() {
        use std::collections::hash_map::DefaultHasher;
        let h = |v: &dyn Fn(&mut DefaultHasher)| {
            let mut s = DefaultHasher::new();
            v(&mut s);
            s.finish()
        };
        let j = JavaString::from("abc");
        assert_eq!(j, "abc");
        assert_ne!(lone(), "\u{FFFD}");
        assert_eq!(h(&|s| j.hash(s)), h(&|s| "abc".hash(s)));
        assert_eq!(format!("{j:?}"), format!("{:?}", "abc"));
    }

    #[test]
    fn a_pair_split_over_two_strings_joins() {
        let mut s = JavaString::from_utf16_units(&[0x61, 0xD83D]);
        s.push_java(&JavaString::from_utf16_units(&[0xDE00, 0x62]));
        assert_eq!(s, "a\u{1F600}b");
        assert!(!s.has_lone_surrogate());
        let mut t = JavaString::from_utf16_units(&[0xDE00, 0xD83D]);
        t.push_str("x");
        t.push('\u{1F600}');
        assert_eq!(t.utf16_len(), 5);
        assert!(t.has_lone_surrogate());
    }

    #[test]
    fn units_append_one_by_one() {
        let mut s = JavaString::from("a");
        for u in [0xD83D, 0xDE00, 0xDE00, 0xD83D, 0x62] {
            s.push_utf16_unit(u);
        }
        assert_eq!(
            s.encode_utf16().collect::<Vec<_>>(),
            [0x61, 0xD83D, 0xDE00, 0xDE00, 0xD83D, 0x62]
        );
        assert_eq!(s.as_lossy_str(), "a\u{1F600}\u{FFFD}\u{FFFD}b");
    }

    #[test]
    fn substring_splits_a_pair_by_unit() {
        let s = JavaString::from("a\u{1F600}");
        assert_eq!(
            s.substring(1, 2).encode_utf16().collect::<Vec<_>>(),
            [0xD83D]
        );
        assert_eq!(
            s.substring(2, 3).encode_utf16().collect::<Vec<_>>(),
            [0xDE00]
        );
        assert_eq!(s.substring(0, 3), s);
    }

    #[test]
    fn java_order_is_utf16_unit_order() {
        let (bmp, supp) = (JavaString::from("\u{E000}"), JavaString::from("\u{10000}"));
        assert_eq!(bmp.as_lossy_str().cmp(supp.as_lossy_str()), Ordering::Less);
        assert_eq!(bmp.java_cmp(&supp), Ordering::Greater);
        assert_eq!(lone().java_cmp(&bmp), Ordering::Less);
    }

    #[test]
    fn decode_utf8_counts_replacements_like_java() {
        assert_eq!(decode_utf8(b"\xed\xa0\x80"), "\u{FFFD}");
        assert_eq!(decode_utf8(b"a\xc0\x80b"), "a\u{FFFD}\u{FFFD}b");
        assert_eq!(decode_utf8("\u{1F600}".as_bytes()), "\u{1F600}");
    }
}
