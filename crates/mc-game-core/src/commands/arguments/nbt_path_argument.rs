//! `NbtPathArgument` and `NbtPathArgument.NbtPath`.
//!
//! Vanilla's nodes collect *references* into the tag tree and mutate through them. Here the intermediate results of the
//! mutating operations are paths from the root (`Vec<Step>`), resolved to `&mut Tag` when a node needs to act on them;
//! the processing order is the same level-by-level order as vanilla's, so results, counts and "nothing found" errors
//! agree. Elements of `byte[]`/`int[]`/`long[]` have no tag identity (they are values); like in vanilla nothing below
//! them can match, they are kept as dead entries so the emptiness checks behave identically.

use crate::commands::CommandSourceStack;
use crate::commands::message::{dynamic_translatable, message, simple_translatable};
use mc_brigadier::arguments::value;
use mc_brigadier::utf16::substring;
use mc_brigadier::{
    ArgumentType, CommandContext, CommandSyntaxException, DynamicCommandExceptionType,
    ImmutableStringReader, ParsedValue, SimpleCommandExceptionType, StringReader,
};
use mc_nbt::nbt_utils::compare_nbt;
use mc_nbt::{CompoundTag, ListTag, Tag};
use std::borrow::Cow;
use std::sync::LazyLock;

pub static ERROR_INVALID_NODE: LazyLock<SimpleCommandExceptionType> =
    LazyLock::new(|| simple_translatable("arguments.nbtpath.node.invalid"));
pub static ERROR_DATA_TOO_DEEP: LazyLock<SimpleCommandExceptionType> =
    LazyLock::new(|| simple_translatable("arguments.nbtpath.too_deep"));
pub static ERROR_NOTHING_FOUND: LazyLock<DynamicCommandExceptionType<String>> =
    LazyLock::new(|| dynamic_translatable("arguments.nbtpath.nothing_found"));
static ERROR_EXPECTED_LIST: LazyLock<DynamicCommandExceptionType<String>> =
    LazyLock::new(|| dynamic_translatable("commands.data.modify.expected_list"));
static ERROR_INVALID_INDEX: LazyLock<DynamicCommandExceptionType<i32>> = LazyLock::new(|| {
    DynamicCommandExceptionType::new(|node: i32| {
        message(crate::translatable!(
            "commands.data.modify.invalid_index",
            node
        ))
    })
});

#[derive(Clone, Debug, PartialEq)]
enum Node {
    AllElements,
    CompoundChild(String),
    IndexedElement(i32),
    MatchElement(CompoundTag),
    MatchObject(String, CompoundTag),
    MatchRootObject(CompoundTag),
}

#[derive(Clone, Debug, PartialEq)]
enum Step {
    Key(String),
    Index(usize),
}

type Path = Vec<Step>;

/// An entry of a level's result list: a mutable position in the tree, or a value without identity.
#[derive(Clone, Debug)]
enum Entry {
    At(Path),
    Dead(Tag),
}

fn resolve_mut<'a>(root: &'a mut Tag, path: &[Step]) -> Option<&'a mut Tag> {
    let mut tag = root;
    for step in path {
        tag = match (step, tag) {
            (Step::Key(k), Tag::Compound(c)) => c.get_mut(k)?,
            (Step::Index(i), Tag::List(l)) => l.get_mut(*i)?,
            _ => return None,
        };
    }
    Some(tag)
}

// ---- CollectionTag (ListTag + the three array tags) ----

fn collection_size(tag: &Tag) -> Option<usize> {
    match tag {
        Tag::List(l) => Some(l.len()),
        Tag::ByteArray(a) => Some(a.len()),
        Tag::IntArray(a) => Some(a.len()),
        Tag::LongArray(a) => Some(a.len()),
        _ => None,
    }
}

fn collection_get(tag: &Tag, index: usize) -> Tag {
    match tag {
        Tag::List(l) => l[index].clone(),
        Tag::ByteArray(a) => Tag::Byte(a[index]),
        Tag::IntArray(a) => Tag::Int(a[index]),
        Tag::LongArray(a) => Tag::Long(a[index]),
        _ => unreachable!("not a collection tag"),
    }
}

