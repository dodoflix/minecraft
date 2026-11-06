//! `net.minecraft.advancements.predicates.BlockPredicate`.

use super::matchers::DataComponentMatchers;
use super::state::{NbtPredicate, StatePropertiesPredicate};
use crate::codec::EncodeResult;
use crate::codec::{Decode, DecodeResult, Encode};
use crate::components::common::RegistrySet;
use mc_dfu::{Codec, MapCodec};

/// `BlockPredicate`.
#[derive(Clone, Debug, PartialEq)]
pub struct BlockPredicate {
    pub blocks: Option<RegistrySet<mc_data::Block>>,
    pub properties: Option<StatePropertiesPredicate>,
    pub nbt: Option<NbtPredicate>,
    pub components: DataComponentMatchers,
}

impl BlockPredicate {
    /// `BlockPredicate.MAP_CODEC`.
    pub fn map_codec() -> MapCodec<BlockPredicate> {
        mc_dfu::record!(BlockPredicate {
            blocks: RegistrySet::<mc_data::Block>::codec().optional_field_of("blocks"),
            properties: StatePropertiesPredicate::codec().optional_field_of("state"),
            nbt: NbtPredicate::codec().optional_field_of("nbt"),
            components: DataComponentMatchers::map_codec(),
        })
    }

    /// `BlockPredicate.CODEC`.
    pub fn codec() -> Codec<BlockPredicate> {
        static C: std::sync::LazyLock<Codec<BlockPredicate>> =
            std::sync::LazyLock::new(|| BlockPredicate::map_codec().codec());
        C.clone()
    }

    /// `willMatchBlockEntity()`.
    pub fn will_match_block_entity(&self) -> bool {
        self.nbt.is_some() || !self.components.is_empty()
    }

    /// `requiresNbt()`.
    pub fn requires_nbt(&self) -> bool {
        self.nbt.is_some()
    }
}

/// `BlockPredicate.STREAM_CODEC`.
impl Encode for BlockPredicate {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        match &self.blocks {
            None => false.encode(out)?,
            Some(b) => {
                true.encode(out)?;
                b.encode_stream(out)?;
            }
        }
        self.properties.encode(out)?;
        self.nbt.encode(out)?;
        self.components.encode(out)?;
        Ok(())
    }
}
impl Decode for BlockPredicate {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        let blocks = if bool::decode(input)? {
            Some(RegistrySet::<mc_data::Block>::decode_stream(input)?)
        } else {
            None
        };
        Ok(BlockPredicate {
            blocks,
            properties: Option::<StatePropertiesPredicate>::decode(input)?,
            nbt: Option::<NbtPredicate>::decode(input)?,
            components: DataComponentMatchers::decode(input)?,
        })
    }
}
