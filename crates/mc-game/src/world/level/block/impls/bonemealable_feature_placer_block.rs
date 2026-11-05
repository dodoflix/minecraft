//! `net.minecraft.world.level.block.BonemealableFeaturePlacerBlock` (moss block, pale moss block).
use super::prelude::*;
use crate::server::level::ServerLevel;
use crate::world::level::block::bonemeal::{BonemealSource, BonemealType};

pub struct BonemealableFeaturePlacerBlock {
    /// The configured feature key (`CaveFeatures.MOSS_PATCH_BONEMEAL` / `VegetationFeatures.PALE_MOSS_PATCH_BONEMEAL`).
    feature: &'static str,
}

impl BlockBehaviour for BonemealableFeaturePlacerBlock {
    fn is_bonemealable(&self) -> bool {
        true
    }
    fn is_valid_bonemeal_target(&self, level: &dyn LevelReader, pos: BlockPos, _state: BlockState, _source: BonemealSource) -> bool {
        level.get_block_state(pos.above()).is_air()
    }
    fn perform_bonemeal(&self, level: &mut ServerLevel, pos: BlockPos, _state: BlockState, _source: BonemealSource) {
        level.place_feature(self.feature, pos.above());
    }
    fn get_bonemeal_type(&self) -> BonemealType {
        BonemealType::NeighborSpreader
    }
}

register_block_class!("BonemealableFeaturePlacerBlock", |block| Box::new(BonemealableFeaturePlacerBlock {
    feature: if block == Block::PALE_MOSS_BLOCK { "pale_moss_patch_bonemeal" } else { "moss_patch_bonemeal" },
}));
