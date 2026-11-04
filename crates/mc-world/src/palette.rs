//! `PalettedContainer` with vanilla's exact `Strategy`/`Configuration`/`Palette` rules
//! (`net.minecraft.world.level.chunk.{PalettedContainer, Strategy, Configuration, Palette,
//! SingleValuePalette, LinearPalette, HashMapPalette, GlobalPalette}`).
//!
//! Values are small `Copy` ids ([`PaletteValue`]): `BlockState` for block states and [`BiomeId`] for
//! biomes. Vanilla's palettes compare by identity; registered values are unique so `Eq` is the same.
//! The network format is the 26.x one: `bits: u8`, palette, then the raw longs *without* a length prefix.

use mc_core::bits::{BitStorage, SimpleBitStorage, ZeroBitStorage};
use mc_core::math::mth;
use mc_data::block::{BlockState, STATE_COUNT};
use mc_nbt::{CompoundTag, ListTag, Tag};
use std::collections::HashMap;
use std::fmt;
use std::hash::Hash;

/// A value storable in a [`PalettedContainer`]: a registered object with a global (protocol) id.
pub trait PaletteValue: Copy + Eq + Hash + fmt::Debug + Send + Sync + 'static {
    fn global_id(self) -> u32;
    /// `IdMap.byId`.
    fn from_global_id(id: u32) -> Option<Self>;
}

impl PaletteValue for BlockState {
    fn global_id(self) -> u32 {
        self.id()
    }
    fn from_global_id(id: u32) -> Option<Self> {
        BlockState::from_id(id)
    }
}

/// `Holder<Biome>`: the biome's id in the (dynamic) biome registry.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct BiomeId(pub u32);

impl PaletteValue for BiomeId {
    fn global_id(self) -> u32 {
        self.0
    }
    fn from_global_id(id: u32) -> Option<Self> {
        Some(BiomeId(id))
    }
}

/// Failure while reading/unpacking a container (vanilla throws `IllegalArgumentException`,
/// `MissingPaletteEntryException`, `IllegalStateException` or reports a `DataResult` error).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaletteError(pub String);

impl fmt::Display for PaletteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for PaletteError {}

fn err<R>(msg: impl Into<String>) -> Result<R, PaletteError> {
    Err(PaletteError(msg.into()))
}

// ------------------------------------------------------------------------------------ wire helpers

