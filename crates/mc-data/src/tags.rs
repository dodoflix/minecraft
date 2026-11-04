//! Tags of the vanilla data pack (`data/minecraft/tags/<registry>/**.json`), with `#tag` references resolved.
//!
//! `get::<Block>("minecraft:mineable/pickaxe")` gives a typed set for any built-in registry; `names` returns the resolved
//! entry names of any tag directory, including those of data-driven registries (`worldgen/biome`, `enchantment`, ...).

use crate::registry::RegistryId;
use serde_json::Value;
use std::any::Any;
use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};

/// A resolved tag of a built-in registry: sorted, deduplicated ids.
#[derive(Debug, Clone)]
pub struct TagSet<T> {
    ids: Vec<T>,
}

impl<T: RegistryId> TagSet<T> {
    pub fn contains(&self, id: T) -> bool {
        self.ids.binary_search(&id).is_ok()
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = T> + '_ {
        self.ids.iter().copied()
    }

    pub fn len(&self) -> usize {
        self.ids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }
}

fn normalize(name: &str) -> String {
    if name.contains(':') { name.to_string() } else { format!("minecraft:{name}") }
}

/// Raw (unresolved) `values` of a tag file.
fn load_raw(dir: &str, name: &str) -> Option<Vec<(String, bool)>> {
    let (ns, path) = name.split_once(':')?;
    let text = crate::datapack::read_string(&format!("data/{ns}/tags/{dir}/{path}.json"))?;
    let json: Value = serde_json::from_str(&text).ok()?;
    let mut out = Vec::new();
    for v in json.get("values")?.as_array()? {
        match v {
            Value::String(s) => out.push((s.clone(), true)),
            Value::Object(o) => out.push((o.get("id")?.as_str()?.to_string(), o.get("required").and_then(Value::as_bool).unwrap_or(true))),
            _ => return None,
        }
    }
    Some(out)
}

fn resolve(dir: &str, name: &str, seen: &mut HashSet<String>, out: &mut Vec<String>) -> Option<()> {
    if !seen.insert(name.to_string()) {
        return Some(()); // reference cycle: vanilla's TagLoader ignores the repeat
    }
    for (entry, _required) in load_raw(dir, name)? {
        match entry.strip_prefix('#') {
            // a missing referenced tag only fails if it was required; the vanilla pack has no such case
            Some(r) => {
                resolve(dir, &normalize(r), seen, out);
            }
            None => {
                let e = normalize(&entry);
                if !out.contains(&e) {
                    out.push(e);
                }
            }
        }
    }
    Some(())
}

/// Resolved entry names of tag `name` in tag directory `dir` (`block`, `item`, `worldgen/biome`, ...), in file order.
pub fn names(dir: &str, name: &str) -> Option<Vec<String>> {
    let mut out = Vec::new();
    resolve(dir, &normalize(name), &mut HashSet::new(), &mut out)?;
    Some(out)
}

/// All tag names (`minecraft:...`) defined for a tag directory.
pub fn list(dir: &str) -> Vec<String> {
    let prefix: &'static str = Box::leak(format!("data/minecraft/tags/{dir}/").into_boxed_str());
    let mut v: Vec<String> = crate::datapack::list(prefix)
        .filter_map(|p| p.strip_prefix(prefix)?.strip_suffix(".json").map(|n| format!("minecraft:{n}")))
        .collect();
    v.sort();
    v
}

type Cache = Mutex<HashMap<(&'static str, String), Option<&'static (dyn Any + Send + Sync)>>>;

/// Typed tag lookup (`minecraft:` namespace optional); cached for the lifetime of the process.
pub fn get<T: RegistryId>(name: &str) -> Option<&'static TagSet<T>> {
    static CACHE: OnceLock<Cache> = OnceLock::new();
    let reg = T::registry();
    let key = (reg.name(), normalize(name));
    let mut cache = CACHE.get_or_init(Default::default).lock().unwrap();
    let entry = cache.entry(key.clone()).or_insert_with(|| {
        let names = names(reg.tag_dir(), &key.1)?;
        let mut ids: Vec<T> = names.iter().filter_map(|n| reg.id_of(n).and_then(T::from_id)).collect();
        ids.sort();
        ids.dedup();
        let leaked: &'static TagSet<T> = Box::leak(Box::new(TagSet { ids }));
        Some(leaked as &'static (dyn Any + Send + Sync))
    });
    (*entry)?.downcast_ref::<TagSet<T>>()
}
