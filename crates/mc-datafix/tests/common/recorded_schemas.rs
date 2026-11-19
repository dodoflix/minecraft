//! The recorded schema types (`fixtures/schema_types.tsv.gz`, written by `scripts/datafix-oracle.sh --regen` from the real
//! `DataFixerUpper`) and the check of one of our schemas against them.
//!
//! The recording is delta encoded: one line `KEY<TAB>*<TAB>-` per schema and, where the sha256 of the normalised type
//! description (`mc_datafix::Type::description`, grammar in `SchemaWalk.java`) changes, `KEY<TAB>TYPE<TAB>HASH`; hash `-`
//! means the type is gone from this schema on.

use mc_datafix::{References, Schema};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub struct Recorded {
    /// Every schema key, ascending, each with the type hashes in effect from it on.
    schemas: BTreeMap<i32, BTreeMap<String, String>>,
}

impl Recorded {
    pub fn parse(text: &str) -> Recorded {
        let mut schemas = BTreeMap::new();
        let mut current: BTreeMap<String, String> = BTreeMap::new();
        let mut key = None;
        let mut flush = |key: Option<i32>, current: &BTreeMap<String, String>| {
            if let Some(k) = key {
                schemas.insert(k, current.clone());
            }
        };
        for line in text.lines() {
            let f: Vec<&str> = line.split('\t').collect();
            assert_eq!(f.len(), 3, "malformed schema_types line: {line}");
            let k: i32 = f[0].parse().expect("schema key");
            if f[1] == "*" {
                flush(key, &current);
                key = Some(k);
            } else if f[2] == "-" {
                current.remove(f[1]);
            } else {
                current.insert(f[1].to_string(), f[2].to_string());
            }
        }
        flush(key, &current);
        Recorded { schemas }
    }

    pub fn keys(&self) -> BTreeSet<i32> {
        self.schemas.keys().copied().collect()
    }

    pub fn types(&self, key: i32) -> Option<&BTreeMap<String, String>> {
        self.schemas.get(&key)
    }
}

#[derive(Debug)]
pub struct Difference {
    pub key: i32,
    pub ty: String,
    pub what: String,
}

impl std::fmt::Display for Difference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "schema {} type {}: {}", self.key, self.ty, self.what)
    }
}

pub fn hash_description(description: &str) -> String {
    Sha256::digest(description.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Our description of `ty` in `schema` (`Schema.getType`), `None` for a name that is not a `References` type.
pub fn description(schema: &Schema, ty: &str) -> Option<String> {
    References::by_name(ty).map(|r| schema.get_type(&r).description())
}

/// Differences between `schema` and the recording of its key: types we lack or add, and types whose description hashes
/// differently.
pub fn check_schema(recorded: &Recorded, schema: &Schema) -> Vec<Difference> {
    let key = schema.version_key();
    let Some(expected) = recorded.types(key) else {
        return vec![Difference {
            key,
            ty: "*".into(),
            what: "the recording has no schema with this key".into(),
        }];
    };
    let mut ours = schema.types();
    ours.sort();
    let mut out = Vec::new();
    for ty in expected.keys().filter(|t| !ours.contains(t)) {
        out.push(Difference {
            key,
            ty: ty.clone(),
            what: "registered by vanilla, not by us".into(),
        });
    }
    for ty in &ours {
        let Some(want) = expected.get(ty) else {
            out.push(Difference {
                key,
                ty: ty.clone(),
                what: "registered by us, not by vanilla".into(),
            });
            continue;
        };
        match description(schema, ty) {
            None => out.push(Difference {
                key,
                ty: ty.clone(),
                what: "not a References type".into(),
            }),
            Some(d) => {
                let got = hash_description(&d);
                if &got != want {
                    out.push(Difference {
                        key,
                        ty: ty.clone(),
                        what: format!(
                            "description hash {got} differs from vanilla's {want}; compare with `scripts/datafix-oracle.sh schema-dump {key} {ty}` (the oracle test `registered_schemas_match_live_descriptions` prints the first differing node)"
                        ),
                    });
                }
            }
        }
    }
    out
}

/// The first line where two descriptions differ, with its number and both versions.
pub fn first_difference(vanilla: &str, ours: &str) -> Option<String> {
    let (mut v, mut o) = (vanilla.lines(), ours.lines());
    let mut n = 0;
    loop {
        n += 1;
        match (v.next(), o.next()) {
            (None, None) => return None,
            (a, b) if a == b => {}
            (a, b) => {
                return Some(format!(
                    "node {n}: vanilla `{}`, ours `{}`",
                    a.map(str::trim).unwrap_or("<end>"),
                    b.map(str::trim).unwrap_or("<end>")
                ));
            }
        }
    }
}
