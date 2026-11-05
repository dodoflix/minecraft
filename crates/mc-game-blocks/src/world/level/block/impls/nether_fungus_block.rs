//! `net.minecraft.world.level.block.NetherFungusBlock` (extends `VegetationBlock`): bonemeal grows the huge fungus.
use super::prelude::*;
use super::vegetation_block::{MayPlaceOn, vegetation_behaviour_methods};
use crate::server::level::ServerLevel;
use crate::world::level::block::bonemeal::BonemealSource;

pub struct NetherFungusBlock {
    /// `feature` (`worldgen/feature` id).
    feature: &'static str,
    /// `requiredBlock`.
    required_block: Block,
    /// `supportBlocks` (block tag).
    support_blocks: &'static str,
}

impl MayPlaceOn for NetherFungusBlock {
    fn may_place_on(&self, state: BlockState, _level: &dyn BlockGetter, _pos: BlockPos) -> bool {
        state.is_tag(self.support_blocks)
    }
}

impl BlockBehaviour for NetherFungusBlock {
    vegetation_behaviour_methods!();
    fn is_bonemealable(&self) -> bool {
        true
    }
    fn is_valid_bonemeal_target(&self, level: &dyn LevelReader, pos: BlockPos, _state: BlockState, _source: BonemealSource) -> bool {
        level.get_block_state(pos.below()).is_block(self.required_block) && !level.is_outside_build_height(pos.above().y())
    }
    fn is_bonemeal_success(&self, level: &mut ServerLevel, _pos: BlockPos, _state: BlockState, _source: BonemealSource) -> bool {
        level.random().next_float() < 0.4
    }
    fn perform_bonemeal(&self, level: &mut ServerLevel, pos: BlockPos, _state: BlockState, _source: BonemealSource) {
        if let Some(feature) = level.lookup_feature(self.feature) {
            level.place_feature_value(&*feature, pos);
        }
    }
}

register_block_class!("NetherFungusBlock", |block| Box::new(match block.data().name {
    "minecraft:warped_fungus" => NetherFungusBlock { feature: "warped_fungus_planted", required_block: Block::WARPED_NYLIUM, support_blocks: "supports_warped_fungus" },
    "minecraft:crimson_fungus" => NetherFungusBlock { feature: "crimson_fungus_planted", required_block: Block::CRIMSON_NYLIUM, support_blocks: "supports_crimson_fungus" },
    name => panic!("unexpected NetherFungusBlock {name}"),
}));
