//! `net.minecraft.world.level.block.CakeBlock`.
use super::prelude::*;
use crate::server::level::ServerLevel;
use crate::stats::stat::Stat;
use crate::stats::stats::Stats;
use crate::world::entity::ServerPlayer;
use crate::world::item::{InteractionHand, InteractionResult, ItemStack, ItemStackExt};
use crate::world::level::gameevent::GameEventContext;
use mc_data::{GameEvent, SoundEvent};
use mc_protocol::packets::play::level::SoundSource;

pub struct CakeBlock;

/// `CakeBlock.getOutputSignal(bitesTaken)`.
pub fn get_output_signal(bites_taken: i32) -> i32 {
    (7 - bites_taken) * 2
}

/// `CakeBlock.FULL_CAKE_SIGNAL`.
pub const FULL_CAKE_SIGNAL: i32 = 14;

/// `CakeBlock.eat(level, pos, state, player)`.
pub fn eat(
    level: &mut ServerLevel,
    pos: BlockPos,
    state: BlockState,
    player: &mut ServerPlayer,
) -> InteractionResult {
    if !player.can_eat(false) {
        return InteractionResult::PASS;
    }
    player.award_stat_once(level, Stats::EAT_CAKE_SLICE);
    player.player.food_data.eat(2, 0.1);
    let bites = state.get(props::BITES);
    level.game_event_at_block(
        GameEvent::EAT,
        pos,
        &GameEventContext::of_entity(Some(player.base_id())),
    );
    if bites < 6 {
        level.set_block_and_update(pos, state.set(props::BITES, bites + 1));
    } else {
        level.remove_block(pos, false);
        level.game_event_at_block(
            GameEvent::BLOCK_DESTROY,
            pos,
            &GameEventContext::of_entity(Some(player.base_id())),
        );
    }
    InteractionResult::SUCCESS
}

impl BlockBehaviour for CakeBlock {
    fn use_item_on(
        &self,
        stack: &ItemStack,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        player: &mut ServerPlayer,
        hand: InteractionHand,
        _hit: &mc_core::phys::BlockHitResult,
    ) -> InteractionResult {
        let item = stack.item();
        let is_candle_item =
            mc_data::tags::get::<mc_data::Item>("candles").is_some_and(|tag| tag.contains(item));
        let candle_cake = item
            .block()
            .filter(|block| helpers::is_instance_of(block.default_state(), "CandleBlock"));
        match candle_cake {
            Some(candle) if is_candle_item && state.get(props::BITES) == 0 => {
                let mut used = stack.clone();
                used.consume(1, player.has_infinite_materials());
                player.set_item_in_hand(hand, used);
                level.play_sound(
                    None,
                    pos,
                    SoundEvent::BLOCK_CAKE_ADD_CANDLE,
                    SoundSource::Blocks,
                    1.0,
                    1.0,
                );
                if let Some(candle_cake) = super::candle_cake_block::by_candle(candle) {
                    level.set_block_and_update(pos, candle_cake);
                }
                level.game_event_at_block(
                    GameEvent::BLOCK_CHANGE,
                    pos,
                    &GameEventContext::of_entity(Some(player.base_id())),
                );
                player.award_stat_once(level, Stat::Used(item));
                InteractionResult::SUCCESS
            }
            _ => InteractionResult::TRY_WITH_EMPTY_HAND,
        }
    }
    fn use_without_item(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        player: &mut ServerPlayer,
        _hit: &mc_core::phys::BlockHitResult,
    ) -> InteractionResult {
        eat(level, pos, state, player)
    }
    fn can_survive(&self, _state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        level.get_block_state(pos.below()).is_solid()
    }
    update_shape!(
        |self, state, level, pos, direction, neighbour_pos, neighbour_state| {
            if direction == Direction::Down && !state.can_survive(&*level, pos) {
                air()
            } else {
                state
            }
        }
    );
    fn get_analog_output_signal(
        &self,
        state: BlockState,
        _level: &ServerLevel,
        _pos: BlockPos,
        _direction: Direction,
    ) -> i32 {
        get_output_signal(state.get(props::BITES))
    }
    fn is_pathfindable(
        &self,
        _state: BlockState,
        _ty: mc_data::block::PathComputationType,
    ) -> bool {
        false
    }
}

register_block_class!("CakeBlock", |_| Box::new(CakeBlock));
