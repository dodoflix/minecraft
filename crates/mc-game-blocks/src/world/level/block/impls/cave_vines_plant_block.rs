//! `net.minecraft.world.level.block.CaveVinesPlantBlock` (extends `GrowingPlantBodyBlock`).
use super::growing_plant_block::GrowingPlant;
use super::growing_plant_body_block::{GrowingPlantBody, growing_plant_body_behaviour};
use super::prelude::*;
use crate::server::level::ServerLevel;
use crate::world::level::block::bonemeal::BonemealSource;

pub struct CaveVinesPlantBlock;

impl GrowingPlant for CaveVinesPlantBlock {
    fn growth_direction(&self) -> Direction {
        Direction::Down
    }
    fn schedule_fluid_ticks(&self) -> bool {
        false
    }
    fn head_block(&self) -> Block {
        Block::CAVE_VINES
    }
    fn body_block(&self) -> Block {
        Block::CAVE_VINES_PLANT
    }
}
impl GrowingPlantBody for CaveVinesPlantBlock {
    fn update_head_after_converted_from_body(&self, body_state: BlockState, head_state: BlockState) -> BlockState {
        head_state.set(props::BERRIES, body_state.get(props::BERRIES))
    }
}

impl BlockBehaviour for CaveVinesPlantBlock {
    growing_plant_body_behaviour!(custom_bonemeal_and_clone);
    fn get_clone_item_stack(&self, _state: BlockState, _level: &ServerLevel, _pos: BlockPos, _include_data: bool) -> crate::world::item::ItemStack {
        super::cave_vines::glow_berries()
    }
    fn use_without_item(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, player: &mut crate::world::entity::ServerPlayer, _hit: &mc_core::phys::BlockHitResult) -> crate::world::item::InteractionResult {
        super::cave_vines::use_(&*player, state, level, pos)
    }
    fn is_valid_bonemeal_target(&self, _level: &dyn LevelReader, _pos: BlockPos, state: BlockState, _source: BonemealSource) -> bool {
        !state.get(props::BERRIES)
    }
    fn perform_bonemeal(&self, level: &mut ServerLevel, pos: BlockPos, state: BlockState, _source: BonemealSource) {
        level.set_block_flags(pos, state.set(props::BERRIES, true), 2);
    }
}

register_block_class!("CaveVinesPlantBlock", |_| Box::new(CaveVinesPlantBlock));
