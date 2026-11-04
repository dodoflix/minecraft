//! `ClientboundLevelChunkPacketData` and `ClientboundLightUpdatePacketData` payload encoding
//! (`net.minecraft.network.protocol.game`), plus the body of `ClientboundLevelChunkWithLightPacket`.
//! Output is the packet *body* (no packet id / framing) so `mc-protocol` can embed it verbatim.

use crate::data_layer::{self, DataLayer};
use crate::heightmap::HeightmapType;
use crate::palette::{PaletteError, read_varint, write_varint};
use crate::section::LevelChunkSection;
use mc_data::BlockEntityType;
use mc_nbt::{CompoundTag, NbtAccounter, Tag, nbt_io};
use std::collections::BTreeMap;

/// `ByteBufCodecs.byteArray(2097152)` limit of the chunk section buffer.
pub const MAX_CHUNK_BUFFER: usize = 2_097_152;

/// `ClientboundLevelChunkPacketData.BlockEntityInfo`.
#[derive(Clone, Debug, PartialEq)]
pub struct BlockEntityInfo {
    /// `sectionRelative(x) << 4 | sectionRelative(z)`.
    pub packed_xz: u8,
    pub y: i16,
    pub block_entity_type: BlockEntityType,
    /// `blockEntity.getUpdateTag(...)`, `None` when empty.
    pub tag: Option<CompoundTag>,
}

impl BlockEntityInfo {
    /// `BlockEntityInfo.create(blockEntity)` from the position, type and update tag.
    pub fn create(x: i32, y: i32, z: i32, block_entity_type: BlockEntityType, update_tag: CompoundTag) -> Self {
        let xz = (x & 15) << 4 | (z & 15);
        BlockEntityInfo {
            packed_xz: xz as u8,
            y: y as i16,
            block_entity_type,
            tag: if update_tag.is_empty() { None } else { Some(update_tag) },
        }
    }

    /// `forEachBlockEntityTag`: absolute position for chunk `(chunk_x, chunk_z)`.
    pub fn position(&self, chunk_x: i32, chunk_z: i32) -> (i32, i32, i32) {
        (16 * chunk_x + ((self.packed_xz as i8 as i32 >> 4) & 15), i32::from(self.y), 16 * chunk_z + (self.packed_xz as i32 & 15))
    }
}

/// `ClientboundLevelChunkPacketData`.
#[derive(Clone, Debug, PartialEq)]
pub struct LevelChunkPacketData {
    /// `EnumMap`: written in ordinal order.
    pub heightmaps: BTreeMap<HeightmapType, Vec<i64>>,
    pub buffer: Vec<u8>,
    pub block_entities: Vec<BlockEntityInfo>,
}

impl LevelChunkPacketData {
    /// `new ClientboundLevelChunkPacketData(levelChunk)`: `heightmaps` are all of the chunk's (only those with
    /// `sendToClient` are kept); `block_entities` in the iteration order of the chunk's block entity map.
    pub fn new<'a>(
        sections: &[LevelChunkSection],
        heightmaps: impl IntoIterator<Item = (HeightmapType, &'a [i64])>,
        block_entities: Vec<BlockEntityInfo>,
    ) -> Result<Self, PaletteError> {
        let heightmaps =
            heightmaps.into_iter().filter(|(ty, _)| ty.send_to_client()).map(|(ty, raw)| (ty, raw.to_vec())).collect();
        let total: usize = sections.iter().map(|s| s.serialized_size()).sum::<Result<usize, _>>()?;
        let mut buffer = Vec::with_capacity(total);
        for section in sections {
            section.write(&mut buffer)?;
        }
        if buffer.len() != total {
            return Err(PaletteError(format!("Didn't fill chunk buffer: expected {total} bytes, got {}", buffer.len())));
        }
        Ok(LevelChunkPacketData { heightmaps, buffer, block_entities })
    }

