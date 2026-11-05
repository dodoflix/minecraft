//! `net.minecraft.world.level.block.LecternBlock`: placing/taking the book, page-turn redstone pulse, comparator signal,
//! the lectern menu. The block entity is `block_entity::lectern_block_entity`.
use mc_game_core::register_block_class;
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::server::level::ServerLevel;
use crate::world::block_entity::lectern_block_entity::LecternBlockEntity;
use mc_game_core::world::entity::item::ItemEntity;
use mc_game_core::world::entity::{Entity as _, EntityId, ServerPlayer};
use mc_game_core::world::inventory::{AbstractContainerMenu, LecternMenu, MenuProvider, SimpleMenuProvider};
use mc_game_core::world::item::{BlockPlaceContext, InteractionHand, InteractionResult, ItemStack, ItemStackExt};
use mc_game_core::world::level::gameevent::GameEventContext;
use mc_core::ident::Identifier;
use mc_data::GameEvent;
use mc_protocol::components::BLOCK_ENTITY_DATA;
use mc_protocol::packets::play::level::SoundSource;
use mc_text::Component;

pub struct LecternBlock;

const PAGE_CHANGE_IMPULSE_TICKS: i32 = 2;

/// `itemStack.is(ItemTags.LECTERN_BOOKS)`.
fn is_lectern_book(level: &ServerLevel, stack: &ItemStack) -> bool {
    let Some(tag) = Identifier::try_parse("minecraft:lectern_books") else { return false };
    level.registries.registry("minecraft:item").is_some_and(|r| r.is_in_tag(stack.item().id(), &tag))
}

/// `LecternBlock.tryPlaceBook(sourceEntity, level, pos, state, item)`: `held` is the item in `hand` of `player`.
fn try_place_book(player: &mut ServerPlayer, hand: InteractionHand, level: &mut ServerLevel, pos: BlockPos, state: BlockState, held: &ItemStack) -> bool {
    if state.get(props::HAS_BOOK) {
        return false;
    }
    place_book(player, hand, level, pos, state, held.clone());
    true
}

/// `LecternBlock.placeBook`.
fn place_book(player: &mut ServerPlayer, hand: InteractionHand, level: &mut ServerLevel, pos: BlockPos, state: BlockState, mut held: ItemStack) {
    let infinite = player.has_infinite_materials();
    let book = held.consume_and_return(1, infinite);
    player.set_item_in_hand(hand, held);
    let placed = level.with_block_entity(pos, |level, be| match be.as_any_mut().downcast_mut::<LecternBlockEntity>() {
        Some(lectern) => {
            let mut book = book;
            lectern.resolve_book(level, Some(&*player), &mut book);
            lectern.set_book(level, book);
            true
        }
        None => false,
    });
    if placed == Some(true) {
        reset_book_state(Some(player.id()), level, pos, state, true);
        level.play_sound(None, pos, mc_data::SoundEvent::ITEM_BOOK_PUT, SoundSource::Blocks, 1.0, 1.0);
    }
}

/// `LecternBlock.resetBookState(sourceEntity, level, pos, state, hasBook)`.
pub fn reset_book_state(source: Option<EntityId>, level: &mut ServerLevel, pos: BlockPos, state: BlockState, has_book: bool) {
    let new_state = state.set(props::POWERED, false).set(props::HAS_BOOK, has_book);
    level.set_block_and_update(pos, new_state);
    level.game_event_at_block(GameEvent::BLOCK_CHANGE, pos, &GameEventContext::of(source, Some(new_state)));
    update_below(level, pos, state);
}

/// `LecternBlock.signalPageChange(level, pos, state)`.
pub fn signal_page_change(level: &mut ServerLevel, pos: BlockPos, state: BlockState) {
    change_powered(level, pos, state, true);
    level.schedule_tick_block(pos, state.block(), PAGE_CHANGE_IMPULSE_TICKS);
    level.level_event(None, 1043, pos, 0);
}

fn change_powered(level: &mut ServerLevel, pos: BlockPos, state: BlockState, powered: bool) {
    level.set_block_and_update(pos, state.set(props::POWERED, powered));
    update_below(level, pos, state);
}

fn update_below(level: &mut ServerLevel, pos: BlockPos, state: BlockState) {
    let orientation = level.initial_orientation(Some(state.get(props::HORIZONTAL_FACING).get_opposite()), Some(Direction::Up));
    level.update_neighbors_at_with_orientation(pos.below(), state.block(), orientation);
}

