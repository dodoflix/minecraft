//! `net.minecraft.tags.{TagEntry, TagFile, TagLoader}` + `util.DependencySorter`: tag files from every selected pack
//! (`data/<ns>/tags/<registry path>/**.json`), `replace`, optional entries, `#tag` references resolved in dependency
//! order with vanilla's log messages for unreadable files and missing references.

use std::collections::{HashMap, HashSet};
use std::fmt;

use indexmap::IndexMap;
use mc_core::ident::Identifier;
use serde_json::Value;

use crate::server::packs::{FileToIdConverter, ResourceManager};

/// `TagEntry`: an element or `#tag` reference, optionally not required.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TagEntry {
    pub id: Identifier,
    pub tag: bool,
    pub required: bool,
}

impl fmt::Display for TagEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.tag {
            f.write_str("#")?;
        }
        write!(f, "{}", self.id)?;
        if !self.required {
            f.write_str("?")?;
        }
        Ok(())
    }
}

fn compact(v: &Value) -> String {
    serde_json::to_string(v).unwrap_or_default()
}

/// `ExtraCodecs.TAG_OR_ELEMENT_ID`.
fn tag_or_element_id(v: &Value) -> Result<(Identifier, bool), String> {
    let s = v
        .as_str()
        .ok_or_else(|| format!("Not a string: {}", compact(v)))?;
    match s.strip_prefix('#') {
        Some(rest) => Ok((Identifier::read(rest)?, true)),
        None => Ok((Identifier::read(s)?, false)),
    }
}

impl TagEntry {
    /// `TagEntry.CODEC`: a string, or `{id, required?}`.
    pub fn decode(v: &Value) -> Result<TagEntry, String> {
        match v {
            Value::String(_) => {
                let (id, tag) = tag_or_element_id(v)?;
                Ok(TagEntry {
                    id,
                    tag,
                    required: true,
                })
            }
            Value::Object(o) => {
                let id = o
                    .get("id")
                    .ok_or_else(|| format!("No key id in MapLike[{}]", compact(v)))
                    .and_then(tag_or_element_id);
                let required = match o.get("required") {
                    None => Ok(true),
                    Some(Value::Bool(b)) => Ok(*b),
                    Some(Value::Number(n)) => Ok(n.as_f64().is_some_and(|f| f != 0.0)),
                    Some(other) => Err(format!("Not a boolean: {}", compact(other))),
                };
                match (id, required) {
                    (Ok((id, tag)), Ok(required)) => Ok(TagEntry { id, tag, required }),
                    (Err(a), Ok(_)) | (Ok(_), Err(a)) => Err(a),
                    (Err(a), Err(b)) => Err(format!("{b}; {a}")),
                }
            }
            other => Err(format!("Not a string: {}", compact(other))),
        }
    }
}

/// `TagFile.CODEC`: `{values: [entries], replace?: bool}`.
pub fn decode_tag_file(v: &Value) -> Result<(Vec<TagEntry>, bool), String> {
    let o = v
        .as_object()
        .ok_or_else(|| format!("Not a JSON object: {}", compact(v)))?;
    let entries = match o.get("values") {
        None => Err(format!("No key values in MapLike[{}]", compact(v))),
        Some(Value::Array(list)) => list
            .iter()
            .map(TagEntry::decode)
            .collect::<Result<Vec<_>, _>>(),
        Some(other) => Err(format!("Not a list: {}", compact(other))),
    };
    let replace = match o.get("replace") {
        None => Ok(false),
        Some(Value::Bool(b)) => Ok(*b),
        Some(Value::Number(n)) => Ok(n.as_f64().is_some_and(|f| f != 0.0)),
        Some(other) => Err(format!("Not a boolean: {}", compact(other))),
    };
    match (entries, replace) {
        (Ok(e), Ok(r)) => Ok((e, r)),
        (Err(a), Ok(_)) | (Ok(_), Err(a)) => Err(a),
        (Err(a), Err(b)) => Err(format!("{b}; {a}")),
    }
}

/// `TagLoader.EntryWithSource`.
#[derive(Clone, Debug)]
pub struct EntryWithSource {
    pub entry: TagEntry,
    pub source: String,
}

impl fmt::Display for EntryWithSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (from {})", self.entry, self.source)
    }
}

