//! Client-side world state: chunk storage (block state ids per section, biome ids) decoded from the
//! chunk packet buffer, and block lookups for collision.
use std::collections::HashMap;

use mc_core::pos::block_pos::BlockPos;
use mc_data::block::BlockState;
use mc_data::Block;
use mc_protocol::codec::{read_varint, take, DecodeError};

/// Palette strategy of a `PalettedContainer` (vanilla `PalettedContainer.Strategy`).
#[derive(Clone, Copy)]
enum Strategy {
    /// Block states of a section: 4096 entries, linear palette up to 4 bits, hash palette up to 8, else global.
    States,
    /// Biomes of a section: 64 entries, linear palette up to 3 bits, else global with `global_bits`.
    Biomes { global_bits: u8 },
}

impl Strategy {
    fn entries(self) -> usize {
        match self {
            Strategy::States => 4096,
            Strategy::Biomes { .. } => 64,
        }
    }
}

/// `Mth.ceillog2`
fn ceil_log2(n: usize) -> u8 {
    if n <= 1 { 0 } else { (usize::BITS - (n - 1).leading_zeros()) as u8 }
}

/// A decoded paletted container: either one value or one id per entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Container {
    Single(u16),
    Data(Box<[u16]>),
}

impl Container {
    fn get(&self, index: usize) -> u16 {
        match self {
            Container::Single(v) => *v,
            Container::Data(d) => d[index],
        }
    }

    fn set(&mut self, index: usize, value: u16, entries: usize) {
        match self {
            Container::Single(v) if *v == value => {}
            Container::Single(v) => {
                let mut d = vec![*v; entries].into_boxed_slice();
                d[index] = value;
                *self = Container::Data(d);
            }
            Container::Data(d) => d[index] = value,
        }
    }
}

/// `PalettedContainer.read`: byte bits, palette, fixed-size long array; returns global ids.
fn read_container(input: &mut &[u8], strategy: Strategy) -> Result<Container, DecodeError> {
    let wire_bits = take(input, 1)?[0];
    let entries = strategy.entries();
    // (bits in storage, has palette list)
    let (bits, palette_kind) = match (strategy, wire_bits) {
        (_, 0) => (0u8, 0),
        (Strategy::States, 1..=4) => (4, 1),
        (Strategy::States, 5..=8) => (wire_bits, 1),
        (Strategy::States, _) => (ceil_log2(mc_data::block::STATE_COUNT), 2),
        (Strategy::Biomes { .. }, 1..=3) => (wire_bits, 1),
        (Strategy::Biomes { global_bits }, _) => (global_bits, 2),
    };
    let palette: Vec<u16> = match palette_kind {
        0 => vec![read_varint(input)? as u16],
        1 => {
            let n = read_varint(input)?;
            if n < 0 {
                return Err(DecodeError::msg(format!("negative palette size {n}")));
            }
            (0..n).map(|_| read_varint(input).map(|v| v as u16)).collect::<Result<_, _>>()?
        }
        _ => Vec::new(),
    };
    if bits == 0 {
        return Ok(Container::Single(palette[0]));
    }
    let per_long = 64 / bits as usize;
    let longs = entries.div_ceil(per_long);
    let raw = take(input, longs * 8)?;
    let mask = (1u64 << bits) - 1;
    let mut out = vec![0u16; entries];
    for (i, o) in out.iter_mut().enumerate() {
        let word = u64::from_be_bytes(raw[(i / per_long) * 8..(i / per_long) * 8 + 8].try_into().expect("8 bytes"));
        let v = (word >> ((i % per_long) * bits as usize) & mask) as usize;
        *o = if palette_kind == 2 { v as u16 } else { *palette.get(v).ok_or_else(|| DecodeError::msg("palette index out of range"))? };
    }
    Ok(Container::Data(out.into_boxed_slice()))
}

#[derive(Clone, Debug)]
pub struct Section {
    pub non_empty_blocks: i16,
    pub fluid_count: i16,
    pub blocks: Container,
    pub biomes: Container,
}

#[derive(Clone, Debug)]
pub struct Chunk {
    pub sections: Vec<Section>,
}