pub(crate) fn write_varint(out: &mut Vec<u8>, value: u32) {
    let mut v = value;
    while v & !0x7F != 0 {
        out.push((v & 0x7F) as u8 | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}

pub(crate) fn varint_size(value: u32) -> usize {
    (((31 - (value | 1).leading_zeros()) / 7) + 1) as usize
}

pub(crate) fn read_varint(input: &mut &[u8]) -> Result<u32, PaletteError> {
    let mut value = 0u32;
    for i in 0..5 {
        let Some((&b, rest)) = input.split_first() else { return err("Not enough bytes in buffer") };
        *input = rest;
        value |= u32::from(b & 0x7F) << (7 * i);
        if b & 0x80 == 0 {
            return Ok(value);
        }
    }
    err("VarInt too big")
}

// ------------------------------------------------------------------------------------ Strategy

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum StrategyKind {
    BlockStates,
    Biomes,
}

/// `Configuration`: how a palette + bit storage pair is laid out.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Configuration {
    /// `Configuration.Simple(SingleValuePalette, 0)`.
    Single,
    /// `Configuration.Simple(LinearPalette, bits)`.
    Linear(i32),
    /// `Configuration.Simple(HashMapPalette, bits)`.
    HashMap(i32),
    /// `Configuration.Global(bitsInMemory, bitsInStorage)`.
    Global { bits_in_memory: i32, bits_in_storage: i32 },
}

impl Configuration {
    pub fn always_repack(self) -> bool {
        matches!(self, Configuration::Global { .. })
    }

    pub fn bits_in_memory(self) -> i32 {
        match self {
            Configuration::Single => 0,
            Configuration::Linear(b) | Configuration::HashMap(b) => b,
            Configuration::Global { bits_in_memory, .. } => bits_in_memory,
        }
    }

    pub fn bits_in_storage(self) -> i32 {
        match self {
            Configuration::Global { bits_in_storage, .. } => bits_in_storage,
            other => other.bits_in_memory(),
        }
    }

    fn create_palette<T: PaletteValue>(self, entries: &[T]) -> Result<Palette<T>, PaletteError> {
        Ok(match self {
            Configuration::Single => {
                if entries.len() > 1 {
                    return err(format!("Can't initialize SingleValuePalette with {} values.", entries.len()));
                }
                Palette::Single(entries.first().copied())
            }
            Configuration::Linear(bits) => {
                let cap = 1usize << bits;
                if entries.len() > cap {
                    return err(format!(
                        "Can't initialize LinearPalette of size {cap} with {} entries",
                        entries.len()
                    ));
                }
                Palette::Linear { values: entries.to_vec(), bits }
            }
            Configuration::HashMap(bits) => Palette::hash_from(bits, entries),
            Configuration::Global { .. } => Palette::Global,
        })
    }
}

/// `Strategy<T>`: block states (4 bits/axis) or biomes (2 bits/axis) plus the global id range.
#[derive(Clone, Debug)]
pub struct Strategy<T: PaletteValue> {
    kind: StrategyKind,
    global_size: usize,
    global_palette_bits_in_memory: i32,
    bits_per_axis: u32,
    entry_count: usize,
    _marker: std::marker::PhantomData<T>,
}

impl<T: PaletteValue> Strategy<T> {
    fn new(kind: StrategyKind, global_size: usize, bits_per_axis: u32) -> Self {
        Strategy {
            kind,
            global_size,
            global_palette_bits_in_memory: mth::ceillog2(global_size as i32),
            bits_per_axis,
            entry_count: 1 << (bits_per_axis * 3),
            _marker: std::marker::PhantomData,
        }
    }

    pub fn entry_count(&self) -> usize {
        self.entry_count
    }

    /// `getIndex(x, y, z)`.
    pub fn get_index(&self, x: i32, y: i32, z: i32) -> usize {
        ((((y << self.bits_per_axis) | z) << self.bits_per_axis) | x) as usize
    }

    /// `IdMap.size()` of the global map.
    pub fn global_size(&self) -> usize {
        self.global_size
    }

    pub fn configuration_for_bit_count(&self, entry_bits: i32) -> Configuration {
        let global = Configuration::Global {
            bits_in_memory: self.global_palette_bits_in_memory,
            bits_in_storage: entry_bits,
        };
        match self.kind {
            StrategyKind::BlockStates => match entry_bits {
                0 => Configuration::Single,
                1..=4 => Configuration::Linear(4),
                5 => Configuration::HashMap(5),
                6 => Configuration::HashMap(6),
                7 => Configuration::HashMap(7),
                8 => Configuration::HashMap(8),
                _ => global,
            },
            StrategyKind::Biomes => match entry_bits {
                0 => Configuration::Single,
                1 => Configuration::Linear(1),
                2 => Configuration::Linear(2),
                3 => Configuration::Linear(3),
                _ => global,
            },
        }
    }

    pub fn configuration_for_palette_size(&self, palette_size: usize) -> Configuration {
        self.configuration_for_bit_count(mth::ceillog2(palette_size as i32))
    }
}

impl Strategy<BlockState> {
    /// `Strategy.createForBlockStates(Block.BLOCK_STATE_REGISTRY)`.
    pub fn for_block_states() -> Self {
        Self::new(StrategyKind::BlockStates, STATE_COUNT, 4)
    }
}

impl Strategy<BiomeId> {
    /// `Strategy.createForBiomes(registry.asHolderIdMap())` for a biome registry of `biome_count` entries.
    pub fn for_biomes(biome_count: usize) -> Self {
        Self::new(StrategyKind::Biomes, biome_count, 2)
    }
}

// ------------------------------------------------------------------------------------ Palette

/// Result of `Palette.idFor`: the id, or a request to grow (vanilla calls `PaletteResize.onResize`).
enum IdFor {
    Id(u32),
    Resize(i32),
}

#[derive(Clone, Debug)]
enum Palette<T: PaletteValue> {
    Single(Option<T>),
    Linear { values: Vec<T>, bits: i32 },
    /// `CrudeIncrementalIntIdentityHashBiMap`: ids are insertion order, duplicates keep the first id.
    Hash { values: Vec<T>, index: HashMap<T, u32>, bits: i32 },
    Global,
}

impl<T: PaletteValue> Palette<T> {
    fn hash_from(bits: i32, entries: &[T]) -> Self {
        let mut index = HashMap::with_capacity(entries.len());
        for (i, &v) in entries.iter().enumerate() {
            index.entry(v).or_insert(i as u32);
        }
        Palette::Hash { values: entries.to_vec(), index, bits }
    }

    fn id_for(&mut self, value: T) -> IdFor {
        match self {
            Palette::Single(slot) => match slot {
                Some(v) if *v != value => IdFor::Resize(1),
                _ => {
                    *slot = Some(value);
                    IdFor::Id(0)
                }
            },
            Palette::Linear { values, bits } => {
                if let Some(i) = values.iter().position(|v| *v == value) {
                    return IdFor::Id(i as u32);
                }
                if values.len() < (1usize << *bits) {
                    values.push(value);
                    IdFor::Id(values.len() as u32 - 1)
                } else {
                    IdFor::Resize(*bits + 1)
                }
            }
            Palette::Hash { values, index, bits } => {
                if let Some(&id) = index.get(&value) {
                    return IdFor::Id(id);
                }
                let id = values.len() as u32;
                values.push(value);
                index.insert(value, id);
                if id >= 1u32 << *bits { IdFor::Resize(*bits + 1) } else { IdFor::Id(id) }
            }
            Palette::Global => IdFor::Id(value.global_id()),
        }
    }

    /// `idFor(value, noResizeExpected())`.
    fn id_for_no_resize(&mut self, value: T) -> u32 {
        match self.id_for(value) {
            IdFor::Id(id) => id,
            IdFor::Resize(bits) => panic!("Unexpected palette resize, bits = {bits}, added value = {value:?}"),
        }
    }

    fn maybe_has(&self, pred: &mut dyn FnMut(T) -> bool) -> bool {
        match self {
            Palette::Single(v) => pred(v.expect("Use of an uninitialized palette")),
            Palette::Linear { values, .. } | Palette::Hash { values, .. } => values.iter().any(|&v| pred(v)),
            Palette::Global => true,
        }
    }

    fn value_for(&self, index: u32) -> Result<T, PaletteError> {
        let found = match self {
            Palette::Single(v) => {
                return match v {
                    Some(v) if index == 0 => Ok(*v),
                    _ => err(format!("Missing Palette entry for id {index}.")),
                };
            }
            Palette::Linear { values, .. } | Palette::Hash { values, .. } => values.get(index as usize).copied(),
            Palette::Global => T::from_global_id(index),
        };
        found.ok_or_else(|| PaletteError(format!("Missing Palette entry for index {index}.")))
    }

    fn read(&mut self, input: &mut &[u8]) -> Result<(), PaletteError> {
        fn by_id_or_throw<T: PaletteValue>(input: &mut &[u8]) -> Result<T, PaletteError> {
            let id = read_varint(input)?;
            T::from_global_id(id).ok_or_else(|| PaletteError(format!("No value with id {id}")))
        }
        match self {
            Palette::Single(v) => *v = Some(by_id_or_throw(input)?),
            Palette::Linear { values, bits } => {
                let size = read_varint(input)? as usize;
                let cap = 1usize << *bits;
                if size > cap {
                    // vanilla: ArrayIndexOutOfBoundsException on the fixed-size backing array
                    return err(format!("Index {cap} out of bounds for length {cap}"));
                }
                values.clear();
                for _ in 0..size {
                    values.push(by_id_or_throw(input)?);
                }
            }
            Palette::Hash { values, index, bits } => {
                let size = read_varint(input)?;
                let mut entries = Vec::with_capacity(size.min(1 << 16) as usize);
                for _ in 0..size {
                    entries.push(by_id_or_throw(input)?);
                }
                if let Palette::Hash { values: v, index: i, .. } = Palette::hash_from(*bits, &entries) {
                    *values = v;
                    *index = i;
                }
            }
            Palette::Global => {}
        }
        Ok(())
    }

    fn write(&self, out: &mut Vec<u8>) -> Result<(), PaletteError> {
        match self {
            Palette::Single(v) => match v {
                Some(v) => write_varint(out, v.global_id()),
                None => return err("Use of an uninitialized palette"),
            },
            Palette::Linear { values, .. } | Palette::Hash { values, .. } => {
                write_varint(out, values.len() as u32);
                for v in values {
                    write_varint(out, v.global_id());
                }
            }
            Palette::Global => {}
        }
        Ok(())
    }

    fn serialized_size(&self) -> Result<usize, PaletteError> {
        Ok(match self {
            Palette::Single(Some(v)) => varint_size(v.global_id()),
            Palette::Single(None) => return err("Use of an uninitialized palette"),
            Palette::Linear { values, .. } | Palette::Hash { values, .. } => {
                varint_size(values.len() as u32) + values.iter().map(|v| varint_size(v.global_id())).sum::<usize>()
            }
            Palette::Global => 0,
        })
    }

    fn size(&self, global_size: usize) -> usize {
        match self {
            Palette::Single(_) => 1,
            Palette::Linear { values, .. } | Palette::Hash { values, .. } => values.len(),
            Palette::Global => global_size,
        }
    }

    /// `HashMapPalette.getEntries()`.
    fn entries(&self) -> Vec<T> {
        match self {
            Palette::Single(v) => v.iter().copied().collect(),
            Palette::Linear { values, .. } | Palette::Hash { values, .. } => values.clone(),
            Palette::Global => Vec::new(),
        }
    }
}

// ------------------------------------------------------------------------------------ container

struct Data<T: PaletteValue> {
    configuration: Configuration,
    storage: Box<dyn BitStorage>,
    palette: Palette<T>,
}

impl<T: PaletteValue> Data<T> {
    fn copy(&self) -> Self {
        Data { configuration: self.configuration, storage: self.storage.copy(), palette: self.palette.clone() }
    }

    /// `Data.copyFrom(oldPalette, oldStorage)`.
    fn copy_from(&mut self, old_palette: &Palette<T>, old_storage: &dyn BitStorage, global_size: usize) {
        if old_palette.size(global_size) == 1 {
            let value = old_palette.value_for(0).expect("single-value palette is initialized");
            let id = self.palette.id_for_no_resize(value);
            self.storage.fill(id as i32);
        } else {
            let mut last_old_id = -1;
            let mut new_id = -1;
            for i in 0..old_storage.get_size() {
                let old_id = old_storage.get(i);
                if old_id != last_old_id {
                    let value = old_palette.value_for(old_id as u32).expect("palette entry exists");
                    new_id = self.palette.id_for_no_resize(value) as i32;
                    last_old_id = old_id;
                }
                self.storage.set(i, new_id);
            }
        }
    }
}

/// `PalettedContainer.PackedData`: palette entries and (when more than one entry) the packed storage.
#[derive(Clone, Debug, PartialEq)]
pub struct PackedData<T> {
    pub palette_entries: Vec<T>,
    pub storage: Option<Vec<i64>>,
    /// `-1` = unknown.
    pub bits_per_entry: i32,
}

pub struct PalettedContainer<T: PaletteValue> {
    data: Data<T>,
    strategy: Strategy<T>,
}

impl<T: PaletteValue> Clone for PalettedContainer<T> {
    fn clone(&self) -> Self {
        PalettedContainer { data: self.data.copy(), strategy: self.strategy.clone() }
    }
}

impl<T: PaletteValue> fmt::Debug for PalettedContainer<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PalettedContainer").field("config", &self.data.configuration).finish()
    }
}

