//! `net.minecraft.world.level.block.CaveVinesBlock` (extends `GrowingPlantHeadBlock`).
use mc_game_core::register_block_class;
use super::growing_plant_block::GrowingPlant;
use super::growing_plant_head_block::{GrowingPlantHead, growing_plant_head_behaviour};
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::level::block::bonemeal::BonemealSource;
use mc_core::random::RandomSource;

pub struct CaveVinesBlock;

/// `CHANCE_OF_BERRIES_ON_GROWTH`.
const CHANCE_OF_BERRIES_ON_GROWTH: f32 = 0.11;

impl GrowingPlant for CaveVinesBlock {
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
impl GrowingPlantHead for CaveVinesBlock {
    fn grow_per_tick_probability(&self) -> f64 {
        0.1
    }
    fn blocks_to_grow_when_bonemealed(&self, _random: &mut dyn RandomSource) -> i32 {
        1
    }
    fn can_grow_into(&self, state: BlockState) -> bool {
        state.is_air()
    }
    fn update_body_after_converted_from_head(&self, head_state: BlockState, body_state: BlockState) -> BlockState {
        body_state.set(props::BERRIES, head_state.get(props::BERRIES))
    }
    fn get_grow_into_state(&self, grow_from_state: BlockState, random: &mut dyn RandomSource) -> BlockState {
        grow_from_state.cycle(props::AGE_25).set(props::BERRIES, random.next_float() < CHANCE_OF_BERRIES_ON_GROWTH)
    }
}

impl BlockBehaviour for CaveVinesBlock {
    growing_plant_head_behaviour!(custom_bonemeal);
    fn get_clone_item_stack(&self, _state: BlockState, _level: &ServerLevel, _pos: BlockPos, _include_data: bool) -> mc_game_core::world::item::ItemStack {
        mc_game_core::world::level::block::impls::cave_vines::glow_berries()
    }
    fn use_without_item(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, player: &mut mc_game_core::world::entity::ServerPlayer, _hit: &mc_core::phys::BlockHitResult) -> mc_game_core::world::item::InteractionResult {
        mc_game_core::world::level::block::impls::cave_vines::use_(&*player, state, level, pos)
    }
    fn is_valid_bonemeal_target(&self, _level: &dyn LevelReader, _pos: BlockPos, state: BlockState, _source: BonemealSource) -> bool {
        !state.get(props::BERRIES)
    }
    fn perform_bonemeal(&self, level: &mut ServerLevel, pos: BlockPos, state: BlockState, _source: BonemealSource) {
        level.set_block_flags(pos, state.set(props::BERRIES, true), 2);
    }
}

register_block_class!("CaveVinesBlock", |_| Box::new(CaveVinesBlock));