/// `CollectionTag.setTag`.
fn collection_set(tag: &mut Tag, index: usize, value: Tag) -> bool {
    match tag {
        Tag::List(l) => {
            l.set(index, value);
            true
        }
        Tag::ByteArray(a) => value.as_number().is_some_and(|n| {
            a[index] = n.byte_value();
            true
        }),
        Tag::IntArray(a) => value.as_number().is_some_and(|n| {
            a[index] = n.int_value();
            true
        }),
        Tag::LongArray(a) => value.as_number().is_some_and(|n| {
            a[index] = n.long_value();
            true
        }),
        _ => false,
    }
}

/// `CollectionTag.addTag`; `index > size` is vanilla's `IndexOutOfBoundsException` (`Err`).
fn collection_add(tag: &mut Tag, index: usize, value: Tag) -> Result<bool, ()> {
    let size = collection_size(tag).unwrap_or(0);
    if index > size {
        return Err(());
    }
    Ok(match tag {
        Tag::List(l) => {
            l.insert(index, value);
            true
        }
        Tag::ByteArray(a) => value.as_number().is_some_and(|n| {
            a.insert(index, n.byte_value());
            true
        }),
        Tag::IntArray(a) => value.as_number().is_some_and(|n| {
            a.insert(index, n.int_value());
            true
        }),
        Tag::LongArray(a) => value.as_number().is_some_and(|n| {
            a.insert(index, n.long_value());
            true
        }),
        _ => false,
    })
}

fn collection_remove(tag: &mut Tag, index: usize) {
    match tag {
        Tag::List(l) => {
            l.remove(index);
        }
        Tag::ByteArray(a) => {
            a.remove(index);
        }
        Tag::IntArray(a) => {
            a.remove(index);
        }
        Tag::LongArray(a) => {
            a.remove(index);
        }
        _ => {}
    }
}

fn collection_clear(tag: &mut Tag) {
    match tag {
        Tag::List(l) => l.clear(),
        Tag::ByteArray(a) => a.clear(),
        Tag::IntArray(a) => a.clear(),
        Tag::LongArray(a) => a.clear(),
        _ => {}
    }
}

fn matches_pattern(pattern: &CompoundTag, tag: Option<&Tag>) -> bool {
    compare_nbt(Some(&Tag::Compound(pattern.clone())), tag, true)
}

fn actual_index(index: i32, size: usize) -> Option<usize> {
    let size = size as i32;
    let actual = if index < 0 {
        size.wrapping_add(index)
    } else {
        index
    };
    (0 <= actual && actual < size).then_some(actual as usize)
}

impl Node {
    /// `createPreferredParentTag`.
    fn create_preferred_parent_tag(&self) -> Tag {
        match self {
            Node::AllElements | Node::IndexedElement(_) | Node::MatchElement(_) => {
                Tag::List(ListTag::new())
            }
            Node::CompoundChild(_) | Node::MatchObject(..) | Node::MatchRootObject(_) => {
                Tag::Compound(CompoundTag::new())
            }
        }
    }