#[derive(Debug)]
pub struct ClientWorld {
    pub min_y: i32,
    pub height: i32,
    /// Entries of the biome registry (width of the global biome palette).
    pub biome_count: usize,
    chunks: HashMap<(i32, i32), Chunk>,
}

impl ClientWorld {
    pub fn new(min_y: i32, height: i32, biome_count: usize) -> Self {
        Self { min_y, height, biome_count, chunks: HashMap::new() }
    }

    pub fn section_count(&self) -> usize {
        (self.height >> 4) as usize
    }

    /// Decode a chunk packet buffer (`LevelChunkPacketData.buffer`) and store it.
    pub fn load_chunk(&mut self, x: i32, z: i32, buffer: &[u8]) -> Result<(), DecodeError> {
        let mut input = buffer;
        let global_bits = ceil_log2(self.biome_count);
        let mut sections = Vec::with_capacity(self.section_count());
        for _ in 0..self.section_count() {
            let non_empty_blocks = i16::from_be_bytes(take(&mut input, 2)?.try_into().expect("2 bytes"));
            let fluid_count = i16::from_be_bytes(take(&mut input, 2)?.try_into().expect("2 bytes"));
            let blocks = read_container(&mut input, Strategy::States)?;
            let biomes = read_container(&mut input, Strategy::Biomes { global_bits })?;
            sections.push(Section { non_empty_blocks, fluid_count, blocks, biomes });
        }
        self.chunks.insert((x, z), Chunk { sections });
        Ok(())
    }

    /// Store an all-air chunk (synthetic worlds in tests).
    pub fn insert_air_chunk(&mut self, x: i32, z: i32) {
        let air = Block::AIR.default_state().id() as u16;
        let section = Section { non_empty_blocks: 0, fluid_count: 0, blocks: Container::Single(air), biomes: Container::Single(0) };
        self.chunks.insert((x, z), Chunk { sections: vec![section; self.section_count()] });
    }

    pub fn unload_chunk(&mut self, x: i32, z: i32) {
        self.chunks.remove(&(x, z));
    }

    pub fn has_chunk(&self, x: i32, z: i32) -> bool {
        self.chunks.contains_key(&(x, z))
    }

