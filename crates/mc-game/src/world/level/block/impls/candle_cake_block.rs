//! `net.minecraft.world.level.block.CandleCakeBlock` (extends `AbstractCandleBlock`).
use super::abstract_candle_block;
use super::prelude::*;
use crate::server::level::ServerLevel;
use crate::world::entity::ServerPlayer;
use crate::world::item::{InteractionHand, InteractionResult, ItemStack};
use crate::world::level::explosion::Explosion;
use mc_data::Item;

pub struct CandleCakeBlock;

/// `CandleCakeBlock.byCandle(candle)`: `<color>_candle` -> `<color>_candle_cake`.
pub fn by_candle(candle: Block) -> Option<BlockState> {
    Block::by_name(&format!("{}_cake", candle.data().name)).map(Block::default_state)
}

/// `CandleCakeBlock.canLight(state)`.
pub fn can_light(state: BlockState) -> bool {
    state.is_tag("candle_cakes") && state.has_property(props::LIT) && !state.get(props::LIT)
}

impl BlockBehaviour for CandleCakeBlock {
    fn use_item_on(&self, stack: &ItemStack, state: BlockState, level: &mut ServerLevel, pos: BlockPos, player: &mut ServerPlayer, _hand: InteractionHand, hit: &mc_core::phys::BlockHitResult) -> InteractionResult {
        if stack.is(Item::FLINT_AND_STEEL) || stack.is(Item::FIRE_CHARGE) {
            InteractionResult::PASS
        } else if hit.location.y - f64::from(hit.block_pos.y()) > 0.5 && stack.is_empty() && state.get(props::LIT) {
            abstract_candle_block::extinguish(Some(player.id()), state, level, pos);
            InteractionResult::SUCCESS
        } else {
            InteractionResult::TRY_WITH_EMPTY_HAND
        }
    }
    fn use_without_item(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, player: &mut ServerPlayer, _hit: &mc_core::phys::BlockHitResult) -> InteractionResult {
        let eat_result = super::cake_block::eat(level, pos, Block::CAKE.default_state(), player);
        if eat_result.consumes_action() {
            helpers::drop_resources(level, state, pos, None);
        }
        eat_result
    }
    fn get_clone_item_stack(&self, _state: BlockState, _level: &ServerLevel, _pos: BlockPos, _include_data: bool) -> ItemStack {
        ItemStack::new(Item::CAKE)
    }
    fn can_survive(&self, _state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        level.get_block_state(pos.below()).is_solid()
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if direction == Direction::Down && !state.can_survive(&*level, pos) { air() } else { state }
    });
    fn get_analog_output_signal(&self, _state: BlockState, _level: &ServerLevel, _pos: BlockPos, _direction: Direction) -> i32 {
        super::cake_block::FULL_CAKE_SIGNAL
    }
    fn is_pathfindable(&self, _state: BlockState, _ty: mc_data::block::PathComputationType) -> bool {
        false
    }
    fn on_projectile_hit(&self, level: &mut ServerLevel, state: BlockState, hit: &mc_core::phys::BlockHitResult, projectile: &mut dyn crate::world::entity::projectile::Projectile) {
        abstract_candle_block::on_projectile_hit(level, state, hit, &*projectile, !state.get(props::LIT));
    }
    fn on_explosion_hit(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, explosion: &dyn Explosion, on_hit: &mut dyn FnMut(ItemStack, BlockPos)) {
        abstract_candle_block::on_explosion_hit(state, level, pos, explosion, on_hit);
    }
}

register_block_class!("CandleCakeBlock", |_| Box::new(CandleCakeBlock));