    /// `getTag` (read only).
    fn get_tag<'a>(&self, parent: &'a Tag, out: &mut Vec<Cow<'a, Tag>>) {
        match self {
            Node::AllElements => {
                if let Some(size) = collection_size(parent) {
                    match parent {
                        Tag::List(l) => out.extend(l.iter().map(Cow::Borrowed)),
                        _ => out.extend((0..size).map(|i| Cow::Owned(collection_get(parent, i)))),
                    }
                }
            }
            Node::CompoundChild(name) => {
                if let Tag::Compound(c) = parent
                    && let Some(r) = c.get(name)
                {
                    out.push(Cow::Borrowed(r));
                }
            }
            Node::IndexedElement(index) => {
                if let Some(size) = collection_size(parent)
                    && let Some(i) = actual_index(*index, size)
                {
                    match parent {
                        Tag::List(l) => out.push(Cow::Borrowed(&l[i])),
                        _ => out.push(Cow::Owned(collection_get(parent, i))),
                    }
                }
            }
            Node::MatchElement(pattern) => {
                if let Tag::List(l) = parent {
                    out.extend(
                        l.iter()
                            .filter(|t| matches_pattern(pattern, Some(t)))
                            .map(Cow::Borrowed),
                    );
                }
            }
            Node::MatchObject(name, pattern) => {
                if let Tag::Compound(c) = parent {
                    let result = c.get(name);
                    if matches_pattern(pattern, result)
                        && let Some(r) = result
                    {
                        out.push(Cow::Borrowed(r));
                    }
                }
            }
            Node::MatchRootObject(pattern) => {
                if matches!(parent, Tag::Compound(_)) && matches_pattern(pattern, Some(parent)) {
                    out.push(Cow::Borrowed(parent));
                }
            }
        }
    }

    /// `getTag` over paths: the positions (or dead values) matched below `parent` at `parent_path`.
    fn get_paths(&self, parent: &Tag, parent_path: &Path, out: &mut Vec<Entry>) {
        let child = |step: Step| {
            let mut p = parent_path.clone();
            p.push(step);
            Entry::At(p)
        };
        match self {
            Node::AllElements => match parent {
                Tag::List(l) => out.extend((0..l.len()).map(|i| child(Step::Index(i)))),
                _ => out.extend(
                    (0..collection_size(parent).unwrap_or(0))
                        .map(|i| Entry::Dead(collection_get(parent, i))),
                ),
            },
            Node::CompoundChild(name) => {
                if let Tag::Compound(c) = parent
                    && c.contains(name)
                {
                    out.push(child(Step::Key(name.clone())));
                }
            }
            Node::IndexedElement(index) => {
                if let Some(size) = collection_size(parent)
                    && let Some(i) = actual_index(*index, size)
                {
                    out.push(if matches!(parent, Tag::List(_)) {
                        child(Step::Index(i))
                    } else {
                        Entry::Dead(collection_get(parent, i))
                    });
                }
            }
            Node::MatchElement(pattern) => {
                if let Tag::List(l) = parent {
                    out.extend(
                        l.iter()
                            .enumerate()
                            .filter(|(_, t)| matches_pattern(pattern, Some(t)))
                            .map(|(i, _)| child(Step::Index(i))),
                    );
                }
            }
            Node::MatchObject(name, pattern) => {
                if let Tag::Compound(c) = parent {
                    let result = c.get(name);
                    if matches_pattern(pattern, result) && result.is_some() {
                        out.push(child(Step::Key(name.clone())));
                    }
                }
            }
            Node::MatchRootObject(pattern) => {
                if matches!(parent, Tag::Compound(_)) && matches_pattern(pattern, Some(parent)) {
                    out.push(Entry::At(parent_path.clone()));
                }
            }
        }
    }

    /// `getOrCreateTag` over paths.
    fn get_or_create_paths(
        &self,
        parent: &mut Tag,
        parent_path: &Path,
        child_supplier: &dyn Fn() -> Tag,
        out: &mut Vec<Entry>,
    ) {
        let child = |step: Step| {
            let mut p = parent_path.clone();
            p.push(step);
            Entry::At(p)
        };
        match self {
            Node::AllElements => match parent {
                Tag::List(l) => {
                    if l.is_empty() {
                        l.insert(0, child_supplier());
                        out.push(child(Step::Index(0)));
                    } else {
                        out.extend((0..l.len()).map(|i| child(Step::Index(i))));
                    }
                }
                Tag::ByteArray(_) | Tag::IntArray(_) | Tag::LongArray(_) => {
                    if collection_size(parent) == Some(0) {
                        // `list.addTag(0, result)` adds only when the supplied tag is numeric.
                        let result = child_supplier();
                        if collection_add(parent, 0, result).unwrap_or(false) {
                            out.push(Entry::Dead(collection_get(parent, 0)));
                        }
                    } else {
                        out.extend(
                            (0..collection_size(parent).unwrap_or(0))
                                .map(|i| Entry::Dead(collection_get(parent, i))),
                        );
                    }
                }
                _ => {}
            },
            Node::CompoundChild(name) => {
                if let Tag::Compound(c) = parent {
                    if !c.contains(name) {
                        c.put(name.clone(), child_supplier());
                    }
                    out.push(child(Step::Key(name.clone())));
                }
            }
            Node::IndexedElement(_) => self.get_paths(parent, parent_path, out),
            Node::MatchElement(pattern) => {
                if let Tag::List(l) = parent {
                    let before = out.len();
                    out.extend(
                        l.iter()
                            .enumerate()
                            .filter(|(_, t)| matches_pattern(pattern, Some(t)))
                            .map(|(i, _)| child(Step::Index(i))),
                    );
                    if out.len() == before {
                        l.push(Tag::Compound(pattern.copy()));
                        out.push(child(Step::Index(l.len() - 1)));
                    }
                }
            }
            Node::MatchObject(name, pattern) => {
                if let Tag::Compound(c) = parent {
                    match c.get(name) {
                        None => {
                            c.put(name.clone(), Tag::Compound(pattern.copy()));
                            out.push(child(Step::Key(name.clone())));
                        }
                        Some(result) if matches_pattern(pattern, Some(result)) => {
                            out.push(child(Step::Key(name.clone())))
                        }
                        Some(_) => {}
                    }
                }
            }
            Node::MatchRootObject(_) => self.get_paths(parent, parent_path, out),
        }
    }

    /// `setTag`: the number of changed tags.
    fn set_tag(&self, parent: &mut Tag, to_add: &mut dyn FnMut() -> Tag) -> i32 {
        match self {
            Node::AllElements => {
                let Some(size) = collection_size(parent) else {
                    return 0;
                };
                if size == 0 {
                    let _ = collection_add(parent, 0, to_add());
                    return 1;
                }
                let new_value = to_add();
                let changed_count = size as i32
                    - (0..size)
                        .filter(|&i| collection_get(parent, i) == new_value)
                        .count() as i32;
                if changed_count == 0 {
                    return 0;
                }
                collection_clear(parent);
                if !collection_add(parent, 0, new_value).unwrap_or(false) {
                    return 0;
                }
                for i in 1..size {
                    let _ = collection_add(parent, i, to_add());
                }
                changed_count
            }
            Node::CompoundChild(name) => {
                if let Tag::Compound(c) = parent {
                    let new_value = to_add();
                    let previous = c.put(name.clone(), new_value.clone());
                    if previous.as_ref() != Some(&new_value) {
                        return 1;
                    }
                }
                0
            }
            Node::IndexedElement(index) => {
                if let Some(size) = collection_size(parent)
                    && let Some(i) = actual_index(*index, size)
                {
                    let previous = collection_get(parent, i);
                    let new_value = to_add();
                    if new_value != previous && collection_set(parent, i, new_value) {
                        return 1;
                    }
                }
                0
            }
            Node::MatchElement(pattern) => {
                let mut changed = 0;
                if let Tag::List(l) = parent {
                    let size = l.len();
                    if size == 0 {
                        l.push(to_add());
                        changed += 1;
                    } else {
                        for i in 0..size {
                            let current = l[i].clone();
                            if matches_pattern(pattern, Some(&current)) {
                                let new_value = to_add();
                                if new_value != current {
                                    l.set(i, new_value);
                                    changed += 1;
                                }
                            }
                        }
                    }
                }
                changed
            }
            Node::MatchObject(name, pattern) => {
                if let Tag::Compound(c) = parent {
                    let current = c.get(name).cloned();
                    if matches_pattern(pattern, current.as_ref()) {
                        let new_value = to_add();
                        if Some(&new_value) != current.as_ref() {
                            c.put(name.clone(), new_value);
                            return 1;
                        }
                    }
                }
                0
            }
            Node::MatchRootObject(_) => 0,
        }
    }

    /// `removeTag`: the number of removed tags.
    fn remove_tag(&self, parent: &mut Tag) -> i32 {
        match self {
            Node::AllElements => {
                if let Some(size) = collection_size(parent)
                    && size > 0
                {
                    collection_clear(parent);
                    return size as i32;
                }
                0
            }
            Node::CompoundChild(name) => match parent {
                Tag::Compound(c) if c.contains(name) => {
                    c.remove(name);
                    1
                }
                _ => 0,
            },
            Node::IndexedElement(index) => {
                if let Some(size) = collection_size(parent)
                    && let Some(i) = actual_index(*index, size)
                {
                    collection_remove(parent, i);
                    return 1;
                }
                0
            }
            Node::MatchElement(pattern) => {
                let mut changed = 0;
                if let Tag::List(l) = parent {
                    for i in (0..l.len()).rev() {
                        if matches_pattern(pattern, Some(&l[i])) {
                            l.remove(i);
                            changed += 1;
                        }
                    }
                }
                changed
            }
            Node::MatchObject(name, pattern) => {
                if let Tag::Compound(c) = parent
                    && matches_pattern(pattern, c.get(name))
                {
                    c.remove(name);
                    return 1;
                }
                0
            }
            Node::MatchRootObject(_) => 0,
        }
    }
}

