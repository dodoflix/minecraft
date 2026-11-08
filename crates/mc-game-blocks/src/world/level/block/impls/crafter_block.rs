//! `net.minecraft.world.level.block.CrafterBlock`: redstone-triggered 3x3 crafting that dispenses the result into the
//! container in front (or drops it). `orientation` rotates / mirrors like the jigsaw block.
//!
//! Not implemented: `useWithoutItem` opens the menu through the block entity's `menu_provider`, which stays `None` until
//! the crafter menu exists.
use crate::world::block_entity::crafter_block_entity::CrafterBlockEntity;
use crate::world::block_entity::hopper_block_entity::{add_item, get_container_at, with_container};
use mc_core::direction::FrontAndTop;
use mc_core::phys::{AABB, BlockHitResult, Vec3};
use mc_game_core::advancements::triggers::CriteriaTriggers;
use mc_game_core::register_block_class;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::block_entity::{changed_log, deferred};
use mc_game_core::world::container::Container;
use mc_game_core::world::dispenser::spawn_item;
use mc_game_core::world::entity::ServerPlayer;
use mc_game_core::world::item::crafting::{CraftingInput, RecipeCache, RecipeHolder};
use mc_game_core::world::item::{BlockPlaceContext, InteractionResult, ItemStack, behaviour};
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::world::level::redstone::Orientation;
use mc_game_core::world::level::signal_getter::has_neighbor_signal;
use std::cell::RefCell;

pub struct CrafterBlock;

const CRAFTING_TICK_DELAY: i32 = 4;
const MAX_CRAFTING_TICKS: i32 = 6;
const CRAFTER_ADVANCEMENT_DIAMETER: f64 = 17.0;

thread_local! {
    /// `CrafterBlock.RECIPE_CACHE` (`new RecipeCache(10)`).
    static RECIPE_CACHE: RefCell<RecipeCache> = RefCell::new(RecipeCache::new(10));
}

/// `CrafterBlock.getPotentialResults(level, input)`.
pub fn get_potential_results(level: &ServerLevel, input: &CraftingInput) -> Option<RecipeHolder> {
    RECIPE_CACHE.with(|cache| cache.borrow_mut().get(level, input))
}

/// `mc_data`'s `FrontAndTop` (the property value) as the `mc_core` one that has the geometry (same constant order).
fn core_orientation(state: BlockState) -> FrontAndTop {
    FrontAndTop::ALL[state.get(props::ORIENTATION).ordinal()]
}

fn with_orientation(state: BlockState, orientation: FrontAndTop) -> BlockState {
    state.set(
        props::ORIENTATION,
        mc_data::enums::FrontAndTop::ALL[orientation.ordinal()],
    )
}

/// `setBlockEntityTriggered(blockEntity, triggered)`.
fn set_block_entity_triggered(level: &mut ServerLevel, pos: BlockPos, triggered: bool) {
    if let Some(crafter) = level
        .get_block_entity_check_mut(pos)
        .and_then(|be| be.as_any_mut().downcast_mut::<CrafterBlockEntity>())
    {
        crafter.set_triggered(triggered);
    }
}

impl BlockBehaviour for CrafterBlock {
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        with_orientation(
            state,
            rotation
                .rotation()
                .rotate_front_and_top(core_orientation(state))
                .expect("rotated jigsaw orientation"),
        )
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        with_orientation(
            state,
            mirror
                .rotation()
                .rotate_front_and_top(core_orientation(state))
                .expect("mirrored jigsaw orientation"),
        )
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
            .and_then(|be| be.as_any().downcast_ref::<CrafterBlockEntity>())
            .map_or(0, CrafterBlockEntity::get_redstone_signal)
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
        let should_trigger = has_neighbor_signal(level, pos);
        let is_triggered = state.get(props::TRIGGERED);
        if should_trigger && !is_triggered {
            level.schedule_tick_block(pos, state.block(), CRAFTING_TICK_DELAY);
            level.set_block_flags(pos, state.set(props::TRIGGERED, true), 2);
            set_block_entity_triggered(level, pos, true);
        } else if !should_trigger && is_triggered {
            level.set_block_flags(
                pos,
                state
                    .set(props::TRIGGERED, false)
                    .set(props::CRAFTING, false),
                2,
            );
            set_block_entity_triggered(level, pos, false);
        }
    }

    fn tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        dispense_from(level, state, pos);
    }

    fn get_state_for_placement(
        &self,
        block: Block,
        level: &mut ServerLevel,
        context: &BlockPlaceContext,
    ) -> Option<BlockState> {
        let nearest_looking_direction = context.get_nearest_looking_direction().get_opposite();
        let vertical_direction = match nearest_looking_direction {
            Direction::Down => context.get_horizontal_direction().get_opposite(),
            Direction::Up => context.get_horizontal_direction(),
            _ => Direction::Up,
        };
        let orientation =
            FrontAndTop::from_front_and_top(nearest_looking_direction, vertical_direction)?;
        Some(with_orientation(block.default_state(), orientation).set(
            props::TRIGGERED,
            has_neighbor_signal(level, context.get_clicked_pos()),
        ))
    }

    fn set_placed_by(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        _placer: Option<&mut ServerPlayer>,
        _stack: &ItemStack,
    ) {
        if state.get(props::TRIGGERED) {
            level.schedule_tick_block(pos, state.block(), CRAFTING_TICK_DELAY);
        }
    }

    /// `Containers.updateNeighboursAfterDestroy(state, level, pos)`.
    fn affect_neighbors_after_removal(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        _moved_by_piston: bool,
    ) {
        level.update_neighbour_for_output_signal(pos, state.block());
    }

    fn use_without_item(
        &self,
        _state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        player: &mut ServerPlayer,
        _hit: &BlockHitResult,
    ) -> InteractionResult {
        let provider = level
            .get_block_entity(pos)
            .filter(|be| be.as_any().is::<CrafterBlockEntity>())
            .and_then(|be| be.menu_provider());
        if let Some(provider) = provider {
            player.open_menu(level, &*provider);
        }
        InteractionResult::SUCCESS
    }
}

