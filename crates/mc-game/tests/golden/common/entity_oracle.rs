//! Reader/comparator of the entity-oracle fixtures (producer: `tests/oracle/EntityOracleDriver.java`, run through
//! `scripts/entity-oracle.sh`; scenarios in `tests/oracle/scenarios/NAME.txt`, committed output in
//! `tests/fixtures/entity_oracle/NAME.txt`).
//!
//! Typical golden test:
//! ```ignore
//! mod common; use common::entity_oracle::*;
//! let fixture = Fixture::load("fall");                      // tests/fixtures/entity_oracle/fall.txt
//! let mut actual = Vec::new();
//! /* replay the same scenario with the Rust code under test; after every tick push the lines the Java driver would print:
//!    actual.push("T 1".into()); actual.push(format!("P Steve minecraft:player pos={},{},{} ...", x, y, z)); */
//! fixture.assert_matches(&actual, &Options::default());     // panics with the first differences + context
//! // or just the ticks / a single entity:
//! let steve: Vec<Line> = fixture.entity_lines("Steve");
//! ```
//! Line grammar (see the driver header): `T n`, `P name type k=v ...`, `E name type k=v ...`, `I player slot item`,
//! `PKT player type hex`, `B x y z state`, `C text`, `HURT ...`, `DIG ...`, `EFFECT ...`, `# note`.
//! Comparison is token based: a line is split at whitespace and at `, = ; / * :`; two tokens match when they are equal text, or
//! both parse as `f64` (Java `Double/Float.toString`, Rust `{}`/`{:?}` of f32/f64 both parse) and are bit identical
//! (`NaN` equals `NaN`, `-0.0 != 0.0`) or, with [`Options::float_tolerance`], within that absolute difference.
//! Write `f32` fields with `{}` of the f32 (shortest round trip), `f64` with `{}`/`{:?}` of the f64, exactly as the driver prints
//! the Java `float`/`double`.
#![allow(
    dead_code,
    reason = "each test target uses a subset of the shared helpers"
)]

use std::collections::BTreeMap;
use std::path::PathBuf;

/// Directory of the committed fixtures.
pub fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../mc-game/tests/fixtures/entity_oracle")
}

/// Directory of the scenario scripts (inputs of the Java driver; the Rust replay of a scenario may parse the same text).
pub fn scenario_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../mc-game/tests/oracle/scenarios")
}

#[derive(Debug, Clone, Default)]
pub struct Options {
    /// Allowed absolute difference of numeric tokens (None = bit exact, the default).
    pub float_tolerance: Option<f64>,
    /// Line kinds (first token, e.g. `"PKT"`, `"C"`, `"#"`) skipped on both sides before comparing.
    pub ignore_kinds: Vec<&'static str>,
    /// Field keys (`"fire"`, `"hurt"`, ...) of `P`/`E` lines ignored on both sides (for fields the Rust side does not model yet).
    pub ignore_fields: Vec<&'static str>,
}

/// A parsed `P`/`E`/`I`/... line: `kind name rest...` with `k=v` pairs of the rest in `fields`.
#[derive(Debug, Clone)]
pub struct Line {
    pub kind: String,
    pub name: String,
    pub rest: Vec<String>,
    pub fields: BTreeMap<String, String>,
}

impl Line {
    pub fn parse(text: &str) -> Line {
        let mut it = text.split_whitespace();
        let kind = it.next().unwrap_or("").to_string();
        let name = it.next().unwrap_or("").to_string();
        let rest: Vec<String> = it.map(str::to_string).collect();
        let fields = rest
            .iter()
            .filter_map(|t| {
                t.split_once('=')
                    .map(|(k, v)| (k.to_string(), v.to_string()))
            })
            .collect();
        Line {
            kind,
            name,
            rest,
            fields,
        }
    }

    /// `f64` of `key=value` (first number if the value is a `,` list).
    pub fn f64(&self, key: &str) -> f64 {
        self.vec(key)[0]
    }

    /// Comma separated numbers of `key=a,b,c`.
    pub fn vec(&self, key: &str) -> Vec<f64> {
        self.fields
            .get(key)
            .unwrap_or_else(|| panic!("line `{} {}` has no field {key}", self.kind, self.name))
            .split(',')
            .map(|s| {
                s.parse::<f64>()
                    .unwrap_or_else(|_| panic!("field {key}={s} is not a number"))
            })
            .collect()
    }

    pub fn str(&self, key: &str) -> &str {
        self.fields
            .get(key)
            .unwrap_or_else(|| panic!("line `{} {}` has no field {key}", self.kind, self.name))
    }
}

