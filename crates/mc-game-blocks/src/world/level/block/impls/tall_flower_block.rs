//! `net.minecraft.world.level.block.TallFlowerBlock` (extends `DoublePlantBlock`; sunflower, lilac, rose bush, peony):
//! bonemeal pops the flower as an item.
use super::double_plant_block::double_plant_behaviour_methods;
use super::prelude::*;
use super::vegetation_block::MayPlaceOn;
use crate::server::level::ServerLevel;
use crate::world::item::ItemStack;
use crate::world::level::block::bonemeal::BonemealSource;

pub struct TallFlowerBlock;
impl MayPlaceOn for TallFlowerBlock {}

impl BlockBehaviour for TallFlowerBlock {
    double_plant_behaviour_methods!();
    fn is_bonemealable(&self) -> bool {
        true
    }
    fn is_valid_bonemeal_target(&self, _level: &dyn LevelReader, _pos: BlockPos, _state: BlockState, _source: BonemealSource) -> bool {
        true
    }
    fn perform_bonemeal(&self, level: &mut ServerLevel, pos: BlockPos, state: BlockState, _source: BonemealSource) {
        if let Some(item) = state.block().item() {
            helpers::pop_resource(level, pos, ItemStack::new(item));
        }
    }
}

register_block_class!("TallFlowerBlock", |_| Box::new(TallFlowerBlock));
