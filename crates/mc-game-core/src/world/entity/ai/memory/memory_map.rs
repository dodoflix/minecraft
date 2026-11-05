//! `MemoryMap`: the persisted memories of a brain (`Brain.Packed`).

use super::memory_module_type::{MemoryModuleType, MemoryTypeId};
use super::value::{MemoryData, MemoryValue};
use super::ExpirableValue;
use mc_nbt::CompoundTag;

#[derive(Clone, PartialEq, Debug, Default)]
pub struct MemoryMap {
    memories: Vec<(MemoryTypeId, ExpirableValue<MemoryValue>)>,
}

impl MemoryMap {
    /// `MemoryMap.EMPTY`.
    pub const EMPTY: MemoryMap = MemoryMap { memories: Vec::new() };

    /// `MemoryMap.Builder.add` (a later value of the same type replaces the earlier one).
    pub fn insert(&mut self, type_id: MemoryTypeId, value: ExpirableValue<MemoryValue>) {
        match self.memories.iter_mut().find(|(t, _)| *t == type_id) {
            Some(entry) => entry.1 = value,
            None => self.memories.push((type_id, value)),
        }
    }

    /// Typed `Builder.add(type, value)`.
    pub fn add<U: MemoryData>(mut self, type_: MemoryModuleType<U>, value: U, time_to_live: Option<i64>) -> Self {
        self.insert(type_.id(), ExpirableValue { value: value.into_value(), time_to_live });
        self
    }

    /// `get(type)`.
    pub fn get<U: MemoryData>(&self, type_: &MemoryModuleType<U>) -> Option<ExpirableValue<&U>> {
        let (_, v) = self.memories.iter().find(|(t, _)| *t == type_.id())?;
        Some(ExpirableValue { value: U::from_value(&v.value)?, time_to_live: v.time_to_live })
    }

    pub fn iter(&self) -> impl Iterator<Item = (MemoryTypeId, &ExpirableValue<MemoryValue>)> {
        self.memories.iter().map(|(t, v)| (*t, v))
    }

    pub fn len(&self) -> usize {
        self.memories.len()
    }

    pub fn is_empty(&self) -> bool {
        self.memories.is_empty()
    }
}

/// `Brain.Packed`.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct BrainPacked {
    pub memories: MemoryMap,
}

impl BrainPacked {
    /// `Brain.Packed.EMPTY`.
    pub const EMPTY: BrainPacked = BrainPacked { memories: MemoryMap::EMPTY };

    /// `Brain.Packed.CODEC` encode: `{memories: {...}}`.
    pub fn to_nbt(&self) -> CompoundTag {
        let mut c = CompoundTag::new();
        c.put("memories", self.memories.to_nbt());
        c
    }

    /// `Brain.Packed.CODEC` decode (`ValueInput.read("Brain", ..)`): a missing/invalid `memories` field decodes to nothing
    /// (the brain stays as it was).
    pub fn from_nbt(tag: &CompoundTag) -> Option<BrainPacked> {
        tag.get_compound("memories").map(|m| BrainPacked { memories: MemoryMap::from_nbt(m) })
    }
}
