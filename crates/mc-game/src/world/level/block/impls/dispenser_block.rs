//! `net.minecraft.world.level.block.DispenserBlock` (the dropper extends it, see `dropper_block`).
use super::prelude::*;
use crate::server::level::ServerLevel;
use crate::world::block_entity::{BlockEntity, DispenserBlockEntity, changed_log};
use crate::world::container::Container;
use crate::world::dispenser::{BlockSource, get_dispense_method};
use crate::world::item::BlockPlaceContext;
use crate::world::level::gameevent::GameEventContext;
use crate::world::level::redstone::Orientation;
use crate::world::level::signal_getter::has_neighbor_signal;
use mc_data::BlockEntityType;
use mc_data::GameEvent;

pub struct DispenserBlock;

/// `Containers.updateNeighboursAfterDestroy(state, level, pos)`.
pub fn update_neighbours_after_destroy(state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
    level.update_neighbour_for_output_signal(pos, state.block());
}

/// `DispenserBlock.TRIGGER_DURATION`.
const TRIGGER_DURATION: i32 = 4;

impl BlockBehaviour for DispenserBlock {
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::FACING, mirror)
    }

    fn get_state_for_placement(&self, block: Block, _level: &mut ServerLevel, context: &BlockPlaceContext) -> Option<BlockState> {
        Some(block.default_state().set(props::FACING, context.get_nearest_looking_direction().get_opposite()))
    }

    fn neighbor_changed(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, block: Block, _orientation: Option<Orientation>, _moved_by_piston: bool) {
        let _ = block;
        let should_trigger = has_neighbor_signal(level, pos) || has_neighbor_signal(level, pos.above());
        let is_triggered = state.get(props::TRIGGERED);
        if should_trigger && !is_triggered {
            level.schedule_tick_block(pos, state.block(), TRIGGER_DURATION);
            level.set_block_flags(pos, state.set(props::TRIGGERED, true), 2);
        } else if !should_trigger && is_triggered {
            level.set_block_flags(pos, state.set(props::TRIGGERED, false), 2);
        }
    }

    fn tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        dispense_from(level, state, pos);
    }

    fn affect_neighbors_after_removal(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, _moved_by_piston: bool) {
        update_neighbours_after_destroy(state, level, pos);
    }

    fn get_analog_output_signal(&self, _state: BlockState, level: &ServerLevel, pos: BlockPos, _direction: Direction) -> i32 {
        crate::world::container::get_redstone_signal_from_block_entity(level.get_block_entity(pos))
    }
}

/// `DispenserBlock.dispenseFrom(level, state, pos)`.
fn dispense_from(level: &mut ServerLevel, state: BlockState, pos: BlockPos) {
    let taken = level.take_block_entity(pos).filter(|be| be.get_type() == BlockEntityType::DISPENSER);
    let Some(mut be) = taken else {
        log::warn!("Ignoring dispensing attempt for Dispenser without matching block entity at {}", pos.to_short_string());
        return;
    };
    be.unpack_loot_table(level, None);
    if let Some(dispenser) = be.as_any_mut().downcast_mut::<DispenserBlockEntity>() {
        let slot = dispenser.get_random_slot(level.random());
        if slot < 0 {
            level.level_event(None, 1001, pos, 0);
            let context = GameEventContext::of_state(Some(dispenser.get_block_state()));
            level.game_event_at_block(GameEvent::BLOCK_ACTIVATE, pos, &context);
        } else {
            let slot = slot as usize;
            let item_stack = dispenser.get_item(slot).clone();
            let behavior = get_dispense_method(level, &item_stack);
            let mut source = BlockSource { level, pos, state, block_entity: dispenser };
            let result = behavior.dispense(&mut source, item_stack);
            dispenser.set_item(slot, result);
        }
    }
    changed_log::flush(level);
    level.put_back_block_entity(pos, be);
}

register_block_class!("DispenserBlock", |_| Box::new(DispenserBlock));
