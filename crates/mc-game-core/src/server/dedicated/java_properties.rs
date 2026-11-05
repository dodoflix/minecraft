//! `java.util.Properties`: `load(Reader)` / `store(Writer|OutputStream)` byte-compatible with the JDK
//! (the JDK writes entries sorted by key since 18, which is what vanilla's `server.properties` shows).

use std::collections::HashMap;

use chrono::{Datelike, Local, Offset, Timelike};

/// Platform `line.separator` as written by `BufferedWriter.newLine()`.
const LINE_SEP: &str = if cfg!(windows) { "\r\n" } else { "\n" };

/// `Properties.load` hit a `\uXXXX` escape that is not four hex digits (`IllegalArgumentException`).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("Malformed \\uxxxx encoding.")]
pub struct MalformedUnicodeEscape;

/// The key/value table of a `java.util.Properties` (string keys and values only).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Properties {
    map: HashMap<String, String>,
}

impl Properties {
    pub fn new() -> Properties {
        Properties::default()
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.map.get(key).map(String::as_str)
    }

    /// `getProperty(key, default)`.
    pub fn get_property<'a>(&'a self, key: &str, default: &'a str) -> &'a str {
        self.get(key).unwrap_or(default)
    }

    pub fn put(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.map.insert(key.into(), value.into());
    }

    pub fn remove(&mut self, key: &str) -> Option<String> {
        self.map.remove(key)
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Entries in `String.compareTo` (UTF-16 code unit) key order, as `store` writes them.
    pub fn sorted_entries(&self) -> Vec<(&str, &str)> {
        let mut entries: Vec<(&str, &str)> = self.map.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        entries.sort_by(|a, b| a.0.encode_utf16().cmp(b.0.encode_utf16()));
        entries
    }

    /// `Properties.load(Reader)` over already-decoded text.
    pub fn load(text: &str) -> Result<Properties, MalformedUnicodeEscape> {
        let chars: Vec<char> = text.chars().collect();
        let mut reader = LineReader { chars: &chars, pos: 0 };
        let mut props = Properties::new();
        let mut line = Vec::new();
        while reader.read_line(&mut line) {
            let limit = line.len();
            let mut key_len = 0;
            let mut value_start = limit;
            let mut has_sep = false;
            let mut preceding_backslash = false;
            while key_len < limit {
                let c = line[key_len];
                if (c == '=' || c == ':') && !preceding_backslash {
                    value_start = key_len + 1;
                    has_sep = true;
                    break;
                } else if is_ws(c) && !preceding_backslash {
                    value_start = key_len + 1;
                    break;
                }
                preceding_backslash = c == '\\' && !preceding_backslash;
                key_len += 1;
            }
            while value_start < limit {
                let c = line[value_start];
                if !is_ws(c) {
                    if !has_sep && (c == '=' || c == ':') {
                        has_sep = true;
                    } else {
                        break;
                    }
                }
                value_start += 1;
            }
            let key = load_convert(&line[..key_len])?;
            let value = load_convert(&line[value_start..])?;
            props.map.insert(key, value);
        }
        Ok(props)
    }

    /// `Properties.load(InputStream)`: ISO-8859-1.
    pub fn load_latin1(bytes: &[u8]) -> Result<Properties, MalformedUnicodeEscape> {
        let text: String = bytes.iter().map(|&b| b as char).collect();
        Properties::load(&text)
    }

    /// `Properties.store(Writer, comments)` (non-ASCII is written as is).
    pub fn store(&self, comments: Option<&str>) -> String {
        self.store0(comments, false)
    }

    /// `Properties.store(OutputStream, comments)` as ISO-8859-1 bytes (anything above `~` is `\uXXXX`-escaped).
    pub fn store_latin1(&self, comments: Option<&str>) -> Vec<u8> {
        // every char of the output is <= 0xFF by construction (escUnicode), so this is lossless
        self.store0(comments, true).chars().map(|c| c as u32 as u8).collect()
    }

    fn store0(&self, comments: Option<&str>, esc_unicode: bool) -> String {
        let mut out = String::new();
        if let Some(comments) = comments {
            write_comments(&mut out, comments);
        }
        out.push('#');
        out.push_str(&java_date_string());
        out.push_str(LINE_SEP);
        for (key, value) in self.sorted_entries() {
            out.push_str(&save_convert(key, true, esc_unicode));
            out.push('=');
            out.push_str(&save_convert(value, false, esc_unicode));
            out.push_str(LINE_SEP);
        }
        out
    }
}

fn is_ws(c: char) -> bool {
    c == ' ' || c == '\t' || c == '\x0c'
}

struct LineReader<'a> {
    chars: &'a [char],
    pos: usize,
}