/// `NbtPathArgument.NbtPath`.
#[derive(Clone, Debug)]
pub struct NbtPath {
    original: String,
    nodes: Vec<Node>,
    /// `nodeToOriginalPosition` per node (the shared `[]` singleton keeps its last position, like vanilla's map).
    node_positions: Vec<usize>,
}

impl PartialEq for NbtPath {
    fn eq(&self, other: &Self) -> bool {
        self.original == other.original
    }
}

impl NbtPath {
    /// `NbtPath.of(string)`.
    pub fn of(string: &str) -> Result<NbtPath, CommandSyntaxException> {
        NbtPathArgument::parse_path(&mut StringReader::new(string))
    }

    pub fn as_string(&self) -> &str {
        &self.original
    }

    fn create_not_found_exception(&self, node_index: usize) -> CommandSyntaxException {
        let mut index = self.node_positions[node_index];
        if self.nodes[node_index] == Node::AllElements {
            // `AllElementsNode.INSTANCE` is one key in the map: the last `[]` position wins.
            index = self
                .nodes
                .iter()
                .zip(&self.node_positions)
                .rev()
                .find(|(n, _)| **n == Node::AllElements)
                .map_or(index, |(_, p)| *p);
        }
        ERROR_NOTHING_FOUND.create(substring(&self.original, 0, index))
    }