/// `LecternBlockEntity.preRemoveSideEffects`: the book pops out in front of the lectern.
pub fn drop_book(level: &mut ServerLevel, pos: BlockPos, state: BlockState, book: &ItemStack) {
    if !state.get(props::HAS_BOOK) {
        return;
    }
    let direction = state.get(props::HORIZONTAL_FACING);
    let xo = 0.25_f32 * direction.get_step_x() as f32;
    let zo = 0.25_f32 * direction.get_step_z() as f32;
    let mut entity = ItemEntity::with_stack(level, f64::from(pos.x()) + 0.5 + f64::from(xo), f64::from(pos.y()) + 1.0, f64::from(pos.z()) + 0.5 + f64::from(zo), book.copy());
    entity.set_default_pick_up_delay();
    level.add_fresh_entity(Box::new(entity));
}

/// `player.openMenu(lecternBlockEntity)`: `LecternBlockEntity.createMenu` + `getDisplayName`.
pub fn lectern_menu_provider(pos: BlockPos) -> Box<dyn MenuProvider> {
    Box::new(SimpleMenuProvider::new(Component::translatable("container.lectern", vec![]), move |id, player| Some(Box::new(LecternMenu::new(id, player, pos)) as Box<dyn AbstractContainerMenu>)))
}

impl BlockBehaviour for LecternBlock {
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::HORIZONTAL_FACING, mirror)
    }

    fn get_state_for_placement(&self, block: Block, _level: &mut ServerLevel, context: &BlockPlaceContext) -> Option<BlockState> {
        let mut has_book = false;
        let stack = context.use_on.get_item_in_hand();
        if context.use_on.player.is_some_and(|p| p.can_use_game_master_blocks) && stack.get(&BLOCK_ENTITY_DATA).is_some_and(|data| data.tag.contains("Book")) {
            has_book = true;
        }
        Some(block.default_state().set(props::HORIZONTAL_FACING, context.get_horizontal_direction().get_opposite()).set(props::HAS_BOOK, has_book))
    }

    fn tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        change_powered(level, pos, state, false);
    }

    fn affect_neighbors_after_removal(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, _moved_by_piston: bool) {
        if state.get(props::POWERED) {
            update_below(level, pos, state);
        }
    }

    fn own_signal(&self, state: BlockState, _level: &dyn BlockGetter, _pos: BlockPos) -> i32 {
        if state.get(props::POWERED) { 15 } else { 0 }
    }

    fn get_direct_signal(&self, state: BlockState, _level: &dyn BlockGetter, _pos: BlockPos, direction: Direction) -> i32 {
        if direction == Direction::Up && state.get(props::POWERED) { 15 } else { 0 }
    }

    fn get_analog_output_signal(&self, state: BlockState, level: &ServerLevel, pos: BlockPos, _direction: Direction) -> i32 {
        if !state.get(props::HAS_BOOK) {
            return 0;
        }
        level.get_block_entity(pos).and_then(|be| be.as_any().downcast_ref::<LecternBlockEntity>()).map_or(0, |l| l.get_redstone_signal())
    }

    fn use_item_on(&self, stack: &ItemStack, state: BlockState, level: &mut ServerLevel, pos: BlockPos, player: &mut ServerPlayer, hand: InteractionHand, _hit: &mc_core::phys::BlockHitResult) -> InteractionResult {
        if state.get(props::HAS_BOOK) {
            InteractionResult::TRY_WITH_EMPTY_HAND
        } else if is_lectern_book(level, stack) {
            if try_place_book(player, hand, level, pos, state, stack) { InteractionResult::SUCCESS } else { InteractionResult::PASS }
        } else if stack.is_empty() && hand == InteractionHand::MainHand {
            InteractionResult::PASS
        } else {
            InteractionResult::TRY_WITH_EMPTY_HAND
        }
    }

    fn use_without_item(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, player: &mut ServerPlayer, _hit: &mc_core::phys::BlockHitResult) -> InteractionResult {
        if state.get(props::HAS_BOOK) {
            if level.get_block_entity(pos).is_some_and(|be| be.as_any().is::<LecternBlockEntity>()) {
                let provider = lectern_menu_provider(pos);
                player.open_menu(level, provider.as_ref());
                player.award_stat_once(level, mc_game_core::stats::stats::Stats::INTERACT_WITH_LECTERN);
            }
            InteractionResult::SUCCESS
        } else {
            InteractionResult::CONSUME
        }
    }

    fn get_menu_provider(&self, state: BlockState, level: &ServerLevel, pos: BlockPos) -> Option<Box<dyn MenuProvider>> {
        if !state.get(props::HAS_BOOK) {
            return None;
        }
        level.get_block_entity(pos).and_then(|be| be.menu_provider())
    }

    fn is_pathfindable(&self, _state: BlockState, _ty: mc_data::block::PathComputationType) -> bool {
        false
    }
}

register_block_class!("LecternBlock", |_| Box::new(LecternBlock));