fn new_storage(config: Configuration, entry_count: usize) -> Box<dyn BitStorage> {
    match config.bits_in_memory() {
        0 => Box::new(ZeroBitStorage::new(entry_count)),
        bits => Box::new(SimpleBitStorage::new(bits, entry_count)),
    }
}

impl<T: PaletteValue> PalettedContainer<T> {
    /// `new PalettedContainer(initialValue, strategy)`.
    pub fn new(initial_value: T, strategy: Strategy<T>) -> Self {
        let mut c = PalettedContainer { data: Self::create_data(&strategy, None, 0), strategy };
        c.data.palette.id_for_no_resize(initial_value);
        c
    }

    /// `createOrReuseData(oldData, targetBits)` (the reuse case is handled by the callers that can).
    fn create_data(strategy: &Strategy<T>, old: Option<&Data<T>>, target_bits: i32) -> Data<T> {
        let config = strategy.configuration_for_bit_count(target_bits);
        if let Some(old) = old
            && old.configuration == config
        {
            return old.copy();
        }
        let palette = config.create_palette(&[]).expect("empty palette always fits");
        Data { configuration: config, storage: new_storage(config, strategy.entry_count), palette }
    }

    /// `onResize(bits, lastAddedValue)`.
    fn on_resize(&mut self, bits: i32, last_added: T) -> u32 {
        // (a resize always changes the configuration, so vanilla's "reuse old data" branch never applies)
        let mut new_data = Self::create_data(&self.strategy, None, bits);
        new_data.copy_from(&self.data.palette, self.data.storage.as_ref(), self.strategy.global_size);
        self.data = new_data;
        self.data.palette.id_for_no_resize(last_added)
    }

