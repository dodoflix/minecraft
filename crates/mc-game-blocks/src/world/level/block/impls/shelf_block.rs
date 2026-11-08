//! `net.minecraft.world.level.block.ShelfBlock` (`SelectableSlotContainer` 1x3, `SideChainPartBlock`, waterloggable): the
//! stored items live in `block_entity::shelf_block_entity`.
use super::selectable_slot_container::get_hit_slot;
use super::side_chain_part::SideChainPartBlock;
use crate::world::block_entity::shelf_block_entity::ShelfBlockEntity;
use mc_core::phys::BlockHitResult;
use mc_data::enums::SideChainPart;
use mc_data::fluid::Fluid;
use mc_data::{GameEvent, SoundEvent};
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::container::Container;
use mc_game_core::world::entity::ServerPlayer;
use mc_game_core::world::item::{BlockPlaceContext, InteractionHand, InteractionResult, ItemStack};
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::world::level::gameevent::GameEventContext;
use mc_game_core::world::level::redstone::Orientation;
use mc_game_core::world::level::signal_getter::has_neighbor_signal;
use mc_game_core::world::level::{BlockGetter, LevelAccessor};
use mc_game_core::{register_block_class, update_shape};
use mc_protocol::components::USE_EFFECTS;
use mc_protocol::packets::play::level::SoundSource;

pub struct ShelfBlock;

const ROWS: i32 = 1;
const COLUMNS: i32 = 3;

fn play_sound(level: &mut ServerLevel, pos: BlockPos, sound: SoundEvent) {
    level.play_sound(None, pos, sound, SoundSource::Blocks, 1.0, 1.0);
}

/// `swapSingleItem(itemStack, player, shelf, hitSlot, inventory)`; `item_stack` is the held stack (it moves into the shelf).
fn swap_single_item(
    level: &mut ServerLevel,
    pos: BlockPos,
    player: &mut ServerPlayer,
    item_stack: ItemStack,
    hit_slot: usize,
) -> bool {
    let infinite = player.has_infinite_materials();
    let inventory = &mut player.player.inventory;
    level
        .with_block_entity(pos, |_, be| {
            let shelf = be.as_any_mut().downcast_mut::<ShelfBlockEntity>()?;
            let removed = shelf.swap_item_no_update(hit_slot, item_stack);
            // `itemStack.copy()` is taken after the shelf limited the (shared) stack size
            let new_inventory_item = if infinite && removed.is_empty() {
                shelf.get_item(hit_slot).clone()
            } else {
                removed.clone()
            };
            let event = if new_inventory_item
                .get(&USE_EFFECTS)
                .is_some_and(|u| !u.interact_vibrations)
            {
                None
            } else {
                Some(GameEvent::ITEM_INTERACT_FINISH)
            };
            let selected = inventory.get_selected_slot() as usize;
            inventory.set_item(selected, new_inventory_item);
            inventory.set_changed();
            shelf.set_changed_with_event(event);
            Some(!removed.is_empty())
        })
        .flatten()
        .unwrap_or(false)
}

impl ShelfBlock {
    /// `swapHotbar(level, pos, inventory)`.
    fn swap_hotbar(
        &self,
        level: &mut ServerLevel,
        pos: BlockPos,
        player: &mut ServerPlayer,
    ) -> bool {
        let connected = self.get_all_blocks_connected_to(&*level, pos);
        if connected.is_empty() {
            return false;
        }
        let mut any_swapped = false;
        let chain_len = connected.len() as i32;
        for (index, part_pos) in connected.iter().enumerate() {
            let inventory = &mut player.player.inventory;
            let swapped = level.with_block_entity(*part_pos, |_, be| {
                let shelf = be.as_any_mut().downcast_mut::<ShelfBlockEntity>()?;
                let size = shelf.get_container_size() as i32;
                let mut swapped = false;
                for slot in 0..size {
                    let inventory_slot = 9 - (chain_len - index as i32) * size + slot;
                    if inventory_slot >= 0
                        && inventory_slot as usize <= inventory.get_container_size()
                    {
                        let inventory_slot = inventory_slot as usize;
                        let placed = inventory.remove_item_no_update(inventory_slot);
                        let placed_empty = placed.is_empty();
                        let removed_shelf_item = shelf.swap_item_no_update(slot as usize, placed);
                        if !placed_empty || !removed_shelf_item.is_empty() {
                            inventory.set_item(inventory_slot, removed_shelf_item);
                            swapped = true;
                        }
                    }
                }
                inventory.set_changed();
                shelf.set_changed_with_event(Some(GameEvent::ENTITY_INTERACT));
                Some(swapped)
            });
            any_swapped |= swapped.flatten().unwrap_or(false);
        }
        any_swapped
    }
}

impl SideChainPartBlock for ShelfBlock {
    fn get_side_chain_part(&self, state: BlockState) -> SideChainPart {
        state.get(props::SIDE_CHAIN_PART)
    }
    fn set_side_chain_part(&self, state: BlockState, new_part: SideChainPart) -> BlockState {
        state.set(props::SIDE_CHAIN_PART, new_part)
    }
    fn get_facing(&self, state: BlockState) -> Direction {
        state.get(props::HORIZONTAL_FACING)
    }
    fn is_connectable(&self, state: BlockState) -> bool {
        state.is_tag("wooden_shelves")
            && state.has_property(props::POWERED)
            && state.get(props::POWERED)
    }
    fn get_max_chain_length(&self) -> i32 {
        3
    }
}