/// `CrafterBlock.dispenseFrom(state, level, pos)`.
fn dispense_from(level: &mut ServerLevel, state: BlockState, pos: BlockPos) {
    level.with_block_entity(pos, |level, be| {
        let Some(crafter) = be.as_any_mut().downcast_mut::<CrafterBlockEntity>() else {
            return;
        };
        let input = crafter.as_craft_input();
        let Some(picked_recipe) = get_potential_results(level, &input) else {
            level.level_event(None, 1050, pos, 0);
            return;
        };
        let mut results = picked_recipe.value.assemble_crafting(&input);
        if results.is_empty() {
            level.level_event(None, 1050, pos, 0);
            return;
        }
        crafter.set_crafting_ticks_remaining(MAX_CRAFTING_TICKS);
        level.set_block_flags(pos, state.set(props::CRAFTING, true), 2);
        behaviour(results.item()).on_crafted_post_process(&mut results, level);
        dispense_item(level, pos, crafter, results, state, &picked_recipe);
        for remaining_item in picked_recipe.value.get_remaining_items(&input) {
            if !remaining_item.is_empty() {
                dispense_item(level, pos, crafter, remaining_item, state, &picked_recipe);
            }
        }
        for item in crafter.items_mut() {
            if !item.is_empty() {
                item.shrink(1);
            }
        }
        Container::set_changed(crafter);
        // the comparators beside the crafter read it once it is back in its chunk
        deferred::push(changed_log::flush);
    });
}

/// `CrafterBlock.dispenseItem(level, pos, blockEntity, results, blockState, recipe)`.
fn dispense_item(
    level: &mut ServerLevel,
    pos: BlockPos,
    crafter: &CrafterBlockEntity,
    results: ItemStack,
    state: BlockState,
    recipe: &RecipeHolder,
) {
    let direction = core_orientation(state).front();
    let from_direction = Some(direction.get_opposite());
    let mut remaining = results.copy();
    if let Some(into) = get_container_at(level, pos.relative(direction)) {
        // `into instanceof CrafterBlockEntity || results.getCount() > into.getMaxStackSize(results)`
        let one_at_a_time = with_container(level, &into, |_, container| {
            container.as_any().is::<CrafterBlockEntity>()
                || results.count() > container.get_max_stack_size_for(&results)
        });
        match one_at_a_time {
            Some(true) => {
                while !remaining.is_empty() {
                    let copy = remaining.copy_with_count(1);
                    let left = with_container(level, &into, |level, container| {
                        add_item(level, None, container, copy, from_direction)
                    });
                    if left.is_none_or(|left| !left.is_empty()) {
                        break;
                    }
                    remaining.shrink(1);
                }
            }
            Some(false) => {
                while !remaining.is_empty() {
                    let old_size = remaining.count();
                    let Some(left) = with_container(level, &into, |level, container| {
                        add_item(level, None, container, remaining.copy(), from_direction)
                    }) else {
                        break;
                    };
                    remaining = left;
                    if old_size == remaining.count() {
                        break;
                    }
                }
            }
            None => {}
        }
    }
    if !remaining.is_empty() {
        let center_pos = Vec3::at_center_of(&pos);
        let item_spawn_offset = center_pos.relative(direction, 0.7);
        spawn_item(level, remaining, 6, direction, item_spawn_offset);
        let area = AABB::of_size(
            center_pos,
            CRAFTER_ADVANCEMENT_DIAMETER,
            CRAFTER_ADVANCEMENT_DIAMETER,
            CRAFTER_ADVANCEMENT_DIAMETER,
        );
        for id in level.get_entities_of_class(&area, |e| e.is_player(), |e| !e.is_spectator()) {
            level.with_player(id, |level, player| {
                CriteriaTriggers::CRAFTER_RECIPE_CRAFTED.trigger(
                    level,
                    player,
                    &recipe.id,
                    crafter.items(),
                );
            });
        }
        level.level_event(None, 1049, pos, 0);
        level.level_event(None, 2010, pos, direction.get_3d_data_value() as i32);
    }
}

register_block_class!("CrafterBlock", |_| Box::new(CrafterBlock));