    /// `STREAM_CODEC.encode`.
    pub fn write(&self, out: &mut Vec<u8>) -> Result<(), PaletteError> {
        write_varint(out, self.heightmaps.len() as u32);
        for (ty, longs) in &self.heightmaps {
            write_varint(out, u32::from(ty.id()));
            write_varint(out, longs.len() as u32);
            for l in longs {
                out.extend_from_slice(&l.to_be_bytes());
            }
        }
        if self.buffer.len() > MAX_CHUNK_BUFFER {
            return Err(PaletteError(format!("ByteArray with size {} is bigger than allowed {MAX_CHUNK_BUFFER}", self.buffer.len())));
        }
        write_varint(out, self.buffer.len() as u32);
        out.extend_from_slice(&self.buffer);
        write_varint(out, self.block_entities.len() as u32);
        for info in &self.block_entities {
            out.push(info.packed_xz);
            out.extend_from_slice(&info.y.to_be_bytes());
            write_varint(out, info.block_entity_type.id());
            match &info.tag {
                // OPTIONAL_COMPOUND_TAG = FriendlyByteBuf.writeNbt(@Nullable): END (0) or the nameless network root
                None => out.push(0),
                Some(tag) => nbt_io::write_any_tag(&Tag::Compound(tag.clone()), out).map_err(|e| PaletteError(e.to_string()))?,
            }
        }
        Ok(())
    }

    /// `STREAM_CODEC.decode`.
    pub fn read(input: &mut &[u8]) -> Result<Self, PaletteError> {
        let count = read_varint(input)?;
        let mut heightmaps = BTreeMap::new();
        for _ in 0..count {
            let ty = HeightmapType::from_id(read_varint(input)? as i32);
            let n = read_varint(input)? as usize;
            if n.checked_mul(8).is_none_or(|b| b > input.len()) {
                return Err(PaletteError("Not enough bytes in buffer".into()));
            }
            let longs = (0..n)
                .map(|_| {
                    let (c, rest) = input.split_first_chunk::<8>().expect("length checked");
                    *input = rest;
                    i64::from_be_bytes(*c)
                })
                .collect();
            heightmaps.insert(ty, longs);
        }
        let len = read_varint(input)? as usize;
        if len > MAX_CHUNK_BUFFER {
            return Err(PaletteError(format!("ByteArray with size {len} is bigger than allowed {MAX_CHUNK_BUFFER}")));
        }
        if len > input.len() {
            return Err(PaletteError("Not enough bytes in buffer".into()));
        }
        let (buffer, rest) = input.split_at(len);
        *input = rest;
        let n = read_varint(input)?;
        let mut block_entities = Vec::new();
        for _ in 0..n {
            let Some((&[packed_xz, y_hi, y_lo], rest)) = input.split_first_chunk::<3>() else {
                return Err(PaletteError("Not enough bytes in buffer".into()));
            };
            *input = rest;
            let id = read_varint(input)?;
            let block_entity_type = BlockEntityType::from_id(id)
                .ok_or_else(|| PaletteError(format!("Received unknown block entity type id {id}")))?;
            let tag = match nbt_io::read_any_tag(input, &mut NbtAccounter::create(MAX_CHUNK_BUFFER as i64))
                .map_err(|e| PaletteError(e.to_string()))?
            {
                Tag::End => None,
                Tag::Compound(c) => Some(c),
                _ => return Err(PaletteError("Not a compound tag".into())),
            };
            block_entities.push(BlockEntityInfo { packed_xz, y: i16::from_be_bytes([y_hi, y_lo]), block_entity_type, tag });
        }
        Ok(LevelChunkPacketData { heightmaps, buffer: buffer.to_vec(), block_entities })
    }
}

/// `java.util.BitSet` as `ByteBufCodecs.BIT_SET` writes it: `FriendlyByteBuf.writeByteArray(bitSet.toByteArray())`
/// (little-endian bytes, trailing zero bytes trimmed).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BitSet(Vec<u64>);

impl BitSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&mut self, index: usize) {
        if self.0.len() <= index / 64 {
            self.0.resize(index / 64 + 1, 0);
        }
        self.0[index / 64] |= 1 << (index % 64);
    }

    pub fn get(&self, index: usize) -> bool {
        self.0.get(index / 64).is_some_and(|w| w >> (index % 64) & 1 != 0)
    }

    /// `toByteArray()`.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes: Vec<u8> = self.0.iter().flat_map(|w| w.to_le_bytes()).collect();
        while bytes.last() == Some(&0) {
            bytes.pop();
        }
        bytes
    }

    /// `BitSet.valueOf(byte[])`.
    pub fn from_bytes(bytes: &[u8]) -> Self {
        BitSet(
            bytes
                .chunks(8)
                .map(|c| {
                    let mut w = [0u8; 8];
                    w[..c.len()].copy_from_slice(c);
                    u64::from_le_bytes(w)
                })
                .collect(),
        )
    }

    fn write(&self, out: &mut Vec<u8>) {
        let bytes = self.to_bytes();
        write_varint(out, bytes.len() as u32);
        out.extend_from_slice(&bytes);
    }

    fn read(input: &mut &[u8]) -> Result<Self, PaletteError> {
        let n = read_varint(input)? as usize;
        if n > input.len() {
            return Err(PaletteError("Not enough bytes in buffer".into()));
        }
        let (bytes, rest) = input.split_at(n);
        *input = rest;
        Ok(Self::from_bytes(bytes))
    }
}