impl BlockBehaviour for ShelfBlock {
    mc_game_core::simple_waterlogged!();

    fn is_pathfindable(&self, state: BlockState, ty: mc_data::block::PathComputationType) -> bool {
        ty == mc_data::block::PathComputationType::Water && state.get_fluid_state().is_tag("water")
    }

    fn affect_neighbors_after_removal(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        _moved_by_piston: bool,
    ) {
        // `Containers.updateNeighboursAfterDestroy`
        level.update_neighbour_for_output_signal(pos, state.block());
        self.update_neighbors_after_powering_down(level, pos, state);
    }

    fn neighbor_changed(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        _block: Block,
        _orientation: Option<Orientation>,
        _moved_by_piston: bool,
    ) {
        let signal = has_neighbor_signal(&*level, pos);
        if state.get(props::POWERED) != signal {
            let mut new_state = state.set(props::POWERED, signal);
            if !signal {
                new_state = new_state.set(props::SIDE_CHAIN_PART, SideChainPart::Unconnected);
            }
            level.set_block_and_update(pos, new_state);
            play_sound(
                level,
                pos,
                if signal {
                    SoundEvent::BLOCK_SHELF_ACTIVATE
                } else {
                    SoundEvent::BLOCK_SHELF_DEACTIVATE
                },
            );
            level.game_event_at_block(
                if signal {
                    GameEvent::BLOCK_ACTIVATE
                } else {
                    GameEvent::BLOCK_DEACTIVATE
                },
                pos,
                &GameEventContext::of_state(Some(new_state)),
            );
        }
    }

    fn get_state_for_placement(
        &self,
        block: Block,
        level: &mut ServerLevel,
        context: &BlockPlaceContext,
    ) -> Option<BlockState> {
        let pos = context.get_clicked_pos();
        Some(
            block
                .default_state()
                .set(
                    props::HORIZONTAL_FACING,
                    context.get_horizontal_direction().get_opposite(),
                )
                .set(props::POWERED, has_neighbor_signal(&*level, pos))
                .set(
                    props::WATERLOGGED,
                    level.get_fluid_state(pos).get_type() == Fluid::WATER,
                ),
        )
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
        if !level
            .get_block_entity(pos)
            .is_some_and(|be| be.as_any().is::<ShelfBlockEntity>())
            || hand == InteractionHand::OffHand
        {
            return InteractionResult::PASS;
        }
        let Some(hit_slot) = get_hit_slot(hit, state.get(props::HORIZONTAL_FACING), ROWS, COLUMNS)
        else {
            return InteractionResult::PASS;
        };
        if !state.get(props::POWERED) {
            let item_removed =
                swap_single_item(level, pos, player, stack.clone(), hit_slot as usize);
            if item_removed {
                play_sound(
                    level,
                    pos,
                    if stack.is_empty() {
                        SoundEvent::BLOCK_SHELF_TAKE_ITEM
                    } else {
                        SoundEvent::BLOCK_SHELF_SINGLE_SWAP
                    },
                );
            } else {
                if stack.is_empty() {
                    return InteractionResult::PASS;
                }
                play_sound(level, pos, SoundEvent::BLOCK_SHELF_PLACE_ITEM);
            }
            InteractionResult::SUCCESS_SERVER.held_item_transformed_to(stack.clone())
        } else {
            let previous_item = player.player.inventory.get_selected_item().clone();
            if !self.swap_hotbar(level, pos, player) {
                return InteractionResult::CONSUME;
            }
            play_sound(level, pos, SoundEvent::BLOCK_SHELF_MULTI_SWAP);
            let selected = player.player.inventory.get_selected_item().clone();
            if previous_item == selected {
                InteractionResult::SUCCESS_SERVER
            } else {
                InteractionResult::SUCCESS_SERVER.held_item_transformed_to(selected)
            }
        }
    }

    fn on_place(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        old_state: BlockState,
        _moved_by_piston: bool,
    ) {
        if state.get(props::POWERED) {
            self.update_self_and_neighbors_on_powering_up(level, pos, state, old_state);
        } else {
            self.update_neighbors_after_powering_down(level, pos, state);
        }
    }

    update_shape!(
        |self, state, level, pos, direction, neighbour_pos, neighbour_state| {
            if state.get(props::WATERLOGGED) {
                helpers::schedule_water_tick(level, pos);
            }
            state
        }
    );

    fn get_analog_output_signal(
        &self,
        state: BlockState,
        level: &ServerLevel,
        pos: BlockPos,
        direction: Direction,
    ) -> i32 {
        if direction != state.get(props::HORIZONTAL_FACING).get_opposite() {
            return 0;
        }
        level
            .get_block_entity(pos)
            .and_then(|be| be.as_any().downcast_ref::<ShelfBlockEntity>())
            .map_or(0, |shelf| {
                (0..3).fold(0, |bits, slot| {
                    bits | (i32::from(!shelf.get_item(slot).is_empty()) << slot)
                })
            })
    }

    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::HORIZONTAL_FACING, mirror)
    }
}

register_block_class!("ShelfBlock", |_| Box::new(ShelfBlock));
