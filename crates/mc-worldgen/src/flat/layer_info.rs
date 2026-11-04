//! `FlatLayerInfo`: `height` layers of one block.

use std::fmt;

use mc_data::Block;
use mc_data::block::BlockState;
use serde_json::Value;

use crate::data::LoadError;
use crate::densityfunction::codec::{
    Fields, LoadResult, parse_identifier, parse_int_range,
};
use crate::noise_settings::DIMENSION_Y_SIZE;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FlatLayerInfo {
    height: i32,
    block: Block,
}

impl FlatLayerInfo {
    pub fn new(height: i32, block: Block) -> FlatLayerInfo {
        FlatLayerInfo { height, block }
    }

    /// `FlatLayerInfo.CODEC`: `height` is `Codec.intRange(0, DimensionType.Y_SIZE)`, `block` a registered block id.
    pub fn from_json(value: &Value) -> LoadResult<FlatLayerInfo> {
        let fields = Fields::new(value)?;
        let height = fields.required("height", |v| parse_int_range(v, 0, DIMENSION_Y_SIZE))?;
        let block = fields.required("block", |v| {
            let id = parse_identifier(v)?;
            Block::by_name(&id.to_string()).ok_or_else(|| {
                LoadError::new(format!("Unknown registry key in minecraft:block: {id}"))
            })
        })?;
        Ok(FlatLayerInfo { height, block })
    }

    pub fn get_height(&self) -> i32 {
        self.height
    }

    pub fn get_block(&self) -> Block {
        self.block
    }

    pub fn get_block_state(&self) -> BlockState {
        self.block.default_state()
    }

    pub fn height_limited(&self, max_height: i32) -> FlatLayerInfo {
        if self.height > max_height {
            FlatLayerInfo::new(max_height, self.block)
        } else {
            *self
        }
    }
}

/// `FlatLayerInfo.toString()`.
impl fmt::Display for FlatLayerInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.height != 1 {
            write!(f, "{}*", self.height)?;
        }
        f.write_str(self.block.name())
    }
}