impl LineReader<'_> {
    /// `Properties.LineReader.readLine`: the next logical line (comments, blank lines and continuations resolved).
    fn read_line(&mut self, line: &mut Vec<char>) -> bool {
        line.clear();
        let mut skip_white_space = true;
        let mut is_comment_line = false;
        let mut is_new_line = true;
        let mut appended_line_begin = false;
        let mut preceding_backslash = false;
        let mut skip_lf = false;
        loop {
            let Some(&c) = self.chars.get(self.pos) else {
                if line.is_empty() || is_comment_line {
                    return false;
                }
                if preceding_backslash {
                    line.pop();
                }
                return true;
            };
            self.pos += 1;
            if skip_lf {
                skip_lf = false;
                if c == '\n' {
                    continue;
                }
            }
            if skip_white_space {
                if is_ws(c) {
                    continue;
                }
                if !appended_line_begin && (c == '\r' || c == '\n') {
                    continue;
                }
                skip_white_space = false;
                appended_line_begin = false;
            }
            if is_new_line {
                is_new_line = false;
                if c == '#' || c == '!' {
                    is_comment_line = true;
                    continue;
                }
            }
            if c != '\n' && c != '\r' {
                line.push(c);
                preceding_backslash = c == '\\' && !preceding_backslash;
            } else {
                if is_comment_line || line.is_empty() {
                    is_comment_line = false;
                    is_new_line = true;
                    skip_white_space = true;
                    line.clear();
                    continue;
                }
                if self.pos >= self.chars.len() {
                    if preceding_backslash {
                        line.pop();
                    }
                    return true;
                }
                if preceding_backslash {
                    line.pop();
                    skip_white_space = true;
                    appended_line_begin = true;
                    preceding_backslash = false;
                    if c == '\r' {
                        skip_lf = true;
                    }
                } else {
                    return true;
                }
            }
        }
    }
}

/// `Properties.loadConvert`: resolve backslash escapes.
fn load_convert(input: &[char]) -> Result<String, MalformedUnicodeEscape> {
    let mut out = String::with_capacity(input.len());
    let mut units: Vec<u16> = Vec::new(); // pending UTF-16 units so \uD83D\uDE00 pairs decode like Java strings
    let flush = |units: &mut Vec<u16>, out: &mut String| {
        if !units.is_empty() {
            out.push_str(&String::from_utf16_lossy(units));
            units.clear();
        }
    };
    let mut i = 0;
    while i < input.len() {
        let c = input[i];
        i += 1;
        if c != '\\' {
            flush(&mut units, &mut out);
            out.push(c);
            continue;
        }
        // a trailing lone backslash is dropped by Java's readLine; defensively keep nothing
        let Some(&e) = input.get(i) else { break };
        i += 1;
        match e {
            'u' => {
                let mut value: u32 = 0;
                for _ in 0..4 {
                    let digit = input.get(i).and_then(|c| c.to_digit(16)).ok_or(MalformedUnicodeEscape)?;
                    i += 1;
                    value = (value << 4) + digit;
                }
                units.push(value as u16);
            }
            't' | 'r' | 'n' | 'f' => {
                flush(&mut units, &mut out);
                out.push(match e {
                    't' => '\t',
                    'r' => '\r',
                    'n' => '\n',
                    _ => '\x0c',
                });
            }
            other => {
                flush(&mut units, &mut out);
                out.push(other);
            }
        }
    }
    flush(&mut units, &mut out);
    Ok(out)
}

/// `Properties.saveConvert`.
fn save_convert(s: &str, escape_space: bool, esc_unicode: bool) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    let mut first = true;
    for c in s.chars() {
        match c {
            ' ' => {
                if first || escape_space {
                    out.push('\\');
                }
                out.push(' ');
            }
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\x0c' => out.push_str("\\f"),
            '=' | ':' | '#' | '!' => {
                out.push('\\');
                out.push(c);
            }
            c if (c < ' ' || c > '~') && esc_unicode => {
                let mut buf = [0u16; 2];
                for unit in c.encode_utf16(&mut buf) {
                    out.push_str(&format!("\\u{unit:04X}"));
                }
            }
            c => out.push(c),
        }
        first = false;
    }
    out
}

