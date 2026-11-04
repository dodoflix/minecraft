//! `net.minecraft.world.level.levelgen.blending.BlendingData`: the persisted shape only (`Packed` + `pack`/`unpack`);
//! the runtime density/biome sampling of `Blender` is not ported here.

use mc_nbt::{CompoundTag, ListTag, Number, Tag};

/// `BlendingData.NO_VALUE`.
pub const NO_VALUE: f32 = f32::MAX;
/// `BlendingData.CELL_COLUMN_COUNT` (`CELL_COLUMN_INSIDE_COUNT (7) + CELL_COLUMN_OUTSIDE_COUNT (9)`).
pub const CELL_COLUMN_COUNT: usize = 16;

/// Elements of a `CollectionTag` (list or typed array) when every element is numeric: what
/// `NbtOps.getStream` + `getNumberValue` accept (`DynamicOps.getLongStream`, `ListCodec` of a numeric codec).
pub(crate) fn collection_numbers(tag: &Tag) -> Option<Vec<Number>> {
    match tag {
        Tag::List(list) => list.iter().map(Tag::as_number).collect(),
        Tag::ByteArray(v) => Some(v.iter().map(|&x| Number::Byte(x)).collect()),
        Tag::IntArray(v) => Some(v.iter().map(|&x| Number::Int(x)).collect()),
        Tag::LongArray(v) => Some(v.iter().map(|&x| Number::Long(x)).collect()),
        _ => None,
    }
}

/// `BlendingData.Packed`.
#[derive(Clone, Debug, PartialEq)]
pub struct BlendingDataPacked {
    pub min_section: i32,
    pub max_section: i32,
    pub heights: Option<Vec<f32>>,
}

impl BlendingDataPacked {
    /// `BlendingData.Packed.validateArraySize`.
    fn validate(&self) -> Result<(), String> {
        match &self.heights {
            Some(h) if h.len() != CELL_COLUMN_COUNT => Err(format!("heights has to be of length {CELL_COLUMN_COUNT}")),
            _ => Ok(()),
        }
    }

    /// `Packed.CODEC.parse(NbtOps, tag).result()`: `None` when the codec errors (not a compound, `min_section`/
    /// `max_section` missing or not numeric, `heights` present, decodable but not exactly 16 long).
    /// A `heights` value that is not a list of numbers is ignored (`lenientOptionalFieldOf`).
    pub fn parse(tag: &Tag) -> Option<BlendingDataPacked> {
        let c = tag.as_compound()?;
        let packed = BlendingDataPacked {
            min_section: c.get("min_section")?.as_number()?.boxed_int_value(),
            max_section: c.get("max_section")?.as_number()?.boxed_int_value(),
            heights: c
                .get("heights")
                .and_then(collection_numbers)
                .map(|numbers| numbers.into_iter().map(Number::float_value).collect()),
        };
        packed.validate().ok()?;
        Some(packed)
    }

    /// `Packed.CODEC.encodeStart(NbtOps, packed)`; `Err` when `heights` has the wrong length (the codec's `validate`).
    pub fn to_tag(&self) -> Result<Tag, String> {
        self.validate()?;
        let mut tag = CompoundTag::new();
        tag.put_int("min_section", self.min_section);
        tag.put_int("max_section", self.max_section);
        if let Some(heights) = &self.heights {
            let mut list = ListTag::with_capacity(heights.len());
            for &h in heights {
                list.push(Tag::Float(h));
            }
            tag.put("heights", Tag::List(list));
        }
        Ok(Tag::Compound(tag))
    }
}

/// The persisted state of `BlendingData`: the old-generation area (`LevelHeightAccessor.create(minY, height)`)
/// and the 16 column heights (`NO_VALUE` = not yet known). Densities/biomes are derived at runtime.
#[derive(Clone, Debug, PartialEq)]
pub struct BlendingData {
    area_min_y: i32,
    area_height: i32,
    pub has_calculated_data: bool,
    pub heights: [f32; CELL_COLUMN_COUNT],
}

impl BlendingData {
    /// `BlendingData.unpack(packed)`.
    pub fn unpack(packed: Option<&BlendingDataPacked>) -> Option<BlendingData> {
        let packed = packed?;
        let min_y = packed.min_section.wrapping_shl(4);
        let mut heights = [NO_VALUE; CELL_COLUMN_COUNT];
        if let Some(h) = &packed.heights {
            // Packed values always passed `validate`; a hand-built one with the wrong length keeps NO_VALUE for missing cells.
            let n = h.len().min(CELL_COLUMN_COUNT);
            heights[..n].copy_from_slice(&h[..n]);
        }
        Some(BlendingData {
            area_min_y: min_y,
            area_height: packed.max_section.wrapping_shl(4).wrapping_sub(min_y),
            has_calculated_data: false,
            heights,
        })
    }

    /// `pack()`.
    pub fn pack(&self) -> BlendingDataPacked {
        let has_height = self.heights.iter().any(|&h| h != NO_VALUE);
        let max_y = self.area_min_y.wrapping_add(self.area_height).wrapping_sub(1);
        BlendingDataPacked {
            min_section: self.area_min_y >> 4,
            max_section: (max_y >> 4).wrapping_add(1),
            heights: has_height.then(|| self.heights.to_vec()),
        }
    }

    /// `getAreaWithOldGeneration().getMinY()`.
    pub fn min_y(&self) -> i32 {
        self.area_min_y
    }

    /// `getAreaWithOldGeneration().getHeight()`.
    pub fn height(&self) -> i32 {
        self.area_height
    }
}