    /// `get(tag)`.
    pub fn get(&self, tag: &Tag) -> Result<Vec<Tag>, CommandSyntaxException> {
        let mut result: Vec<Cow<'_, Tag>> = vec![Cow::Borrowed(tag)];
        for (i, node) in self.nodes.iter().enumerate() {
            let mut next: Vec<Cow<'_, Tag>> = Vec::new();
            for t in &result {
                match t {
                    Cow::Borrowed(b) => node.get_tag(b, &mut next),
                    Cow::Owned(o) => {
                        // Values without identity (array elements): nothing below them can match.
                        let mut tmp = Vec::new();
                        node.get_tag(o, &mut tmp);
                        next.extend(tmp.into_iter().map(|c| Cow::Owned(c.into_owned())));
                    }
                }
            }
            result = next;
            if result.is_empty() {
                return Err(self.create_not_found_exception(i));
            }
        }
        Ok(result.into_iter().map(Cow::into_owned).collect())
    }

    /// `countMatching(tag)`.
    pub fn count_matching(&self, tag: &Tag) -> usize {
        let mut result: Vec<Cow<'_, Tag>> = vec![Cow::Borrowed(tag)];
        for node in &self.nodes {
            let mut next: Vec<Cow<'_, Tag>> = Vec::new();
            for t in &result {
                match t {
                    Cow::Borrowed(b) => node.get_tag(b, &mut next),
                    Cow::Owned(o) => {
                        let mut tmp = Vec::new();
                        node.get_tag(o, &mut tmp);
                        next.extend(tmp.into_iter().map(|c| Cow::Owned(c.into_owned())));
                    }
                }
            }
            result = next;
            if result.is_empty() {
                return 0;
            }
        }
        result.len()
    }