    fn id_for(&mut self, value: T) -> u32 {
        match self.data.palette.id_for(value) {
            IdFor::Id(id) => id,
            IdFor::Resize(bits) => self.on_resize(bits, value),
        }
    }

    pub fn strategy(&self) -> &Strategy<T> {
        &self.strategy
    }

    /// `getAndSet(x, y, z, value)`.
    pub fn get_and_set(&mut self, x: i32, y: i32, z: i32, value: T) -> T {
        let index = self.strategy.get_index(x, y, z);
        self.get_and_set_index(index, value)
    }

    fn get_and_set_index(&mut self, index: usize, value: T) -> T {
        let id = self.id_for(value);
        let old_id = self.data.storage.get_and_set(index, id as i32);
        self.data.palette.value_for(old_id as u32).expect("stored id is in the palette")
    }

    pub fn set(&mut self, x: i32, y: i32, z: i32, value: T) {
        let index = self.strategy.get_index(x, y, z);
        let id = self.id_for(value);
        self.data.storage.set(index, id as i32);
    }

    pub fn get(&self, x: i32, y: i32, z: i32) -> T {
        self.get_index(self.strategy.get_index(x, y, z))
    }

    pub fn get_index(&self, index: usize) -> T {
        self.data.palette.value_for(self.data.storage.get(index) as u32).expect("stored id is in the palette")
    }