/// `Properties.writeComments`.
fn write_comments(out: &mut String, comments: &str) {
    out.push('#');
    let chars: Vec<char> = comments.chars().collect();
    let mut current = 0;
    let mut last = 0;
    while current < chars.len() {
        let c = chars[current];
        if c > '\u{ff}' || c == '\n' || c == '\r' {
            if last != current {
                out.extend(&chars[last..current]);
            }
            if c > '\u{ff}' {
                let mut buf = [0u16; 2];
                for unit in c.encode_utf16(&mut buf) {
                    out.push_str(&format!("\\u{unit:04X}"));
                }
            } else {
                out.push_str(LINE_SEP);
                if c == '\r' && current != chars.len() - 1 && chars[current + 1] == '\n' {
                    current += 1;
                }
                if current == chars.len() - 1 || (chars[current + 1] != '#' && chars[current + 1] != '!') {
                    out.push('#');
                }
            }
            last = current + 1;
        }
        current += 1;
    }
    if last != current {
        out.extend(&chars[last..current]);
    }
    out.push_str(LINE_SEP);
}

/// `new Date().toString()`: `EEE MMM dd HH:mm:ss zzz yyyy`. The zone is `UTC` for offset 0 and `GMT+hh:mm`
/// otherwise (the JDK prints that form for zones without a short display name; named zones such as `CET` are not
/// reproduced).
pub fn java_date_string() -> String {
    java_date_string_of(Local::now())
}

fn java_date_string_of<Tz: chrono::TimeZone>(now: chrono::DateTime<Tz>) -> String {
    const DAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let offset = now.offset().fix().local_minus_utc();
    let zone = if offset == 0 {
        "UTC".to_owned()
    } else {
        let sign = if offset < 0 { '-' } else { '+' };
        let abs = offset.abs();
        format!("GMT{sign}{:02}:{:02}", abs / 3600, abs % 3600 / 60)
    };
    format!(
        "{} {} {:02} {:02}:{:02}:{:02} {} {}",
        DAYS[now.weekday().num_days_from_monday() as usize],
        MONTHS[now.month0() as usize],
        now.day(),
        now.hour(),
        now.minute(),
        now.second(),
        zone,
        now.year()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_formats() {
        let p = Properties::load("# c\n! c2\n\na=1\nb:2\nc 3\nd = 4\n  e\t=\tf g \nk\\ ey=v\\u0041\\n\\tx\nm=one\\\n   two\nn\n").unwrap();
        assert_eq!(p.get("a"), Some("1"));
        assert_eq!(p.get("b"), Some("2"));
        assert_eq!(p.get("c"), Some("3"));
        assert_eq!(p.get("d"), Some("4"));
        assert_eq!(p.get("e"), Some("f g "));
        assert_eq!(p.get("k ey"), Some("vA\n\tx"));
        assert_eq!(p.get("m"), Some("onetwo"));
        assert_eq!(p.get("n"), Some(""));
        assert_eq!(p.len(), 8);
    }

    #[test]
    fn load_crlf_and_continuation_edge_cases() {
        let p = Properties::load("a=1\r\nb=2\\\r\n3\r\nc=\\\\\n#x\\\nd=4").unwrap();
        assert_eq!(p.get("a"), Some("1"));
        assert_eq!(p.get("b"), Some("23"));
        assert_eq!(p.get("c"), Some("\\"));
        assert_eq!(p.get("d"), Some("4"));
        assert!(Properties::load("a=\\u12G4").is_err());
        let p = Properties::load("s=\\uD83D\\uDE00").unwrap();
        assert_eq!(p.get("s"), Some("😀"));
    }

    #[test]
    fn store_escapes_and_sorts() {
        let mut p = Properties::new();
        p.put("b", " lead=x:y#z!w\\ q\t");
        p.put("a b", "é😀");
        p.put("Z", "");
        let text = p.store(Some("hello\nworld"));
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], "#hello");
        assert_eq!(lines[1], "#world");
        assert!(lines[2].starts_with('#'));
        assert_eq!(&lines[3..], &["Z=", "a\\ b=é😀", "b=\\ lead\\=x\\:y\\#z\\!w\\\\ q\\t"]);
        let latin = String::from_utf8(p.store_latin1(None)).unwrap();
        assert!(latin.contains("a\\ b=\\u00E9\\uD83D\\uDE00"));
        assert_eq!(Properties::load(&text).unwrap(), p);
    }

    #[test]
    fn date_format() {
        use chrono::TimeZone;
        let dt = chrono::FixedOffset::east_opt(3 * 3600).unwrap().with_ymd_and_hms(2026, 11, 4, 14, 13, 59).unwrap();
        assert_eq!(java_date_string_of(dt), "Wed Nov 04 14:13:59 GMT+03:00 2026");
        assert_eq!(java_date_string_of(chrono::Utc.with_ymd_and_hms(2026, 11, 4, 12, 0, 0).unwrap()), "Wed Nov 04 12:00:00 UTC 2026");
    }
}
