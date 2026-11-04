//! `HashedStack` / `HashedPatchMap`: the item stacks a client sends in container clicks, with components reduced to
//! CRC32C hashes of their codec encoding (`HashOps`).

use super::item_stack::ItemStack;
use super::map::{DataComponentPatch, TypedDataComponent};
use super::registry::def;
use crate::codec::{read_count, read_varint, write_varint, Decode, DecodeResult, Encode};
use mc_data::{DataComponentType, Item};
use mc_dfu::HASH;

/// `HashedPatchMap.HashGenerator`.
pub type HashGenerator<'a> = &'a dyn Fn(&TypedDataComponent) -> i32;

/// The server's hasher (`ServerPlayer` container synchronizer): `codecOrThrow().encodeStart(HashOps, value).asInt()`.
pub fn component_hash(component: &TypedDataComponent) -> i32 {
    let d = def(component.id);
    match d.codec_or_throw().encode_start(&HASH, &component.value).into_result() {
        Ok(tree) => mc_dfu::hash_dyn(&tree) as i32,
        Err(e) => panic!("Failed to hash {}: {e}", component.id.name()),
    }
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct HashedPatchMap {
    /// Component type -> hash, sorted by type id.
    pub added_components: Vec<(DataComponentType, i32)>,
    /// Removed component types, sorted by id.
    pub removed_components: Vec<DataComponentType>,
}

impl HashedPatchMap {
    pub fn create(patch: &DataComponentPatch, hasher: HashGenerator) -> HashedPatchMap {
        let (added, removed) = patch.split();
        let mut added_components: Vec<_> =
            added.iter().map(|(k, v)| (k, hasher(&TypedDataComponent { id: k, value: v.clone() }))).collect();
        added_components.sort_by_key(|(k, _)| k.id());
        let mut removed_components = removed;
        removed_components.sort_by_key(|k| k.id());
        HashedPatchMap { added_components, removed_components }
    }

    pub fn matches(&self, patch: &DataComponentPatch, hasher: HashGenerator) -> bool {
        let (added, mut removed) = patch.split();
        removed.sort_by_key(|k| k.id());
        if removed != self.removed_components || self.added_components.len() != added.len() {
            return false;
        }
        for (k, v) in added.iter() {
            match self.added_components.iter().find(|(t, _)| *t == k) {
                None => return false,
                Some((_, expected)) => {
                    if hasher(&TypedDataComponent { id: k, value: v.clone() }) != *expected {
                        return false;
                    }
                }
            }
        }
        true
    }
}

impl Encode for HashedPatchMap {
    fn encode(&self, out: &mut Vec<u8>) {
        write_varint(out, self.added_components.len() as i32);
        for (k, h) in &self.added_components {
            k.encode(out);
            h.encode(out);
        }
        write_varint(out, self.removed_components.len() as i32);
        for k in &self.removed_components {
            k.encode(out);
        }
    }
}

impl Decode for HashedPatchMap {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        let n = read_count(input, 256)?;
        let mut added: Vec<(DataComponentType, i32)> = Vec::with_capacity(n);
        for _ in 0..n {
            let k = DataComponentType::decode(input)?;
            let h = i32::decode(input)?;
            match added.iter_mut().find(|(t, _)| *t == k) {
                Some(e) => e.1 = h,
                None => added.push((k, h)),
            }
        }
        let n = read_count(input, 256)?;
        let mut removed: Vec<DataComponentType> = Vec::with_capacity(n);
        for _ in 0..n {
            let k = DataComponentType::decode(input)?;
            if !removed.contains(&k) {
                removed.push(k);
            }
        }
        added.sort_by_key(|(k, _)| k.id());
        removed.sort_by_key(|k| k.id());
        Ok(HashedPatchMap { added_components: added, removed_components: removed })
    }
}

#[derive(Clone, Debug, PartialEq, Default)]
pub enum HashedStack {
    #[default]
    Empty,
    ActualItem { item: Item, count: i32, components: HashedPatchMap },
}

impl HashedStack {
    pub fn create(stack: &ItemStack, hasher: HashGenerator) -> HashedStack {
        if stack.is_empty() {
            HashedStack::Empty
        } else {
            HashedStack::ActualItem {
                item: stack.item(),
                count: stack.count(),
                components: HashedPatchMap::create(&stack.get_components_patch(), hasher),
            }
        }
    }

    pub fn matches(&self, stack: &ItemStack, hasher: HashGenerator) -> bool {
        match self {
            HashedStack::Empty => stack.is_empty(),
            HashedStack::ActualItem { item, count, components } => {
                *count == stack.count() && *item == stack.item() && components.matches(&stack.get_components_patch(), hasher)
            }
        }
    }
}

/// `HashedStack.STREAM_CODEC`: optional actual item.
impl Encode for HashedStack {
    fn encode(&self, out: &mut Vec<u8>) {
        match self {
            HashedStack::Empty => out.push(0),
            HashedStack::ActualItem { item, count, components } => {
                out.push(1);
                item.encode(out);
                write_varint(out, *count);
                components.encode(out);
            }
        }
    }
}

impl Decode for HashedStack {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        if !bool::decode(input)? {
            return Ok(HashedStack::Empty);
        }
        let item = crate::codec::read_registry_id::<Item>(input)?;
        let count = read_varint(input)?;
        let components = HashedPatchMap::decode(input)?;
        Ok(HashedStack::ActualItem { item, count, components })
    }
}