    /// `getAll(consumer)`: every distinct value in the storage (ascending palette id).
    pub fn get_all(&self, mut consumer: impl FnMut(T)) {
        let mut seen: Vec<i32> = Vec::new(); // IntArraySet: insertion-ordered
        self.data.storage.get_all(&mut |id| {
            if !seen.contains(&id) {
                seen.push(id);
            }
        });
        for id in seen {
            consumer(self.data.palette.value_for(id as u32).expect("stored id is in the palette"));
        }
    }

    /// `read(buffer)` (network format).
    pub fn read(&mut self, input: &mut &[u8]) -> Result<(), PaletteError> {
        let Some((&new_bits, rest)) = input.split_first() else { return err("Not enough bytes in buffer") };
        *input = rest;
        let new_bits = new_bits as i8 as i32;
        let mut new_data = Self::create_data(&self.strategy, Some(&self.data), new_bits);
        new_data.palette.read(input)?;
        let longs = new_data.storage.get_raw().len();
        let mut raw = Vec::with_capacity(longs);
        for _ in 0..longs {
            let Some((chunk, rest)) = input.split_first_chunk::<8>() else { return err("Not enough bytes in buffer") };
            *input = rest;
            raw.push(i64::from_be_bytes(*chunk));
        }
        if longs > 0 {
            new_data.storage = Box::new(
                SimpleBitStorage::with_data(new_data.configuration.bits_in_memory(), self.strategy.entry_count, raw)
                    .map_err(|e| PaletteError(e.0))?,
            );
        }
        self.data = new_data;
        Ok(())
    }

    /// `write(buffer)` (network format).
    pub fn write(&self, out: &mut Vec<u8>) -> Result<(), PaletteError> {
        out.push(self.data.storage.get_bits() as u8);
        self.data.palette.write(out)?;
        for l in self.data.storage.get_raw() {
            out.extend_from_slice(&l.to_be_bytes());
        }
        Ok(())
    }

    /// `getSerializedSize()`.
    pub fn serialized_size(&self) -> Result<usize, PaletteError> {
        Ok(1 + self.data.palette.serialized_size()? + self.data.storage.get_raw().len() * 8)
    }

    /// `bitsPerEntry()`.
    pub fn bits_per_entry(&self) -> i32 {
        self.data.storage.get_bits()
    }

    pub fn maybe_has(&self, mut pred: impl FnMut(T) -> bool) -> bool {
        self.data.palette.maybe_has(&mut pred)
    }

    /// `forEachInPalette(consumer)`.
    pub fn for_each_in_palette(&self, mut consumer: impl FnMut(T)) {
        for i in 0..self.data.palette.size(self.strategy.global_size) {
            consumer(self.data.palette.value_for(i as u32).expect("palette entry exists"));
        }
    }

    /// `recreate()`: an empty container filled with this one's first palette value.
    pub fn recreate(&self) -> Self {
        Self::new(self.data.palette.value_for(0).expect("palette entry 0 exists"), self.strategy.clone())
    }