/// Result of resolving one element reference (`TagLoader.ElementLookup.get`).
pub enum ElementRef {
    /// The element exists with this id.
    Found(u32),
    /// A required reference to a not-yet-registered element of a registry being loaded: vanilla creates an *unbound*
    /// holder here (the registry then fails to freeze). The tag stays valid, the caller records the unbound key.
    Unbound,
    Missing,
}

/// `TagLoader<T>` over registry entry ids.
pub struct TagLoader<'a> {
    element_lookup: &'a mut dyn FnMut(&Identifier, bool) -> ElementRef,
    directory: String,
}

impl<'a> TagLoader<'a> {
    pub fn new(
        element_lookup: &'a mut dyn FnMut(&Identifier, bool) -> ElementRef,
        directory: &str,
    ) -> TagLoader<'a> {
        TagLoader {
            element_lookup,
            directory: directory.to_owned(),
        }
    }

    /// `load(resourceManager)`: every tag file of every pack, `replace` clearing what lower packs contributed.
    pub fn load(
        &self,
        manager: &dyn ResourceManager,
    ) -> IndexMap<Identifier, Vec<EntryWithSource>> {
        let mut builders: IndexMap<Identifier, Vec<EntryWithSource>> = IndexMap::new();
        let lister = FileToIdConverter::json(&self.directory);
        for (location, resources) in lister.list_matching_resource_stacks(manager) {
            let id = lister.file_to_id(&location);
            for resource in resources {
                let source_id = resource.source_pack_id().to_owned();
                let json = resource
                    .read_all_as_string()
                    .map_err(|e| ("java.io.IOException", e.to_string()))
                    .and_then(|text| {
                        serde_json::from_str::<Value>(&text)
                            .map_err(|e| ("com.google.gson.JsonSyntaxException", e.to_string()))
                    });
                let result = json.and_then(|json| {
                    // `computeIfAbsent` runs before the codec: a tag file that fails to decode still defines an (empty) tag
                    let contents = builders.entry(id.clone()).or_default();
                    let (entries, replace) =
                        decode_tag_file(&json).map_err(|e| ("java.lang.RuntimeException", e))?;
                    if replace {
                        contents.clear();
                    }
                    contents.extend(entries.into_iter().map(|entry| EntryWithSource {
                        entry,
                        source: source_id.clone(),
                    }));
                    Ok(())
                });
                if let Err((class, message)) = result {
                    log::error!(
                        "Couldn't read tag list {id} from {location} in data pack {source_id}\n{}",
                        crate::server::packs::throwable(class, &message)
                    );
                }
            }
        }
        builders
    }

    /// `build(builders)`: resolve the tags in dependency order; tags with unresolvable required references are logged and dropped.
    pub fn build(
        &mut self,
        builders: &IndexMap<Identifier, Vec<EntryWithSource>>,
    ) -> IndexMap<Identifier, Vec<u32>> {
        let mut new_tags: IndexMap<Identifier, Vec<u32>> = IndexMap::new();
        let order = order_by_dependencies(builders);
        for id in order {
            let Some(entries) = builders.get(&id) else {
                continue;
            };
            let mut values: Vec<u32> = Vec::new();
            let mut seen: HashSet<u32> = HashSet::new();
            let mut missing: Vec<&EntryWithSource> = Vec::new();
            for e in entries {
                let ok = if e.entry.tag {
                    match new_tags.get(&e.entry.id) {
                        None => !e.entry.required,
                        Some(t) => {
                            for v in t {
                                if seen.insert(*v) {
                                    values.push(*v);
                                }
                            }
                            true
                        }
                    }
                } else {
                    match (self.element_lookup)(&e.entry.id, e.entry.required) {
                        ElementRef::Found(v) => {
                            if seen.insert(v) {
                                values.push(v);
                            }
                            true
                        }
                        ElementRef::Unbound => true,
                        ElementRef::Missing => !e.entry.required,
                    }
                };
                if !ok {
                    missing.push(e);
                }
            }
            if missing.is_empty() {
                new_tags.insert(id, values);
            } else {
                log::error!(
                    "Couldn't load tag {id} as it is missing following references: {}",
                    missing
                        .iter()
                        .map(|m| m.to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
        }
        new_tags
    }
}

/// `DependencySorter.orderByDependencies`: required then optional tag references become edges unless they would
/// close a cycle; the output visits dependencies first.
fn order_by_dependencies(contents: &IndexMap<Identifier, Vec<EntryWithSource>>) -> Vec<Identifier> {
    let mut direct: HashMap<Identifier, Vec<Identifier>> = HashMap::new();
    fn is_cyclic(
        direct: &HashMap<Identifier, Vec<Identifier>>,
        from: &Identifier,
        to: &Identifier,
        depth: usize,
    ) -> bool {
        // `to` can only reach `from` through already inserted (acyclic) edges, so the walk terminates; the depth guard is paranoia
        if depth > 10_000 {
            return true;
        }
        match direct.get(to) {
            None => false,
            Some(deps) => {
                deps.contains(from) || deps.iter().any(|d| is_cyclic(direct, from, d, depth + 1))
            }
        }
    }
    for required in [true, false] {
        for (id, entries) in contents {
            for e in entries {
                if e.entry.tag
                    && e.entry.required == required
                    && !is_cyclic(&direct, id, &e.entry.id, 0)
                {
                    let deps = direct.entry(id.clone()).or_default();
                    if !deps.contains(&e.entry.id) {
                        deps.push(e.entry.id.clone());
                    }
                }
            }
        }
    }
    fn visit(
        direct: &HashMap<Identifier, Vec<Identifier>>,
        contents: &IndexMap<Identifier, Vec<EntryWithSource>>,
        visited: &mut HashSet<Identifier>,
        id: &Identifier,
        out: &mut Vec<Identifier>,
    ) {
        if visited.insert(id.clone()) {
            if let Some(deps) = direct.get(id) {
                for d in deps {
                    visit(direct, contents, visited, d, out);
                }
            }
            if contents.contains_key(id) {
                out.push(id.clone());
            }
        }
    }
    let mut visited = HashSet::new();
    let mut out = Vec::new();
    for id in contents.keys() {
        visit(&direct, contents, &mut visited, id, &mut out);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn id(s: &str) -> Identifier {
        Identifier::parse(s).unwrap()
    }

    #[test]
    fn entry_codec() {
        assert_eq!(
            TagEntry::decode(&json!("#minecraft:a"))
                .unwrap()
                .to_string(),
            "#minecraft:a"
        );
        assert_eq!(
            TagEntry::decode(&json!({"id": "foo:b", "required": false}))
                .unwrap()
                .to_string(),
            "foo:b?"
        );
        assert!(
            TagEntry::decode(&json!("Bad:Id"))
                .unwrap_err()
                .starts_with("Not a valid resource location: Bad:Id")
        );
        assert!(
            decode_tag_file(&json!({"replace": true}))
                .unwrap_err()
                .starts_with("No key values in MapLike[")
        );
    }

    fn entries(list: &[(&str, bool, bool)]) -> Vec<EntryWithSource> {
        list.iter()
            .map(|(i, tag, required)| EntryWithSource {
                entry: TagEntry {
                    id: id(i),
                    tag: *tag,
                    required: *required,
                },
                source: "p".into(),
            })
            .collect()
    }

    #[test]
    fn references_cycles_and_missing() {
        let mut builders = IndexMap::new();
        builders.insert(id("a"), entries(&[("x", false, true), ("b", true, true)]));
        builders.insert(id("b"), entries(&[("y", false, true), ("x", false, true)]));
        builders.insert(id("c"), entries(&[("nope", false, true)]));
        builders.insert(
            id("d"),
            entries(&[
                ("nope", false, false),
                ("missing_tag", true, false),
                ("y", false, true),
            ]),
        );
        let mut lookup = |i: &Identifier, _req: bool| match i.path() {
            "x" => ElementRef::Found(1),
            "y" => ElementRef::Found(2),
            _ => ElementRef::Missing,
        };
        let tags = TagLoader::new(&mut lookup, "block").build(&builders);
        assert_eq!(tags[&id("a")], vec![1, 2]); // `b` is built first, its entries come in at the `#b` position
        assert_eq!(tags[&id("b")], vec![2, 1]);
        assert!(!tags.contains_key(&id("c")));
        assert_eq!(tags[&id("d")], vec![2]);

        // a <-> b cycle: the second edge is dropped, `b` is built first and misses `#a`, so `a` misses `#b` too
        let mut cyc = IndexMap::new();
        cyc.insert(id("a"), entries(&[("b", true, true), ("x", false, true)]));
        cyc.insert(id("b"), entries(&[("a", true, true), ("y", false, true)]));
        let tags = TagLoader::new(&mut lookup, "block").build(&cyc);
        assert_eq!(tags.len(), 0);
    }
}
