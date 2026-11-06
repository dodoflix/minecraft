//! Differential test of the streaming API against vanilla: `fixtures/stream_golden.tsv` is produced by
//! `fixtures/StreamOracle.java` (real `NbtIo.parse`, `Tag.acceptAsRoot`, `nbt.visitors`). Every cell of
//! a row is rebuilt here in the same order and compared as a whole string.

use mc_nbt::snbt::print_compact;
use mc_nbt::stream::{
    self, CollectFields, CollectToTag, EntryResult, FieldSelector, SkipAll, SkipFields,
    StreamTagVisitor, ValueResult,
};
use mc_nbt::{NbtAccounter, NbtError, Tag, nbt_io};

const QUOTA: i64 = 104_857_600;

fn unhex(h: &str) -> Vec<u8> {
    (0..h.len() / 2)
        .map(|i| u8::from_str_radix(&h[2 * i..2 * i + 2], 16).unwrap())
        .collect()
}

/// `StreamOracle.esc` (no lone surrogates occur in the fixture).
fn esc(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\x{:02X}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// `StreamOracle.status`: `<exception simple name>:<message>`.
fn status(e: &NbtError) -> String {
    esc(&match e {
        NbtError::Eof => return "EOF".into(),
        NbtError::InvalidTagId(id) => format!("IOException:Invalid tag id: {id}"),
        NbtError::Io(m) => format!("IOException:{m}"),
        NbtError::MalformedUtf(m) => format!("UTFDataFormatException:{m}"),
        NbtError::Accounter(m) => format!("NbtAccounterException:{m}"),
        NbtError::Format(m) => format!("NbtFormatException:{m}"),
        NbtError::IllegalArgument(m) => format!("IllegalArgumentException:{m}"),
        NbtError::Reported { .. } => panic!("parse never wraps errors: {e:?}"),
    })
}

fn tag_str(t: Option<&Tag>) -> String {
    t.map_or_else(|| "null".into(), |t| esc(&print_compact(t)))
}

fn run(b: &[u8], v: &mut dyn StreamTagVisitor, mut acc: NbtAccounter) -> String {
    let s = match stream::parse(&mut &*b, v, &mut acc) {
        Ok(()) => "OK".to_string(),
        Err(e) => status(&e),
    };
    format!("{s}|{}", acc.usage())
}

/// `StreamOracle.Trace`: hashes every visitor call and answers with a deterministic policy.
struct Trace {
    mode: usize,
    n: i64,
    count: usize,
    h: u64,
}

const POLICY: [[i64; 3]; 6] = [
    [0, 0, 0],
    [0, 0, 7],
    [0, 2, 0],
    [1, 0, 0],
    [1, 2, 7],
    [2, 3, 5],
];
const FNV_PRIME: u64 = 0x100000001b3;

impl Trace {
    fn new(mode: usize) -> Self {
        Self {
            mode,
            n: 0,
            count: 0,
            h: 0xcbf29ce484222325,
        }
    }

    fn log(&mut self, s: &str) {
        self.count += 1;
        for u in s.encode_utf16() {
            self.h = (self.h ^ u as u64).wrapping_mul(FNV_PRIME);
        }
        self.h = (self.h ^ b'\n' as u64).wrapping_mul(FNV_PRIME);
    }

    fn roll(&mut self) -> i64 {
        let x = (self.n + 1) * 2_654_435_761 + self.mode as i64 * 97;
        self.n += 1;
        ((x >> 8) & 0x7fff_ffff) % 23
    }

    fn value(&mut self) -> ValueResult {
        let r = self.roll();
        let p = POLICY[self.mode];
        if r < p[0] {
            ValueResult::Halt
        } else if r < p[0] + p[1] {
            ValueResult::Break
        } else {
            ValueResult::Continue
        }
    }

    fn entry(&mut self) -> EntryResult {
        let r = self.roll();
        let p = POLICY[self.mode];
        if r < p[0] {
            EntryResult::Halt
        } else if r < p[0] + p[1] {
            EntryResult::Break
        } else if r < p[0] + p[1] + p[2] {
            EntryResult::Skip
        } else {
            EntryResult::Enter
        }
    }

    fn cell(&self) -> String {
        format!("{}|{:x}", self.count, self.h)
    }

    fn array<T: std::fmt::Display>(&mut self, prefix: char, v: &[T]) -> ValueResult {
        let mut s = format!("{prefix}{}", v.len());
        for x in v {
            s.push_str(&format!(",{x}"));
        }
        self.log(&s);
        self.value()
    }
}

impl StreamTagVisitor for Trace {
    fn visit_end(&mut self) -> ValueResult {
        self.log("E");
        self.value()
    }
    fn visit_string(&mut self, v: &str) -> ValueResult {
        self.log(&format!("S{v}"));
        self.value()
    }
    fn visit_byte(&mut self, v: i8) -> ValueResult {
        self.log(&format!("B{v}"));
        self.value()
    }
    fn visit_short(&mut self, v: i16) -> ValueResult {
        self.log(&format!("H{v}"));
        self.value()
    }
    fn visit_int(&mut self, v: i32) -> ValueResult {
        self.log(&format!("I{v}"));
        self.value()
    }
    fn visit_long(&mut self, v: i64) -> ValueResult {
        self.log(&format!("J{v}"));
        self.value()
    }
    fn visit_float(&mut self, v: f32) -> ValueResult {
        let bits = if v.is_nan() { 0x7fc0_0000 } else { v.to_bits() } as i32;
        self.log(&format!("F{bits}"));
        self.value()
    }
    fn visit_double(&mut self, v: f64) -> ValueResult {
        let bits = if v.is_nan() {
            0x7ff8_0000_0000_0000
        } else {
            v.to_bits()
        } as i64;
        self.log(&format!("D{bits}"));
        self.value()
    }
    fn visit_byte_array(&mut self, v: &[i8]) -> ValueResult {
        self.array('A', v)
    }
    fn visit_int_array(&mut self, v: &[i32]) -> ValueResult {
        self.array('N', v)
    }
    fn visit_long_array(&mut self, v: &[i64]) -> ValueResult {
        self.array('O', v)
    }
    fn visit_list(&mut self, t: u8, size: i32) -> ValueResult {
        self.log(&format!("L{},{size}", t as i8));
        self.value()
    }
    fn visit_entry(&mut self, t: u8) -> EntryResult {
        self.log(&format!("e{}", t as i8));
        self.entry()
    }
    fn visit_entry_named(&mut self, t: u8, id: &str) -> EntryResult {
        self.log(&format!("k{}:{id}", t as i8));
        self.entry()
    }
    fn visit_element(&mut self, t: u8, index: i32) -> EntryResult {
        self.log(&format!("m{},{index}", t as i8));
        self.entry()
    }
    fn visit_container_end(&mut self) -> ValueResult {
        self.log("c");
        self.value()
    }
    fn visit_root_entry(&mut self, t: u8) -> ValueResult {
        self.log(&format!("r{}", t as i8));
        self.value()
    }
}

fn sel(type_id: u8, name: &str, path: &[&str]) -> FieldSelector {
    FieldSelector {
        path: path.iter().map(|s| s.to_string()).collect(),
        type_id,
        name: name.to_string(),
    }
}

/// `StreamOracle.SETS`.
fn sets() -> Vec<Vec<FieldSelector>> {
    vec![
        vec![],
        vec![sel(3, "a", &[])],
        vec![sel(8, "a", &[]), sel(3, "b", &[])],
        vec![sel(10, "a", &[])],
        vec![sel(3, "DataVersion", &[])],
        vec![sel(3, "b", &["a"])],
        vec![
            sel(3, "DataVersion", &["Data"]),
            sel(8, "name", &["Data", "version"]),
            sel(3, "DataVersion", &[]),
        ],
        vec![
            sel(9, "k1", &[]),
            sel(10, "k0", &[]),
            sel(3, "k2", &[]),
            sel(8, "k3", &[]),
        ],
        vec![sel(10, "k29", &["k28"]), sel(3, "k2", &["k0", "k1"])],
        vec![
            sel(1, "a", &[]),
            sel(2, "a", &[]),
            sel(4, "a", &[]),
            sel(6, "a", &[]),
            sel(5, "a", &[]),
            sel(7, "a", &[]),
            sel(11, "a", &[]),
            sel(12, "a", &[]),
            sel(9, "a", &[]),
            sel(8, "a", &[]),
        ],
        vec![sel(8, "facing", &["Properties"]), sel(8, "Name", &[])],
        vec![sel(3, "a", &[]), sel(3, "a", &[])],
        vec![sel(3, "x", &["a", "b", "c"])],
    ]
}

fn acc(quota: i64, depth: i32) -> NbtAccounter {
    NbtAccounter::new(quota, depth)
}

/// Mirrors `StreamOracle.cells`; labels are for failure messages only.
fn cells(b: &[u8]) -> Vec<(String, String)> {
    let mut o: Vec<(String, String)> = Vec::new();
    let mut push = |label: String, cell: String| o.push((label, cell));
    let sets = sets();

    let mut c = CollectToTag::new();
    let mut a = acc(QUOTA, 512);
    let r = run_with(b, &mut c, &mut a);
    push("collect".into(), format!("{r}|{}", tag_str(c.get_result())));
    let usage = a.usage();
    push(
        "collect quota/2".into(),
        run(b, &mut CollectToTag::new(), acc(usage / 2, 512)),
    );
    push(
        "collect depth 3".into(),
        run(b, &mut CollectToTag::new(), acc(QUOTA, 3)),
    );
    push("skip all".into(), run(b, &mut SkipAll, acc(QUOTA, 512)));
    for (i, set) in sets.iter().enumerate() {
        let mut cf = CollectFields::new(set);
        let rc = run(b, &mut cf, acc(QUOTA, 512));
        push(
            format!("collect_fields[{i}]"),
            format!(
                "{rc}|{}|{}",
                cf.get_missing_field_count(),
                tag_str(cf.get_result())
            ),
        );
        let mut sf = SkipFields::new(set);
        let rs = run(b, &mut sf, acc(QUOTA, 512));
        push(
            format!("skip_fields[{i}]"),
            format!("{rs}|{}", tag_str(sf.get_result())),
        );
    }
    for mode in 0..POLICY.len() {
        let mut t = Trace::new(mode);
        let r = run(b, &mut t, acc(QUOTA, 512));
        push(format!("trace parse[{mode}]"), format!("{r}|{}", t.cell()));
    }

    // ---- accept
    let tag = nbt_io::read_unnamed_tag(&mut &*b, &mut acc(QUOTA, 512)).ok();
    let Some(tag) = tag else {
        for i in 0..2 + 2 * sets.len() + POLICY.len() {
            push(format!("accept#{i}"), "-".into());
        }
        return o;
    };
    let mut c2 = CollectToTag::new();
    tag.accept_as_root(&mut c2);
    push("accept collect".into(), tag_str(c2.get_result()));
    tag.accept_as_root(&mut SkipAll);
    push("accept skip all".into(), "OK".into());
    for (i, set) in sets.iter().enumerate() {
        let mut cf = CollectFields::new(set);
        tag.accept_as_root(&mut cf);
        push(
            format!("accept collect_fields[{i}]"),
            format!(
                "{}|{}",
                cf.get_missing_field_count(),
                tag_str(cf.get_result())
            ),
        );
        let mut sf = SkipFields::new(set);
        tag.accept_as_root(&mut sf);
        push(format!("accept skip_fields[{i}]"), tag_str(sf.get_result()));
    }
    for mode in 0..POLICY.len() {
        let mut t = Trace::new(mode);
        tag.accept_as_root(&mut t);
        push(format!("trace accept[{mode}]"), t.cell());
    }
    o
}

/// `run` for a caller that needs the accounter afterwards; returns the status only.
fn run_with(b: &[u8], v: &mut dyn StreamTagVisitor, a: &mut NbtAccounter) -> String {
    let s = match stream::parse(&mut &*b, v, a) {
        Ok(()) => "OK".to_string(),
        Err(e) => status(&e),
    };
    format!("{s}|{}", a.usage())
}

#[test]
fn stream_matches_vanilla() {
    let data = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/stream_golden.tsv"
    ))
    .unwrap();
    let mut problems = Vec::new();
    let mut rows = 0;
    for line in data.lines() {
        let f: Vec<&str> = line.split('\t').collect();
        assert_eq!(f[0], "P");
        let bytes = unhex(f[1]);
        let got = cells(&bytes);
        rows += 1;
        if got.len() != f.len() - 2 {
            problems.push(format!(
                "{}: {} cells, want {}",
                f[1],
                got.len(),
                f.len() - 2
            ));
            continue;
        }
        for ((label, cell), want) in got.iter().zip(&f[2..]) {
            if cell != want {
                problems.push(format!("{} [{label}]:\n  got  {cell}\n  want {want}", f[1]));
            }
        }
    }
    assert!(rows > 400, "fixture too small: {rows} rows");
    assert!(
        problems.is_empty(),
        "{} mismatches in {rows} rows, first:\n{}",
        problems.len(),
        problems
            .iter()
            .take(8)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// Parsing with `CollectToTag` yields the tag `read_unnamed_tag` loads, and `accept` re-collects it.
#[test]
fn collect_roundtrips_loaded_tags() {
    let data = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/stream_golden.tsv"
    ))
    .unwrap();
    let mut checked = 0;
    for line in data.lines() {
        let bytes = unhex(line.split('\t').nth(1).unwrap());
        let Ok(loaded) = nbt_io::read_unnamed_tag(&mut &*bytes, &mut acc(QUOTA, 512)) else {
            continue;
        };
        let mut c = CollectToTag::new();
        stream::parse(&mut &*bytes, &mut c, &mut acc(QUOTA, 512)).unwrap();
        assert_eq!(c.get_result(), Some(&loaded), "parse {line}");
        let mut c = CollectToTag::new();
        loaded.accept_as_root(&mut c);
        assert_eq!(c.into_result(), Some(loaded), "accept {line}");
        checked += 1;
    }
    assert!(checked > 100, "{checked}");
}

#[test]
fn parse_compressed_matches_parse() {
    let tag: mc_nbt::CompoundTag = (0..20).map(|i| (format!("k{i}"), i)).collect();
    let gz = nbt_io::write_compressed(&tag);
    let mut c = CollectToTag::new();
    stream::parse_compressed(&gz, &mut c, &mut NbtAccounter::default_quota()).unwrap();
    assert_eq!(c.into_result(), Some(Tag::Compound(tag)));
}