    /// `count(output)`: `output(value, count)` for each distinct value.
    pub fn count(&self, mut output: impl FnMut(T, usize)) {
        if self.data.palette.size(self.strategy.global_size) == 1 {
            output(self.data.palette.value_for(0).expect("palette entry 0 exists"), self.data.storage.get_size());
        } else {
            let mut counts: HashMap<i32, usize> = HashMap::new();
            self.data.storage.get_all(&mut |id| *counts.entry(id).or_default() += 1);
            for (id, n) in counts {
                output(self.data.palette.value_for(id as u32).expect("stored id is in the palette"), n);
            }
        }
    }

    /// `reencodeContents(storage, oldPalette, newPalette)`.
    fn reencode(storage: &dyn BitStorage, old: &Palette<T>, new: &mut Palette<T>) -> Result<Vec<i32>, PaletteError> {
        let mut buffer = vec![0i32; storage.get_size()];
        storage.unpack(&mut buffer);
        let (mut last_read, mut last_written) = (-1, -1);
        for slot in buffer.iter_mut() {
            let id = *slot;
            if id != last_read {
                last_read = id;
                last_written = new.id_for_no_resize(old.value_for(id as u32)?) as i32;
            }
            *slot = last_written;
        }
        Ok(buffer)
    }

    /// `pack(strategy)`: the on-disk form.
    pub fn pack(&self) -> PackedData<T> {
        let storage = self.data.storage.as_ref();
        let mut new_palette = Palette::hash_from(storage.get_bits(), &[]);
        let contents = Self::reencode(storage, &self.data.palette, &mut new_palette)
            .expect("in-memory container only holds palette ids");
        let config = self.strategy.configuration_for_palette_size(new_palette.size(self.strategy.global_size));
        let bits_on_disc = config.bits_in_storage();
        let values = (bits_on_disc != 0).then(|| {
            SimpleBitStorage::from_values(bits_on_disc, self.strategy.entry_count, &contents).get_raw().to_vec()
        });
        PackedData { palette_entries: new_palette.entries(), storage: values, bits_per_entry: bits_on_disc }
    }

    /// `PalettedContainer.unpack(strategy, packedData)`.
    pub fn unpack(strategy: Strategy<T>, packed: PackedData<T>) -> Result<Self, PaletteError> {
        let entries = &packed.palette_entries;
        let entry_count = strategy.entry_count;
        let stored = strategy.configuration_for_palette_size(entries.len());
        let bits_on_disc = stored.bits_in_storage();
        if packed.bits_per_entry != -1 && bits_on_disc != packed.bits_per_entry {
            return err(format!(
                "Invalid bit count, calculated {bits_on_disc}, but container declared {}",
                packed.bits_per_entry
            ));
        }
        let (storage, palette): (Box<dyn BitStorage>, Palette<T>) = if stored.bits_in_memory() == 0 {
            // an empty entry list leaves the single-value palette uninitialised: reading it would throw
            let palette = stored.create_palette(entries)?;
            if entries.is_empty() {
                return err("Missing Palette entry for id 0.");
            }
            (Box::new(ZeroBitStorage::new(entry_count)), palette)
        } else {
            let Some(data) = packed.storage.clone() else { return err("Missing values for non-zero storage") };
            let init = |e: mc_core::bits::InitializationException| PaletteError(format!("Failed to read PalettedContainer: {}", e.0));
            if !stored.always_repack() && stored.bits_in_memory() == bits_on_disc {
                let storage = SimpleBitStorage::with_data(stored.bits_in_memory(), entry_count, data).map_err(init)?;
                // vanilla would throw MissingPaletteEntryException on first access of a dangling id
                if entries.len() < (1usize << bits_on_disc) {
                    let mut ids = vec![0i32; entry_count];
                    storage.unpack(&mut ids);
                    if ids.iter().any(|&id| id as usize >= entries.len()) {
                        return err("Missing Palette entry for index (id outside the palette)");
                    }
                }
                (Box::new(storage), stored.create_palette(entries)?)
            } else {
                let old_palette = Palette::hash_from(bits_on_disc, entries);
                let old_storage = SimpleBitStorage::with_data(bits_on_disc, entry_count, data).map_err(init)?;
                let mut new_palette = stored.create_palette(entries)?;
                let contents = Self::reencode(&old_storage, &old_palette, &mut new_palette)?;
                let storage = SimpleBitStorage::from_values(stored.bits_in_memory(), entry_count, &contents);
                (Box::new(storage), new_palette)
            }
        };
        Ok(PalettedContainer { data: Data { configuration: stored, storage, palette }, strategy })
    }
}