    /// `getOrCreateParents(tag)`: the entries matched by all nodes but the last.
    fn get_or_create_parents(&self, root: &mut Tag) -> Result<Vec<Entry>, CommandSyntaxException> {
        let mut result = vec![Entry::At(Vec::new())];
        for i in 0..self.nodes.len().saturating_sub(1) {
            let node = &self.nodes[i];
            let preferred = &self.nodes[i + 1];
            let supplier = || preferred.create_preferred_parent_tag();
            result = Self::get_or_create_level(node, root, &result, &supplier);
            if result.is_empty() {
                return Err(self.create_not_found_exception(i));
            }
        }
        Ok(result)
    }

    fn get_or_create_level(
        node: &Node,
        root: &mut Tag,
        entries: &[Entry],
        child: &dyn Fn() -> Tag,
    ) -> Vec<Entry> {
        let mut next = Vec::new();
        for entry in entries {
            if let Entry::At(path) = entry
                && let Some(tag) = resolve_mut(root, path)
            {
                node.get_or_create_paths(tag, path, child, &mut next);
            }
        }
        next
    }

    /// `getOrCreate(tag, newTagValue)`: the tags matched by the whole path, creating missing ones (as values).
    pub fn get_or_create(
        &self,
        tag: &mut Tag,
        new_tag_value: &dyn Fn() -> Tag,
    ) -> Result<Vec<Tag>, CommandSyntaxException> {
        let entries = self.get_or_create_last_level(tag, new_tag_value)?;
        Ok(entries
            .into_iter()
            .filter_map(|e| match e {
                Entry::At(path) => resolve_mut(tag, &path).map(|t| t.clone()),
                Entry::Dead(value) => Some(value),
            })
            .collect())
    }

    /// `getOrCreate(tag, newTagValue)` for callers that mutate the matched tags in place: runs `f` on each of them
    /// (vanilla returns references into the tree). Elements of numeric arrays have no identity: `f` sees a copy.
    pub fn get_or_create_mut(
        &self,
        tag: &mut Tag,
        new_tag_value: &dyn Fn() -> Tag,
        f: &mut dyn FnMut(&mut Tag) -> Result<(), CommandSyntaxException>,
    ) -> Result<(), CommandSyntaxException> {
        for entry in self.get_or_create_last_level(tag, new_tag_value)? {
            match entry {
                Entry::At(path) => {
                    if let Some(t) = resolve_mut(tag, &path) {
                        f(t)?;
                    }
                }
                Entry::Dead(mut value) => f(&mut value)?,
            }
        }
        Ok(())
    }

    fn get_or_create_last_level(
        &self,
        tag: &mut Tag,
        new_tag_value: &dyn Fn() -> Tag,
    ) -> Result<Vec<Entry>, CommandSyntaxException> {
        let parents = self.get_or_create_parents(tag)?;
        let last = self.nodes.last().expect("path has nodes");
        Ok(Self::get_or_create_level(
            last,
            tag,
            &parents,
            new_tag_value,
        ))
    }

    /// `isTooDeep(tag, depth)`.
    pub fn is_too_deep(tag: &Tag, depth: usize) -> bool {
        if depth >= 512 {
            return true;
        }
        match tag {
            Tag::Compound(c) => c
                .iter()
                .any(|(_, child)| Self::is_too_deep(child, depth + 1)),
            Tag::List(l) => l.iter().any(|e| Self::is_too_deep(e, depth + 1)),
            _ => false,
        }
    }

    fn estimate_path_depth(&self) -> usize {
        self.nodes.len()
    }

