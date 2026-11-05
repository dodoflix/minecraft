//! Packet log recorder (every packet in/out with timestamp and decoded debug form) and a structural
//! diff of two logs (packet order via LCS on `(direction, name)`, optional per-packet field comparison).
use std::collections::HashMap;
use std::fmt::Write as _;
use std::io::Write as _;
use std::path::Path;
use std::time::Instant;

use serde::{Deserialize, Serialize};

/// Packet direction from the bot's point of view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Dir {
    /// Server → bot (`clientbound`).
    In,
    /// Bot → server (`serverbound`).
    Out,
}

impl Dir {
    pub fn arrow(self) -> &'static str {
        match self {
            Dir::In => "<-",
            Dir::Out => "->",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogEntry {
    pub seq: u64,
    /// Milliseconds since the connection was opened.
    pub t_ms: u64,
    pub dir: Dir,
    /// Connection state id (`login`, `configuration`, `play`, …).
    pub state: String,
    pub id: i32,
    /// Resource name without namespace, e.g. `keep_alive`; `?id` for unknown packets.
    pub name: String,
    /// `Debug` form of the decoded packet (possibly abbreviated, see [`abbreviate`]).
    pub debug: String,
    /// Wire size of the payload (id + body).
    pub len: usize,
    /// Hex of the raw payload, present only when raw recording is on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw: Option<String>,
}

impl LogEntry {
    pub fn key(&self) -> (Dir, &str, &str) {
        (self.dir, &self.state, &self.name)
    }

    pub fn line(&self) -> String {
        format!("{:>7}ms {} {}/{} {}", self.t_ms, self.dir.arrow(), self.state, self.name, self.debug)
    }
}

/// Max length of the stored debug string; the middle of longer strings is replaced by `…(+N chars)…`.
pub const DEBUG_LIMIT: usize = 1200;

/// Shorten a huge `Debug` dump (chunk data, tag payloads) keeping head and tail.
pub fn abbreviate(s: String) -> String {
    let n = s.chars().count();
    if n <= DEBUG_LIMIT {
        return s;
    }
    let head: String = s.chars().take(DEBUG_LIMIT * 2 / 3).collect();
    let tail: String = s.chars().skip(n - DEBUG_LIMIT / 3).collect();
    format!("{head}…(+{} chars)…{tail}", n - DEBUG_LIMIT)
}

#[derive(Debug)]
pub struct PacketLog {
    start: Instant,
    entries: Vec<LogEntry>,
    pub record_raw: bool,
}

impl Default for PacketLog {
    fn default() -> Self {
        Self::new()
    }
}

impl PacketLog {
    pub fn new() -> Self {
        Self { start: Instant::now(), entries: Vec::new(), record_raw: false }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn record(&mut self, dir: Dir, state: &str, id: i32, name: &str, debug: String, payload: &[u8]) {
        let raw = self.record_raw.then(|| hex(payload));
        self.entries.push(LogEntry {
            seq: self.entries.len() as u64,
            t_ms: self.start.elapsed().as_millis() as u64,
            dir,
            state: state.to_string(),
            id,
            name: name.to_string(),
            debug: abbreviate(debug),
            len: payload.len(),
            raw,
        });
    }

    pub fn entries(&self) -> &[LogEntry] {
        &self.entries
    }

    /// Entries of one direction/name (`name` without namespace).
    pub fn find<'a>(&'a self, dir: Dir, name: &'a str) -> impl Iterator<Item = &'a LogEntry> {
        self.entries.iter().filter(move |e| e.dir == dir && e.name == name)
    }

    /// One JSON object per line.
    pub fn write_jsonl(&self, path: impl AsRef<Path>) -> std::io::Result<()> {
        let mut f = std::io::BufWriter::new(std::fs::File::create(path)?);
        for e in &self.entries {
            serde_json::to_writer(&mut f, e).map_err(std::io::Error::other)?;
            f.write_all(b"\n")?;
        }
        f.flush()
    }

    /// Human readable text, one packet per line.
    pub fn to_text(&self) -> String {
        let mut s = String::new();
        for e in &self.entries {
            let _ = writeln!(s, "{}", e.line());
        }
        s
    }
}

pub fn read_jsonl(path: impl AsRef<Path>) -> std::io::Result<Vec<LogEntry>> {
    std::fs::read_to_string(path)?
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).map_err(std::io::Error::other))
        .collect()
}

pub fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// Value of top-level-ish `field: value` inside a derived `Debug` string (first occurrence, nested
/// brackets/strings respected). Returns `None` if the field is absent.
pub fn extract_field<'a>(debug: &'a str, field: &str) -> Option<&'a str> {
    let needle = format!("{field}: ");
    let mut from = 0;
    let start = loop {
        let i = debug[from..].find(&needle)? + from;
        let boundary = i == 0 || !debug.as_bytes()[i - 1].is_ascii_alphanumeric() && debug.as_bytes()[i - 1] != b'_';
        if boundary {
            break i + needle.len();
        }
        from = i + 1;
    };
    let rest = &debug[start..];
    let (mut depth, mut in_str, mut esc) = (0i32, false, false);
    for (i, c) in rest.char_indices() {
        if in_str {
            match (esc, c) {
                (true, _) => esc = false,
                (false, '\\') => esc = true,
                (false, '"') => in_str = false,
                _ => {}
            }
            continue;
        }
        match c {
            '"' => in_str = true,
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' if depth == 0 => return Some(rest[..i].trim_end()),
            ')' | ']' | '}' => depth -= 1,
            ',' if depth == 0 => return Some(rest[..i].trim_end()),
            _ => {}
        }
    }
    Some(rest.trim_end())
}

