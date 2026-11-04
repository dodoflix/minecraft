//! `net.minecraft.world.level.levelgen.BelowZeroRetrogen`: persisted retrogen state of chunks upgraded to the
//! -64 world height (target status + the `BitSet` of columns whose bedrock was missing). The `ProtoChunk` /
//! biome-resolver helpers of the vanilla class need the chunk model and live with it.

use mc_nbt::{CompoundTag, Tag};

use crate::blending_data::collection_numbers;
use crate::status::ChunkStatus;

/// `BelowZeroRetrogen.UPGRADE_HEIGHT_ACCESSOR`: `getMinY()`.
pub const UPGRADE_MIN_Y: i32 = -64;
/// `BelowZeroRetrogen.UPGRADE_HEIGHT_ACCESSOR`: `getHeight()`.
pub const UPGRADE_HEIGHT: i32 = 64;
/// `BelowZeroRetrogen.RETAINED_RETROGEN_BIOMES` (biome resource keys).
pub const RETAINED_RETROGEN_BIOMES: [&str; 3] = ["minecraft:lush_caves", "minecraft:dripstone_caves", "minecraft:deep_dark"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BelowZeroRetrogen {
    target_status: ChunkStatus,
    /// `BitSet.toLongArray()` words (no trailing zero words); empty = `BitSet.isEmpty()`.
    missing_bedrock: Vec<u64>,
}

impl BelowZeroRetrogen {
    /// `BelowZeroRetrogen.CODEC.parse(NbtOps, tag).result()`: `None` if `tag` is not a compound, `target_status`
    /// is missing / not a string / not a registered status / `empty`. A `missing_bedrock` that is not a numeric
    /// list or array is ignored (`lenientOptionalFieldOf`); numeric elements are narrowed like `Number.longValue`.
    pub fn parse(tag: &Tag) -> Option<BelowZeroRetrogen> {
        let c = tag.as_compound()?;
        let target_status = ChunkStatus::decode(c.get("target_status")?).filter(|&s| s != ChunkStatus::Empty)?;
        let mut missing_bedrock: Vec<u64> = c
            .get("missing_bedrock")
            .and_then(collection_numbers)
            .map(|numbers| numbers.into_iter().map(|n| n.boxed_long_value() as u64).collect())
            .unwrap_or_default();
        while missing_bedrock.last() == Some(&0) {
            missing_bedrock.pop();
        }
        Some(BelowZeroRetrogen { target_status, missing_bedrock })
    }

    /// `BelowZeroRetrogen.CODEC.encodeStart(NbtOps, this)`: `target_status` string, plus `missing_bedrock` long array when any bit is set.
    pub fn to_tag(&self) -> Tag {
        let mut tag = CompoundTag::new();
        tag.put_string("target_status", self.target_status.name());
        if !self.missing_bedrock.is_empty() {
            tag.put_long_array("missing_bedrock", self.missing_bedrock.iter().map(|&w| w as i64).collect());
        }
        Tag::Compound(tag)
    }

    pub fn target_status(&self) -> ChunkStatus {
        self.target_status
    }

    pub fn has_bedrock_holes(&self) -> bool {
        !self.missing_bedrock.is_empty()
    }

    /// `hasBedrockHole(x, z)`: bit `(z & 15) * 16 + (x & 15)`.
    pub fn has_bedrock_hole(&self, x: i32, z: i32) -> bool {
        let index = ((z & 15) * 16 + (x & 15)) as usize;
        self.missing_bedrock.get(index / 64).is_some_and(|w| w >> (index % 64) & 1 != 0)
    }
}