    /// `set(tag, toAdd)`: the number of changed tags.
    pub fn set(&self, tag: &mut Tag, to_add: &Tag) -> Result<i32, CommandSyntaxException> {
        if Self::is_too_deep(to_add, self.estimate_path_depth()) {
            return Err(ERROR_DATA_TOO_DEEP.create());
        }
        let parents = self.get_or_create_parents(tag)?;
        if parents.is_empty() {
            return Ok(0);
        }
        let last = self.nodes.last().expect("path has nodes");
        let mut changed = 0i32;
        for entry in parents {
            if let Entry::At(path) = entry
                && let Some(parent) = resolve_mut(tag, &path)
            {
                changed = changed.wrapping_add(last.set_tag(parent, &mut || to_add.copy()));
            }
        }
        Ok(changed)
    }

    /// `insert(index, target, toInsert)`: the number of modified targets.
    pub fn insert(
        &self,
        index: i32,
        target: &mut CompoundTag,
        to_insert: &[Tag],
    ) -> Result<i32, CommandSyntaxException> {
        let mut to_insert_copy = Vec::with_capacity(to_insert.len());
        for tag in to_insert {
            let copy = tag.copy();
            if Self::is_too_deep(&copy, self.estimate_path_depth()) {
                return Err(ERROR_DATA_TOO_DEEP.create());
            }
            to_insert_copy.push(copy);
        }
        let mut root = Tag::Compound(std::mem::take(target));
        let result = self.insert_into(index, &mut root, to_insert_copy);
        if let Tag::Compound(c) = root {
            *target = c;
        }
        result
    }

    fn insert_into(
        &self,
        index: i32,
        root: &mut Tag,
        to_insert_copy: Vec<Tag>,
    ) -> Result<i32, CommandSyntaxException> {
        let entries = self.get_or_create_last_level(root, &|| Tag::List(ListTag::new()))?;
        let mut modified_count = 0;
        let mut used_first = false;
        for entry in entries {
            let path = match entry {
                Entry::At(path) => path,
                Entry::Dead(value) => return Err(ERROR_EXPECTED_LIST.create(value.to_string())),
            };
            let Some(target_tag) = resolve_mut(root, &path) else {
                continue;
            };
            let Some(size) = collection_size(target_tag) else {
                return Err(ERROR_EXPECTED_LIST.create(target_tag.to_string()));
            };
            let mut modified = false;
            let mut actual = if index < 0 {
                (size as i32).wrapping_add(index).wrapping_add(1)
            } else {
                index
            };
            for source in &to_insert_copy {
                let value = if used_first {
                    source.copy()
                } else {
                    source.clone()
                };
                let added = if actual < 0 {
                    Err(())
                } else {
                    collection_add(target_tag, actual as usize, value)
                };
                match added {
                    Ok(true) => {
                        actual += 1;
                        modified = true;
                    }
                    Ok(false) => {}
                    Err(()) => return Err(ERROR_INVALID_INDEX.create(actual)),
                }
            }
            used_first = true;
            modified_count += i32::from(modified);
        }
        Ok(modified_count)
    }

    /// `remove(tag)`: the number of removed tags.
    pub fn remove(&self, tag: &mut Tag) -> i32 {
        let mut result = vec![Entry::At(Vec::new())];
        for node in &self.nodes[..self.nodes.len().saturating_sub(1)] {
            let mut next = Vec::new();
            for entry in &result {
                if let Entry::At(path) = entry
                    && let Some(t) = resolve_mut(tag, path)
                {
                    node.get_paths(t, path, &mut next);
                }
            }
            result = next;
        }
        let last = self.nodes.last().expect("path has nodes");
        let mut changed = 0i32;
        for entry in result {
            if let Entry::At(path) = entry
                && let Some(parent) = resolve_mut(tag, &path)
            {
                changed = changed.wrapping_add(last.remove_tag(parent));
            }
        }
        changed
    }
}

impl std::fmt::Display for NbtPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.original)
    }
}

/// `NbtPathArgument`.
#[derive(Clone, Copy, Debug, Default)]
pub struct NbtPathArgument;