#[derive(Default)]
pub struct DiffOptions {
    /// Packets whose name contains one of these substrings are dropped from both logs first
    /// (e.g. `keep_alive`, `set_time`, timing dependent traffic).
    pub ignore: Vec<String>,
    /// Compare complete debug strings of aligned packets (otherwise only the listed `fields`).
    pub compare_all_fields: bool,
    /// Packet name → fields to compare for aligned packets of that name.
    pub fields: HashMap<String, Vec<String>>,
    /// Restrict to one direction.
    pub dir: Option<Dir>,
    /// Restrict to one connection state.
    pub state: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffOp {
    /// Aligned packets that agree (indices into the filtered logs).
    Same(usize, usize),
    /// Aligned packets with different compared fields: `(i, j, field-or-"*", a, b)`.
    Changed(usize, usize, String, String, String),
    OnlyA(usize),
    OnlyB(usize),
}

#[derive(Debug)]
pub struct LogDiff {
    pub a: Vec<LogEntry>,
    pub b: Vec<LogEntry>,
    pub ops: Vec<DiffOp>,
}

impl LogDiff {
    pub fn is_empty(&self) -> bool {
        self.ops.iter().all(|o| matches!(o, DiffOp::Same(..)))
    }

    pub fn render(&self) -> String {
        let mut s = String::new();
        for op in &self.ops {
            match op {
                DiffOp::Same(..) => {}
                DiffOp::OnlyA(i) => {
                    let _ = writeln!(s, "- only in A: {}", self.a[*i].line());
                }
                DiffOp::OnlyB(j) => {
                    let _ = writeln!(s, "+ only in B: {}", self.b[*j].line());
                }
                DiffOp::Changed(i, _j, field, x, y) => {
                    let _ = writeln!(
                        s,
                        "~ {} {}/{} [{field}]\n    A: {x}\n    B: {y}",
                        self.a[*i].dir.arrow(),
                        self.a[*i].state,
                        self.a[*i].name
                    );
                }
            }
        }
        s
    }
}

/// Structural diff: filter, align by LCS on `(dir, state, name)`, compare fields of aligned packets.
pub fn diff_logs(a: &[LogEntry], b: &[LogEntry], opts: &DiffOptions) -> LogDiff {
    let keep = |e: &&LogEntry| {
        opts.dir.is_none_or(|d| e.dir == d)
            && opts.state.as_ref().is_none_or(|s| &e.state == s)
            && !opts.ignore.iter().any(|i| e.name.contains(i.as_str()))
    };
    let a: Vec<LogEntry> = a.iter().filter(keep).cloned().collect();
    let b: Vec<LogEntry> = b.iter().filter(keep).cloned().collect();

    let pre = a.iter().zip(&b).take_while(|(x, y)| x.key() == y.key()).count();
    let suf = a[pre..].iter().rev().zip(b[pre..].iter().rev()).take_while(|(x, y)| x.key() == y.key()).count();
    let (am, bm) = (&a[pre..a.len() - suf], &b[pre..b.len() - suf]);

    // LCS table over the differing middle.
    let w = bm.len() + 1;
    let mut t = vec![0u32; (am.len() + 1) * w];
    for i in (0..am.len()).rev() {
        for j in (0..bm.len()).rev() {
            t[i * w + j] = if am[i].key() == bm[j].key() {
                t[(i + 1) * w + j + 1] + 1
            } else {
                t[(i + 1) * w + j].max(t[i * w + j + 1])
            };
        }
    }
    let mut pairs: Vec<(Option<usize>, Option<usize>)> = (0..pre).map(|i| (Some(i), Some(i))).collect();
    let (mut i, mut j) = (0, 0);
    while i < am.len() || j < bm.len() {
        if i < am.len() && j < bm.len() && am[i].key() == bm[j].key() {
            pairs.push((Some(pre + i), Some(pre + j)));
            i += 1;
            j += 1;
        } else if j == bm.len() || (i < am.len() && t[(i + 1) * w + j] >= t[i * w + j + 1]) {
            pairs.push((Some(pre + i), None));
            i += 1;
        } else {
            pairs.push((None, Some(pre + j)));
            j += 1;
        }
    }
    pairs.extend((0..suf).map(|k| (Some(a.len() - suf + k), Some(b.len() - suf + k))));

    let ops = pairs
        .into_iter()
        .flat_map(|p| match p {
            (Some(i), None) => vec![DiffOp::OnlyA(i)],
            (None, Some(j)) => vec![DiffOp::OnlyB(j)],
            (Some(i), Some(j)) => compare(&a[i], &b[j], i, j, opts),
            (None, None) => vec![],
        })
        .collect();
    LogDiff { a, b, ops }
}

fn compare(x: &LogEntry, y: &LogEntry, i: usize, j: usize, opts: &DiffOptions) -> Vec<DiffOp> {
    if opts.compare_all_fields {
        return if x.debug == y.debug {
            vec![DiffOp::Same(i, j)]
        } else {
            vec![DiffOp::Changed(i, j, "*".into(), x.debug.clone(), y.debug.clone())]
        };
    }
    let Some(fields) = opts.fields.get(&x.name) else { return vec![DiffOp::Same(i, j)] };
    let diffs: Vec<DiffOp> = fields
        .iter()
        .filter_map(|f| {
            let (fx, fy) = (extract_field(&x.debug, f), extract_field(&y.debug, f));
            (fx != fy).then(|| {
                DiffOp::Changed(i, j, f.clone(), fx.unwrap_or("<absent>").into(), fy.unwrap_or("<absent>").into())
            })
        })
        .collect();
    if diffs.is_empty() { vec![DiffOp::Same(i, j)] } else { diffs }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(dir: Dir, name: &str, debug: &str) -> LogEntry {
        LogEntry { seq: 0, t_ms: 0, dir, state: "play".into(), id: 0, name: name.into(), debug: debug.into(), len: 0, raw: None }
    }

    #[test]
    fn extracts_nested_fields() {
        let d = r#"Login { player_id: 5, name: "a, b", pos: (1, 2), list: [1, 2], last: true }"#;
        assert_eq!(extract_field(d, "player_id"), Some("5"));
        assert_eq!(extract_field(d, "name"), Some(r#""a, b""#));
        assert_eq!(extract_field(d, "pos"), Some("(1, 2)"));
        assert_eq!(extract_field(d, "list"), Some("[1, 2]"));
        assert_eq!(extract_field(d, "last"), Some("true"));
        assert_eq!(extract_field(d, "id"), None);
    }

    #[test]
    fn diff_aligns_order_and_fields() {
        use Dir::*;
        let a = vec![e(In, "login", "L { id: 1 }"), e(In, "keep_alive", "K"), e(In, "set_time", "T { t: 1 }"), e(Out, "pong", "P")];
        let b = vec![e(In, "login", "L { id: 2 }"), e(In, "set_time", "T { t: 1 }"), e(In, "extra", "X"), e(Out, "pong", "P")];
        let mut opts = DiffOptions { ignore: vec!["keep_alive".into()], ..Default::default() };
        opts.fields.insert("login".into(), vec!["id".into()]);
        let d = diff_logs(&a, &b, &opts);
        assert_eq!(
            d.ops,
            vec![
                DiffOp::Changed(0, 0, "id".into(), "1".into(), "2".into()),
                DiffOp::Same(1, 1),
                DiffOp::OnlyB(2),
                DiffOp::Same(2, 3),
            ]
        );
        assert!(!d.is_empty());
        assert!(d.render().contains("only in B"));
    }

    #[test]
    fn abbreviates_long_debug() {
        let s = abbreviate("x".repeat(5000));
        assert!(s.chars().count() < DEBUG_LIMIT + 40 && s.contains("+3800 chars"));
    }
}