/// One `T n` block: the tick counter and the lines printed after that tick.
#[derive(Debug, Clone)]
pub struct TickBlock {
    pub tick: u64,
    pub lines: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Fixture {
    pub name: String,
    pub lines: Vec<String>,
}

impl Fixture {
    pub fn load(name: &str) -> Fixture {
        let path = fixture_dir().join(format!("{name}.txt"));
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!(
                "fixture {}: {e} (regenerate with scripts/entity-oracle.sh --regen)",
                path.display()
            )
        });
        Fixture::from_text(name, &text)
    }

    pub fn from_text(name: &str, text: &str) -> Fixture {
        Fixture {
            name: name.to_string(),
            lines: text.lines().map(str::to_string).collect(),
        }
    }

    /// Lines before the first `T n` header (the `dump` output printed before any tick of a `watch`).
    pub fn preamble(&self) -> Vec<String> {
        self.lines
            .iter()
            .take_while(|l| !l.starts_with("T "))
            .cloned()
            .collect()
    }

    /// Per-tick blocks (`T n` header + following lines up to the next header).
    pub fn ticks(&self) -> Vec<TickBlock> {
        let mut out: Vec<TickBlock> = Vec::new();
        for l in &self.lines {
            if let Some(n) = l.strip_prefix("T ") {
                out.push(TickBlock {
                    tick: n.trim().parse().expect("tick number"),
                    lines: Vec::new(),
                });
            } else if let Some(b) = out.last_mut() {
                b.lines.push(l.clone());
            }
        }
        out
    }

    /// All `P`/`E` lines of the entity `name` in file order (preamble and ticks).
    pub fn entity_lines(&self, name: &str) -> Vec<Line> {
        self.lines
            .iter()
            .filter(|l| l.starts_with("P ") || l.starts_with("E "))
            .map(|l| Line::parse(l))
            .filter(|l| l.name == name)
            .collect()
    }

    pub fn assert_matches(&self, actual: &[String], opts: &Options) {
        if let Err(report) = compare(&self.lines, actual, opts) {
            panic!("entity oracle fixture `{}` differs:\n{report}", self.name);
        }
    }
}

fn keep(line: &str, opts: &Options) -> bool {
    let kind = line.split_whitespace().next().unwrap_or("");
    !opts.ignore_kinds.contains(&kind)
}

fn normalize(line: &str, opts: &Options) -> String {
    if opts.ignore_fields.is_empty() {
        return line.to_string();
    }
    line.split(' ')
        .filter(|t| {
            t.split_once('=')
                .is_none_or(|(k, _)| !opts.ignore_fields.contains(&k))
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn tokens(line: &str) -> Vec<&str> {
    line.split(|c: char| c.is_whitespace() || ",=;/*:".contains(c))
        .filter(|t| !t.is_empty())
        .collect()
}

fn token_eq(a: &str, b: &str, tol: Option<f64>) -> bool {
    if a == b {
        return true;
    }
    match (a.parse::<f64>(), b.parse::<f64>()) {
        (Ok(x), Ok(y)) => {
            if x.is_nan() && y.is_nan() {
                return true;
            }
            match tol {
                Some(t) => (x - y).abs() <= t,
                None => x.to_bits() == y.to_bits(),
            }
        }
        _ => false,
    }
}

/// Do two fixture lines match under `opts` (token based, see the module doc)?
pub fn lines_match(expected: &str, actual: &str, opts: &Options) -> bool {
    let (e, a) = (normalize(expected, opts), normalize(actual, opts));
    let (te, ta) = (tokens(&e), tokens(&a));
    te.len() == ta.len()
        && te
            .iter()
            .zip(&ta)
            .all(|(x, y)| token_eq(x, y, opts.float_tolerance))
}

/// Compare two line lists; `Err` carries a readable report of the first differences with the surrounding lines.
pub fn compare(expected: &[String], actual: &[String], opts: &Options) -> Result<(), String> {
    let e: Vec<&String> = expected.iter().filter(|l| keep(l, opts)).collect();
    let a: Vec<&String> = actual.iter().filter(|l| keep(l, opts)).collect();
    let mut report = String::new();
    let mut shown = 0;
    for i in 0..e.len().max(a.len()) {
        let ok = match (e.get(i), a.get(i)) {
            (Some(x), Some(y)) => lines_match(x, y, opts),
            _ => false,
        };
        if !ok {
            if shown == 0 {
                for j in i.saturating_sub(3)..i {
                    report.push_str(&format!("      {j:>5}: {}\n", e[j]));
                }
            }
            report.push_str(&format!(
                "  exp {i:>5}: {}\n",
                e.get(i).map_or("<missing>", |s| s.as_str())
            ));
            report.push_str(&format!(
                "  act {i:>5}: {}\n",
                a.get(i).map_or("<missing>", |s| s.as_str())
            ));
            shown += 1;
            if shown >= 5 {
                report.push_str("  ... (more differences)\n");
                break;
            }
        }
    }
    if shown == 0 && e.len() == a.len() {
        Ok(())
    } else {
        if shown == 0 {
            report.push_str(&format!(
                "  line count differs: expected {}, actual {}\n",
                e.len(),
                a.len()
            ));
        }
        Err(report)
    }
}