// ------------------------------------------------------------------------------------ NBT codec

/// Element codec of a palette entry (`BlockState.CODEC`, `biomes.holderByNameCodec()`).
pub trait ElementCodec<T>: Send + Sync {
    /// Decode one palette entry; `Err` makes the entry fall back to the default value (partial result).
    fn decode(&self, tag: &Tag) -> Result<T, String>;
    fn encode(&self, value: T) -> Result<Tag, String>;
}

/// `LONG_STREAM` as decoded by `NbtOps`: a long array, or a list of numbers.
fn decode_long_stream(tag: &Tag) -> Option<Vec<i64>> {
    match tag {
        Tag::LongArray(l) => Some(l.clone()),
        Tag::IntArray(l) => Some(l.iter().map(|&v| i64::from(v)).collect()),
        Tag::ByteArray(l) => Some(l.iter().map(|&v| i64::from(v)).collect()),
        Tag::List(l) => l.iter().map(|t| t.as_number().map(|n| n.long_value())).collect(),
        _ => None,
    }
}

/// Result of decoding a container from NBT: the container plus recoverable problems
/// (vanilla `promotePartial(msg -> logErrors(...))`).
pub struct Unpacked<T: PaletteValue> {
    pub container: PalettedContainer<T>,
    pub problems: Vec<String>,
}

impl<T: PaletteValue> PalettedContainer<T> {
    /// `PalettedContainer.codec(elementCodec, strategy, defaultValue)` decode.
    pub fn from_nbt(
        tag: &Tag,
        element_codec: &dyn ElementCodec<T>,
        strategy: Strategy<T>,
        default_value: T,
    ) -> Result<Unpacked<T>, PaletteError> {
        let Some(compound) = tag.as_compound() else { return err("Not a map: expected a compound") };
        let Some(Tag::List(palette_tag)) = compound.get("palette") else {
            return err("No key palette in MapLike[...]");
        };
        let mut problems = Vec::new();
        let mut entries = Vec::with_capacity(palette_tag.len());
        for element in palette_tag {
            match element_codec.decode(element) {
                Ok(v) => entries.push(v),
                Err(msg) => {
                    problems.push(format!("(({msg}) -> using default)"));
                    entries.push(default_value);
                }
            }
        }
        // lenientOptionalFieldOf("data"): an undecodable value counts as absent
        let storage = compound.get("data").and_then(decode_long_stream);
        let container =
            Self::unpack(strategy, PackedData { palette_entries: entries, storage, bits_per_entry: -1 })?;
        Ok(Unpacked { container, problems })
    }

    /// Codec encode: `{palette: [...], data?: long[]}`.
    pub fn to_nbt(&self, element_codec: &dyn ElementCodec<T>) -> Result<CompoundTag, PaletteError> {
        let packed = self.pack();
        let mut palette = ListTag::with_capacity(packed.palette_entries.len());
        for v in packed.palette_entries {
            palette.push(element_codec.encode(v).map_err(PaletteError)?);
        }
        let mut out = CompoundTag::new();
        out.put("palette", palette);
        if let Some(data) = packed.storage {
            out.put_long_array("data", data);
        }
        Ok(out)
    }
}

// ------------------------------------------------------------------------------------ factory

/// `BlockState.CODEC`: a bare block name (default state) or `{id, properties?}`.
pub struct BlockStateCodec;

impl ElementCodec<BlockState> for BlockStateCodec {
    fn decode(&self, tag: &Tag) -> Result<BlockState, String> {
        use mc_data::block::Block;
        let by_name = |name: &str| -> Result<Block, String> {
            mc_core::ident::Identifier::parse(name)
                .ok()
                .and_then(|id| Block::by_name(&id.to_string()))
                .ok_or_else(|| format!("Unknown registry key in minecraft:block: {name}"))
        };
        match tag {
            Tag::String(name) => by_name(name).map(Block::default_state),
            Tag::Compound(c) => {
                let Some(Tag::String(name)) = c.get("id") else { return Err("No key id in MapLike[...]".into()) };
                let block = by_name(name)?;
                let mut state = block.default_state();
                // lenientOptionalFieldOf("properties") + per-property orElseGet(default)
                if let Some(Tag::Compound(props)) = c.get("properties") {
                    for def in block.properties() {
                        if let Some(Tag::String(value)) = props.get(def.name)
                            && let Some(s) = state.with_property_str(def.name, value)
                        {
                            state = s;
                        }
                    }
                }
                Ok(state)
            }
            _ => Err("Not a string or map".into()),
        }
    }