/// `ClientboundLightUpdatePacketData`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LightUpdatePacketData {
    pub sky_y_mask: BitSet,
    pub block_y_mask: BitSet,
    pub empty_sky_y_mask: BitSet,
    pub empty_block_y_mask: BitSet,
    pub sky_updates: Vec<Vec<u8>>,
    pub block_updates: Vec<Vec<u8>>,
}

impl LightUpdatePacketData {
    /// The constructor taking a light engine: one entry per light section (index 0 = lowest light section).
    /// `sky_layers[i]` / `block_layers[i]` is `getLayerListener(layer).getDataLayerData(section)` (None = no data);
    /// the optional filters are the `*ChangedLightSectionFilter` bit sets (`None` = all sections).
    pub fn new(
        sky_layers: &[Option<&DataLayer>],
        block_layers: &[Option<&DataLayer>],
        sky_changed_filter: Option<&BitSet>,
        block_changed_filter: Option<&BitSet>,
    ) -> Self {
        let mut data = LightUpdatePacketData::default();
        for section_index in 0..sky_layers.len().max(block_layers.len()) {
            if sky_changed_filter.is_none_or(|f| f.get(section_index)) {
                prepare_section(sky_layers.get(section_index).copied().flatten(), section_index, &mut data.sky_y_mask, &mut data.empty_sky_y_mask, &mut data.sky_updates);
            }
            if block_changed_filter.is_none_or(|f| f.get(section_index)) {
                prepare_section(block_layers.get(section_index).copied().flatten(), section_index, &mut data.block_y_mask, &mut data.empty_block_y_mask, &mut data.block_updates);
            }
        }
        data
    }

    pub fn write(&self, out: &mut Vec<u8>) {
        self.sky_y_mask.write(out);
        self.block_y_mask.write(out);
        self.empty_sky_y_mask.write(out);
        self.empty_block_y_mask.write(out);
        for list in [&self.sky_updates, &self.block_updates] {
            write_varint(out, list.len() as u32);
            for layer in list {
                write_varint(out, layer.len() as u32);
                out.extend_from_slice(layer);
            }
        }
    }

    pub fn read(input: &mut &[u8]) -> Result<Self, PaletteError> {
        let sky_y_mask = BitSet::read(input)?;
        let block_y_mask = BitSet::read(input)?;
        let empty_sky_y_mask = BitSet::read(input)?;
        let empty_block_y_mask = BitSet::read(input)?;
        let mut lists = [Vec::new(), Vec::new()];
        for list in &mut lists {
            let n = read_varint(input)?;
            for _ in 0..n {
                let len = read_varint(input)? as usize;
                if len > data_layer::SIZE {
                    return Err(PaletteError(format!("ByteArray with size {len} is bigger than allowed {}", data_layer::SIZE)));
                }
                if len > input.len() {
                    return Err(PaletteError("Not enough bytes in buffer".into()));
                }
                let (layer, rest) = input.split_at(len);
                *input = rest;
                list.push(layer.to_vec());
            }
        }
        let [sky_updates, block_updates] = lists;
        Ok(LightUpdatePacketData { sky_y_mask, block_y_mask, empty_sky_y_mask, empty_block_y_mask, sky_updates, block_updates })
    }
}

/// `prepareSectionData`.
fn prepare_section(layer: Option<&DataLayer>, index: usize, mask: &mut BitSet, empty_mask: &mut BitSet, updates: &mut Vec<Vec<u8>>) {
    if let Some(data) = layer {
        if data.is_empty() {
            empty_mask.set(index);
        } else {
            mask.set(index);
            updates.push(data.data().to_vec());
        }
    }
}

/// Body of `ClientboundLevelChunkWithLightPacket`: `x`, `z`, chunk data, light data.
pub fn write_level_chunk_with_light(
    out: &mut Vec<u8>,
    x: i32,
    z: i32,
    chunk: &LevelChunkPacketData,
    light: &LightUpdatePacketData,
) -> Result<(), PaletteError> {
    out.extend_from_slice(&x.to_be_bytes());
    out.extend_from_slice(&z.to_be_bytes());
    chunk.write(out)?;
    light.write(out);
    Ok(())
}
