//! `--ignore-field` specs: which files (or which fields inside decoded files) are left out of an exact comparison.
//!
//! A spec is `GLOB` or `GLOB#FIELD.PATH`.
//! * `GLOB` matches a path relative to the world directory, `/`-separated: `*` matches within one path segment, `**`
//!   across segments; a trailing `/` matches the directory and everything below it. Without `#` the whole entry is
//!   skipped: `session.lock`, `logs/`, `**/*.lock`.
//! * `#FIELD.PATH` removes a field from the decoded content of every matching file (NBT: `.dat`/`.nbt` files and
//!   every chunk of a `.mca` file; JSON: `.json`) on both sides before comparing: `level.dat#Data.LastPlayed`,
//!   `data/*.dat#data.*.ticks`. Segments are compound/object keys; `*` matches every key of a compound/object or
//!   every element of a list/array. Keys containing `.` cannot be addressed (shortcut: split on every dot, add an
//!   escape when such a key has to be ignored).

use std::str::FromStr;

use mc_nbt::{CompoundTag, Tag};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldSpec {
    glob: String,
    field: Option<Vec<String>>,
}

impl FromStr for FieldSpec {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        let (glob, field) = match s.split_once('#') {
            Some((g, f)) => (g, Some(f.split('.').map(str::to_owned).collect::<Vec<_>>())),
            None => (s, None),
        };
        if glob.is_empty()
            || field
                .as_ref()
                .is_some_and(|f| f.iter().any(String::is_empty))
        {
            return Err(format!(
                "bad --ignore-field spec {s:?} (GLOB or GLOB#A.B.C)"
            ));
        }
        Ok(Self {
            glob: glob.to_owned(),
            field,
        })
    }
}

impl FieldSpec {
    fn matches(&self, rel: &str) -> bool {
        if self.glob.ends_with('/') {
            glob_match(format!("{}**", self.glob).as_bytes(), rel.as_bytes())
        } else {
            glob_match(self.glob.as_bytes(), rel.as_bytes())
        }
    }
}

fn glob_match(p: &[u8], t: &[u8]) -> bool {
    match p {
        [] => t.is_empty(),
        [b'*', b'*', rest @ ..] => (0..=t.len()).any(|i| glob_match(rest, &t[i..])),
        [b'*', rest @ ..] => (0..=t.len())
            .take_while(|&i| i == 0 || t[i - 1] != b'/')
            .any(|i| glob_match(rest, &t[i..])),
        [c, rest @ ..] => t.first() == Some(c) && glob_match(rest, &t[1..]),
    }
}

#[derive(Clone, Debug, Default)]
pub struct Ignores(pub Vec<FieldSpec>);

impl Ignores {
    /// True if a spec without a field path matches `rel`.
    pub fn whole(&self, rel: &str) -> bool {
        self.0.iter().any(|s| s.field.is_none() && s.matches(rel))
    }

    /// True if a field spec matches `rel`, so decoded content may have been changed before comparing.
    pub fn touches(&self, rel: &str) -> bool {
        self.paths(rel).next().is_some()
    }

    fn paths<'a>(&'a self, rel: &'a str) -> impl Iterator<Item = &'a [String]> {
        self.0
            .iter()
            .filter(move |s| s.matches(rel))
            .filter_map(|s| s.field.as_deref())
    }

    pub fn strip_nbt(&self, rel: &str, tag: &mut CompoundTag) {
        for p in self.paths(rel) {
            strip_compound(tag, p);
        }
    }

    pub fn strip_json(&self, rel: &str, value: &mut Value) {
        for p in self.paths(rel) {
            strip_json(value, p);
        }
    }
}

fn strip_compound(c: &mut CompoundTag, path: &[String]) {
    let Some((first, rest)) = path.split_first() else {
        return;
    };
    match (first.as_str(), rest.is_empty()) {
        ("*", true) => *c = CompoundTag::new(),
        (name, true) => {
            c.remove(name);
        }
        ("*", false) => c.values_mut().for_each(|v| strip_tag(v, rest)),
        (name, false) => {
            if let Some(v) = c.get_mut(name) {
                strip_tag(v, rest);
            }
        }
    }
}

fn strip_tag(tag: &mut Tag, path: &[String]) {
    match tag {
        Tag::Compound(c) => strip_compound(c, path),
        Tag::List(l) if path[0] == "*" => {
            if path.len() == 1 {
                l.clear();
            }
            for i in 0..l.len() {
                if path.len() > 1
                    && let Some(e) = l.get_mut(i)
                {
                    strip_tag(e, &path[1..]);
                }
            }
        }
        _ => {}
    }
}

fn strip_json(v: &mut Value, path: &[String]) {
    let Some((first, rest)) = path.split_first() else {
        return;
    };
    match v {
        Value::Object(m) => match (first.as_str(), rest.is_empty()) {
            ("*", true) => m.clear(),
            (name, true) => {
                m.shift_remove(name);
            }
            ("*", false) => m.values_mut().for_each(|e| strip_json(e, rest)),
            (name, false) => {
                if let Some(e) = m.get_mut(name) {
                    strip_json(e, rest);
                }
            }
        },
        Value::Array(a) if first == "*" => {
            if rest.is_empty() {
                a.clear();
            } else {
                a.iter_mut().for_each(|e| strip_json(e, rest));
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn globs() {
        let ig = |s: &str| Ignores(vec![s.parse().unwrap()]);
        assert!(ig("session.lock").whole("session.lock"));
        assert!(!ig("session.lock").whole("x/session.lock"));
        assert!(ig("**/session.lock").whole("x/y/session.lock"));
        assert!(ig("logs/").whole("logs/latest.log") && ig("logs/").whole("logs/"));
        assert!(!ig("logs/").whole("logs.txt"));
        assert!(ig("data/*.dat").whole("data/a.dat") && !ig("data/*.dat").whole("data/x/a.dat"));
        assert!(!ig("level.dat#Data.Time").whole("level.dat"));
        assert!("".parse::<FieldSpec>().is_err() && "a#b..c".parse::<FieldSpec>().is_err());
    }

    #[test]
    fn strips_nbt_and_json_fields() {
        let mut t =
            mc_nbt::snbt::parse_compound("{Data:{Time:1L,A:{x:1,y:2},L:[{x:1,y:2},{x:3}]}}")
                .unwrap();
        let ig = Ignores(
            [
                "level.dat#Data.Time",
                "*.dat#Data.A.*",
                "level.dat#Data.L.*.x",
            ]
            .iter()
            .map(|s| s.parse().unwrap())
            .collect(),
        );
        ig.strip_nbt("level.dat", &mut t);
        let want = mc_nbt::snbt::parse_compound("{Data:{A:{},L:[{y:2},{}]}}").unwrap();
        assert_eq!(t, want);
        let mut j: Value = serde_json::from_str(r#"{"a":[{"t":1,"u":2}],"b":1}"#).unwrap();
        Ignores(vec!["x.json#a.*.t".parse().unwrap()]).strip_json("x.json", &mut j);
        assert_eq!(j, serde_json::json!({"a":[{"u":2}],"b":1}));
    }
}