    fn encode(&self, state: BlockState) -> Result<Tag, String> {
        let block = state.block();
        if state == block.default_state() {
            return Ok(Tag::String(block.name().to_owned()));
        }
        let mut props = CompoundTag::new();
        for (def, i) in state.values() {
            props.put_string(def.name, def.values[i]);
        }
        let mut out = CompoundTag::new();
        out.put_string("id", block.name());
        out.put("properties", props);
        Ok(Tag::Compound(out))
    }
}

/// The dynamic biome registry (`Registry<Biome>` from the world's data packs): ids are positions in `names`.
#[derive(Debug, Clone)]
pub struct BiomeRegistry {
    names: Vec<String>,
    by_name: HashMap<String, u32>,
}

impl BiomeRegistry {
    pub fn new(names: Vec<String>) -> Self {
        let by_name = names.iter().enumerate().map(|(i, n)| (n.clone(), i as u32)).collect();
        BiomeRegistry { names, by_name }
    }

    pub fn len(&self) -> usize {
        self.names.len()
    }

    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    pub fn id_of(&self, name: &str) -> Option<BiomeId> {
        let full = if name.contains(':') { name.to_owned() } else { format!("minecraft:{name}") };
        self.by_name.get(&full).map(|&i| BiomeId(i))
    }

    pub fn name_of(&self, id: BiomeId) -> Option<&str> {
        self.names.get(id.0 as usize).map(String::as_str)
    }
}

impl ElementCodec<BiomeId> for BiomeRegistry {
    fn decode(&self, tag: &Tag) -> Result<BiomeId, String> {
        let Tag::String(name) = tag else { return Err("Not a string".into()) };
        let valid = mc_core::ident::Identifier::parse(name).ok();
        valid
            .and_then(|id| self.id_of(&id.to_string()))
            .ok_or_else(|| format!("Unknown registry key in minecraft:worldgen/biome: {name}"))
    }

    fn encode(&self, value: BiomeId) -> Result<Tag, String> {
        self.name_of(value)
            .map(|n| Tag::String(n.to_owned()))
            .ok_or_else(|| format!("Unregistered holder in minecraft:worldgen/biome: {value:?}"))
    }
}

/// `PalettedContainerFactory`: both strategies, the defaults and the codecs for one registry set.
#[derive(Debug, Clone)]
pub struct PalettedContainerFactory {
    pub block_states_strategy: Strategy<BlockState>,
    pub default_block_state: BlockState,
    pub biome_strategy: Strategy<BiomeId>,
    pub default_biome: BiomeId,
    pub biomes: BiomeRegistry,
}

impl PalettedContainerFactory {
    /// `PalettedContainerFactory.create(registries)`; `plains` must be registered.
    pub fn create(biomes: BiomeRegistry) -> Option<Self> {
        let default_biome = biomes.id_of("minecraft:plains")?;
        Some(PalettedContainerFactory {
            block_states_strategy: Strategy::for_block_states(),
            default_block_state: mc_data::block::Block::AIR.default_state(),
            biome_strategy: Strategy::for_biomes(biomes.len()),
            default_biome,
            biomes,
        })
    }

    pub fn create_for_block_states(&self) -> PalettedContainer<BlockState> {
        PalettedContainer::new(self.default_block_state, self.block_states_strategy.clone())
    }

    pub fn create_for_biomes(&self) -> PalettedContainer<BiomeId> {
        PalettedContainer::new(self.default_biome, self.biome_strategy.clone())
    }

    pub fn block_states_from_nbt(&self, tag: &Tag) -> Result<Unpacked<BlockState>, PaletteError> {
        PalettedContainer::from_nbt(
            tag,
            &BlockStateCodec,
            self.block_states_strategy.clone(),
            self.default_block_state,
        )
    }

    pub fn biomes_from_nbt(&self, tag: &Tag) -> Result<Unpacked<BiomeId>, PaletteError> {
        PalettedContainer::from_nbt(tag, &self.biomes, self.biome_strategy.clone(), self.default_biome)
    }
}
