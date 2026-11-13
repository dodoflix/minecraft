//! `net.minecraft.world.level.block.JukeboxBlock`.
use crate::world::block_entity::jukebox_block_entity::JukeboxBlockEntity;
use mc_core::phys::BlockHitResult;
use mc_data::GameEvent;
use mc_game_core::register_block_class;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::stats::stats::Stats;
use mc_game_core::world::entity::ServerPlayer;
use mc_game_core::world::item::{InteractionHand, InteractionResult, ItemStack, ItemStackExt};
use mc_game_core::world::level::block::impls::prelude::*;
use mc_protocol::components::{BLOCK_ENTITY_DATA, JUKEBOX_PLAYABLE};

pub struct JukeboxBlock;

impl BlockBehaviour for JukeboxBlock {
    fn set_placed_by(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        _placer: Option<&mut ServerPlayer>,
        stack: &ItemStack,
    ) {
        if let Some(data) = stack.get(&BLOCK_ENTITY_DATA)
            && data.tag.contains("RecordItem")
        {
            level.set_block(pos, state.set(props::HAS_RECORD, true), 2, 512);
        }
    }

    fn use_without_item(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        _player: &mut ServerPlayer,
        _hit: &BlockHitResult,
    ) -> InteractionResult {
        if state.get(props::HAS_RECORD) {
            let popped = level.with_block_entity(pos, |level, be| {
                be.as_any_mut()
                    .downcast_mut::<JukeboxBlockEntity>()
                    .map(|j| j.pop_out_the_item(level))
                    .is_some()
            });
            if popped == Some(true) {
                return InteractionResult::SUCCESS;
            }
        }
        InteractionResult::PASS
    }

    fn use_item_on(
        &self,
        _stack: &ItemStack,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        player: &mut ServerPlayer,
        hand: InteractionHand,
        _hit: &BlockHitResult,
    ) -> InteractionResult {
        if state.get(props::HAS_RECORD) {
            return InteractionResult::TRY_WITH_EMPTY_HAND;
        }
        // `JukeboxPlayable.tryInsertIntoJukebox(level, pos, toInsert, player)`
        let to_insert = player.get_item_in_hand(hand);
        let result = if to_insert.get(&JUKEBOX_PLAYABLE).is_none() {
            InteractionResult::TRY_WITH_EMPTY_HAND
        } else if level.get_block_state(pos).is_block(Block::JUKEBOX)
            && !level.get_block_state(pos).get(props::HAS_RECORD)
        {
            let infinite = player.player.abilities.instabuild;
            let inserted = player
                .player
                .inventory
                .equipment_get_mut(hand.as_equipment_slot())
                .consume_and_return(1, infinite);
            let found = level.with_block_entity(pos, |level, be| {
                be.as_any_mut()
                    .downcast_mut::<JukeboxBlockEntity>()
                    .map(|j| j.set_the_item(level, inserted))
                    .is_some()
            });
            if found == Some(true) {
                level.game_event_from_entity_ref_in_state(
                    &*player,
                    Some(state),
                    GameEvent::BLOCK_CHANGE,
                    mc_core::phys::Vec3::at_center_of(&pos),
                );
            }
            player.award_stat_once(&*level, Stats::PLAY_RECORD);
            InteractionResult::SUCCESS
        } else {
            InteractionResult::TRY_WITH_EMPTY_HAND
        };
        if result.consumes_action() {
            result
        } else {
            InteractionResult::TRY_WITH_EMPTY_HAND
        }
    }

    fn affect_neighbors_after_removal(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        _moved_by_piston: bool,
    ) {
        level.update_neighbour_for_output_signal(pos, state.block());
    }

    fn own_signal(&self, _state: BlockState, level: &dyn BlockGetter, pos: BlockPos) -> i32 {
        if level
            .get_block_entity(pos)
            .and_then(|be| be.as_any().downcast_ref::<JukeboxBlockEntity>())
            .is_some_and(|j| j.is_playing())
        {
            15
        } else {
            0
        }
    }

    fn get_analog_output_signal(
        &self,
        _state: BlockState,
        level: &ServerLevel,
        pos: BlockPos,
        _direction: Direction,
    ) -> i32 {
        level
            .get_block_entity(pos)
            .and_then(|be| be.as_any().downcast_ref::<JukeboxBlockEntity>())
            .map_or(0, |j| j.comparator_output(level))
    }
}

register_block_class!("JukeboxBlock", |_| Box::new(JukeboxBlock));
