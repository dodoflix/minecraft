//! `net.minecraft.world.level.block.ChiseledBookShelfBlock`.
use super::selectable_slot_container::get_hit_slot;
use crate::world::block_entity::chiseled_book_shelf_block_entity::{
    ChiseledBookShelfBlockEntity, is_bookshelf_book, slot_occupied_property,
};
use mc_core::phys::BlockHitResult;
use mc_data::{GameEvent, Item, SoundEvent};
use mc_game_core::register_block_class;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::stats::Stat;
use mc_game_core::world::entity::{Entity, ServerPlayer};
use mc_game_core::world::inventory::Prediction;
use mc_game_core::world::item::{
    BlockPlaceContext, InteractionHand, InteractionResult, ItemStack, ItemStackExt,
};
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::world::level::gameevent::GameEventContext;
use mc_protocol::packets::play::level::SoundSource;

pub struct ChiseledBookShelfBlock;

const ROWS: i32 = 2;
const COLUMNS: i32 = 3;

fn add_book(
    level: &mut ServerLevel,
    pos: BlockPos,
    player: &mut ServerPlayer,
    hand: InteractionHand,
    slot: usize,
) {
    let item = player.get_item_in_hand(hand).item();
    player.award_stat_once(&*level, Stat::Used(item));
    let sound = if item == Item::ENCHANTED_BOOK {
        SoundEvent::BLOCK_CHISELED_BOOKSHELF_INSERT_ENCHANTED
    } else {
        SoundEvent::BLOCK_CHISELED_BOOKSHELF_INSERT
    };
    let infinite = player.player.abilities.instabuild;
    let consumed = player
        .player
        .inventory
        .equipment_get_mut(hand.as_equipment_slot())
        .consume_and_return(1, infinite);
    level.with_block_entity(pos, |level, be| {
        if let Some(shelf) = be
            .as_any_mut()
            .downcast_mut::<ChiseledBookShelfBlockEntity>()
        {
            shelf.set_item_with_level(level, slot, consumed);
        }
    });
    level.play_sound(None, pos, sound, SoundSource::Blocks, 1.0, 1.0);
}

fn remove_book(level: &mut ServerLevel, pos: BlockPos, player: &mut ServerPlayer, slot: usize) {
    let mut retrieved = level
        .with_block_entity(pos, |level, be| {
            be.as_any_mut()
                .downcast_mut::<ChiseledBookShelfBlockEntity>()
                .map(|s| s.remove_item_with_level(level, slot))
        })
        .flatten()
        .unwrap_or_default();
    let sound = if retrieved.item() == Item::ENCHANTED_BOOK {
        SoundEvent::BLOCK_CHISELED_BOOKSHELF_PICKUP_ENCHANTED
    } else {
        SoundEvent::BLOCK_CHISELED_BOOKSHELF_PICKUP
    };
    level.play_sound(None, pos, sound, SoundSource::Blocks, 1.0, 1.0);
    let infinite = player.player.abilities.instabuild;
    if !player.player.inventory.add(&mut retrieved, infinite) {
        player.drop(level, retrieved, false, Prediction::ServerOnly);
    }
    level.game_event_at_block(
        GameEvent::BLOCK_CHANGE,
        pos,
        &GameEventContext::of_entity(Some(player.id())),
    );
}

impl BlockBehaviour for ChiseledBookShelfBlock {
    fn get_state_for_placement(
        &self,
        block: Block,
        _level: &mut ServerLevel,
        context: &BlockPlaceContext,
    ) -> Option<BlockState> {
        Some(block.default_state().set(
            props::HORIZONTAL_FACING,
            context.get_horizontal_direction().get_opposite(),
        ))
    }

    fn use_item_on(
        &self,
        stack: &ItemStack,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        player: &mut ServerPlayer,
        hand: InteractionHand,
        hit: &BlockHitResult,
    ) -> InteractionResult {
        if level
            .get_block_entity(pos)
            .is_some_and(|be| be.as_any().is::<ChiseledBookShelfBlockEntity>())
        {
            if stack.is_empty() || !is_bookshelf_book(stack.item()) {
                return InteractionResult::TRY_WITH_EMPTY_HAND;
            }
            match get_hit_slot(hit, state.get(props::HORIZONTAL_FACING), ROWS, COLUMNS) {
                None => InteractionResult::PASS,
                Some(slot) if state.get(slot_occupied_property(slot as usize)) => {
                    InteractionResult::TRY_WITH_EMPTY_HAND
                }
                Some(slot) => {
                    add_book(level, pos, player, hand, slot as usize);
                    InteractionResult::SUCCESS
                }
            }
        } else {
            InteractionResult::PASS
        }
    }

    fn use_without_item(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        player: &mut ServerPlayer,
        hit: &BlockHitResult,
    ) -> InteractionResult {
        if level
            .get_block_entity(pos)
            .is_some_and(|be| be.as_any().is::<ChiseledBookShelfBlockEntity>())
        {
            match get_hit_slot(hit, state.get(props::HORIZONTAL_FACING), ROWS, COLUMNS) {
                None => InteractionResult::PASS,
                Some(slot) if !state.get(slot_occupied_property(slot as usize)) => {
                    InteractionResult::CONSUME
                }
                Some(slot) => {
                    remove_book(level, pos, player, slot as usize);
                    InteractionResult::SUCCESS
                }
            }
        } else {
            InteractionResult::PASS
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

    fn get_analog_output_signal(
        &self,
        _state: BlockState,
        level: &ServerLevel,
        pos: BlockPos,
        _direction: Direction,
    ) -> i32 {
        level
            .get_block_entity(pos)
            .and_then(|be| be.as_any().downcast_ref::<ChiseledBookShelfBlockEntity>())
            .map_or(0, |b| b.get_last_interacted_slot() + 1)
    }

    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::HORIZONTAL_FACING, mirror)
    }
}

register_block_class!("ChiseledBookShelfBlock", |_| Box::new(
    ChiseledBookShelfBlock
));