impl NbtPathArgument {
    pub fn nbt_path() -> Self {
        Self
    }

    pub fn get_path(context: &CommandContext<CommandSourceStack>, name: &str) -> NbtPath {
        context.get_argument::<NbtPath>(name).clone()
    }

    fn parse_path(reader: &mut StringReader) -> Result<NbtPath, CommandSyntaxException> {
        let mut nodes = Vec::new();
        let mut positions = Vec::new();
        let start = reader.get_cursor();
        let mut first_node = true;
        while reader.can_read() && reader.peek() != ' ' {
            let node = Self::parse_node(reader, first_node)?;
            nodes.push(node);
            positions.push(reader.get_cursor() - start);
            first_node = false;
            if reader.can_read() {
                let next = reader.peek();
                if next != ' ' && next != '[' && next != '{' {
                    reader.expect('.')?;
                }
            }
        }
        Ok(NbtPath {
            original: substring(reader.get_string(), start, reader.get_cursor()),
            nodes,
            node_positions: positions,
        })
    }

    fn parse_node(
        reader: &mut StringReader,
        first_node: bool,
    ) -> Result<Node, CommandSyntaxException> {
        match reader.peek() {
            '"' | '\'' => {
                let name = reader.read_string()?;
                Self::read_object_node(reader, name)
            }
            '[' => {
                reader.skip();
                let next = reader.peek();
                if next == '{' {
                    let pattern = super::nbt_arguments::parse_compound_as_argument(reader)?;
                    reader.expect(']')?;
                    Ok(Node::MatchElement(pattern))
                } else if next == ']' {
                    reader.skip();
                    Ok(Node::AllElements)
                } else {
                    let index = reader.read_int()?;
                    reader.expect(']')?;
                    Ok(Node::IndexedElement(index))
                }
            }
            '{' => {
                if !first_node {
                    return Err(ERROR_INVALID_NODE.create_with_context(reader));
                }
                let pattern = super::nbt_arguments::parse_compound_as_argument(reader)?;
                Ok(Node::MatchRootObject(pattern))
            }
            _ => {
                let name = Self::read_unquoted_name(reader)?;
                Self::read_object_node(reader, name)
            }
        }
    }

    fn read_object_node(
        reader: &mut StringReader,
        name: String,
    ) -> Result<Node, CommandSyntaxException> {
        if name.is_empty() {
            Err(ERROR_INVALID_NODE.create_with_context(reader))
        } else if reader.can_read() && reader.peek() == '{' {
            let pattern = super::nbt_arguments::parse_compound_as_argument(reader)?;
            Ok(Node::MatchObject(name, pattern))
        } else {
            Ok(Node::CompoundChild(name))
        }
    }

    fn read_unquoted_name(reader: &mut StringReader) -> Result<String, CommandSyntaxException> {
        let start = reader.get_cursor();
        while reader.can_read() && Self::is_allowed_in_unquoted_name(reader.peek()) {
            reader.skip();
        }
        if reader.get_cursor() == start {
            Err(ERROR_INVALID_NODE.create_with_context(reader))
        } else {
            Ok(substring(reader.get_string(), start, reader.get_cursor()))
        }
    }

    fn is_allowed_in_unquoted_name(c: char) -> bool {
        !matches!(c, ' ' | '"' | '\'' | '[' | ']' | '.' | '{' | '}')
    }
}

impl ArgumentType<CommandSourceStack> for NbtPathArgument {
    fn parse(&self, reader: &mut StringReader) -> Result<ParsedValue, CommandSyntaxException> {
        Self::parse_path(reader).map(value)
    }

    fn get_examples(&self) -> Vec<String> {
        ["foo", "foo.bar", "foo[0]", "[0]", "[]", "{foo=bar}"]
            .map(String::from)
            .to_vec()
    }

    fn describe(&self) -> String {
        "NbtPathArgument".to_owned()
    }
}

crate::register_argument_type!(NbtPathArgument, |_a| {
    mc_protocol::packets::play::ArgumentTypeTemplate::NbtPath
});