    pub fn has_chunk_at_block(&self, x: i32, z: i32) -> bool {
        self.has_chunk(x >> 4, z >> 4)
    }

    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }

    pub fn loaded_chunks(&self) -> impl Iterator<Item = (i32, i32)> + '_ {
        self.chunks.keys().copied()
    }

    fn locate(&self, x: i32, y: i32, z: i32) -> Option<(&Chunk, usize, usize)> {
        let chunk = self.chunks.get(&(x >> 4, z >> 4))?;
        let rel_y = y - self.min_y;
        if rel_y < 0 || rel_y >= self.height {
            return None;
        }
        Some((chunk, (rel_y >> 4) as usize, ((y & 15) << 8 | (z & 15) << 4 | (x & 15)) as usize))
    }

    /// Global block state id; `None` when the chunk is not loaded. Outside the build height: void air.
    pub fn block_state_id(&self, x: i32, y: i32, z: i32) -> Option<u16> {
        if !self.has_chunk_at_block(x, z) {
            return None;
        }
        Some(match self.locate(x, y, z) {
            Some((chunk, section, index)) => chunk.sections[section].blocks.get(index),
            None => Block::VOID_AIR.default_state().id() as u16,
        })
    }

    pub fn block_state(&self, x: i32, y: i32, z: i32) -> Option<BlockState> {
        self.block_state_id(x, y, z).and_then(|id| BlockState::from_id(id as u32))
    }

    pub fn block_state_at(&self, pos: BlockPos) -> Option<BlockState> {
        self.block_state(pos.x(), pos.y(), pos.z())
    }

    /// Returns false if the position is not in a loaded chunk / inside the build height.
    pub fn set_block_state_id(&mut self, x: i32, y: i32, z: i32, id: u16) -> bool {
        let rel_y = y - self.min_y;
        if rel_y < 0 || rel_y >= self.height {
            return false;
        }
        let (section, index) = ((rel_y >> 4) as usize, ((y & 15) << 8 | (z & 15) << 4 | (x & 15)) as usize);
        let Some(chunk) = self.chunks.get_mut(&(x >> 4, z >> 4)) else { return false };
        chunk.sections[section].blocks.set(index, id, 4096);
        true
    }

    /// Highest non-air block y at the column (scan from the top), if the chunk is loaded.
    pub fn top_solid_y(&self, x: i32, z: i32) -> Option<i32> {
        if !self.has_chunk_at_block(x, z) {
            return None;
        }
        (self.min_y..self.min_y + self.height).rev().find(|&y| {
            self.block_state(x, y, z).is_some_and(|s| !s.block().is_air() && !s.collision_shape().is_empty())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pack `values` with `bits` per entry like `SimpleBitStorage`.
    fn pack(values: &[u16], bits: usize) -> Vec<u8> {
        let per = 64 / bits;
        let mut out = Vec::new();
        for chunk in values.chunks(per) {
            let mut w = 0u64;
            for (i, v) in chunk.iter().enumerate() {
                w |= (*v as u64) << (i * bits);
            }
            out.extend_from_slice(&w.to_be_bytes());
        }
        out
    }

    fn varint(out: &mut Vec<u8>, v: i32) {
        mc_protocol::codec::write_varint(out, v);
    }

    #[test]
    fn decodes_linear_hashed_global_and_single_containers() {
        let values: Vec<u16> = (0..4096).map(|i| (i % 5) as u16).collect();
        // linear palette, 4 bits
        let mut buf = vec![4u8];
        varint(&mut buf, 5);
        for id in [10, 20, 30, 40, 50] {
            varint(&mut buf, id);
        }
        buf.extend(pack(&values, 4));
        let c = read_container(&mut &buf[..], Strategy::States).unwrap();
        assert_eq!(c.get(7), 30);
        assert_eq!(c.get(4095), 10 + 10 * (4095 % 5) as u16);
        // hash palette, 6 bits
        let mut buf = vec![6u8];
        varint(&mut buf, 5);
        for id in [1, 2, 3, 4, 5] {
            varint(&mut buf, id);
        }
        buf.extend(pack(&values, 6));
        let c = read_container(&mut &buf[..], Strategy::States).unwrap();
        assert_eq!(c.get(3), 4);
        // global, 16 bits, ids direct
        let global: Vec<u16> = (0..4096).map(|i| (i * 7 % 35000) as u16).collect();
        let mut buf = vec![15u8];
        buf.extend(pack(&global, 16));
        let mut rest = &buf[..];
        let c = read_container(&mut rest, Strategy::States).unwrap();
        assert!(rest.is_empty());
        assert_eq!(c.get(1234), global[1234]);
        // single value
        let mut buf = vec![0u8];
        varint(&mut buf, 77);
        assert_eq!(read_container(&mut &buf[..], Strategy::States).unwrap(), Container::Single(77));
        // biomes: single and 7-bit global (88 biomes -> 7 bits)
        let mut buf = vec![0u8];
        varint(&mut buf, 3);
        assert_eq!(read_container(&mut &buf[..], Strategy::Biomes { global_bits: 7 }).unwrap(), Container::Single(3));
        let bio: Vec<u16> = (0..64).map(|i| i as u16).collect();
        let mut buf = vec![7u8];
        buf.extend(pack(&bio, 7));
        let c = read_container(&mut &buf[..], Strategy::Biomes { global_bits: 7 }).unwrap();
        assert_eq!(c.get(63), 63);
    }

    #[test]
    fn world_lookup_and_set() {
        let mut w = ClientWorld::new(0, 16, 64);
        let mut buf = vec![0, 1, 0, 0, 0];
        varint(&mut buf, 1); // stone-ish id 1, single value
        buf.extend([0u8, 0]); // biomes single value 0
        w.load_chunk(0, 0, &buf).unwrap();
        assert_eq!(w.block_state_id(3, 5, 9), Some(1));
        assert_eq!(w.block_state_id(16, 5, 9), None);
        assert!(w.set_block_state_id(3, 5, 9, 2));
        assert_eq!(w.block_state_id(3, 5, 9), Some(2));
        assert_eq!(w.block_state_id(3, 5, 8), Some(1));
        assert_eq!(w.block_state_id(-1, 5, 8), None);
    }
}
