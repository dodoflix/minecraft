//! `net.minecraft.world.level.block.DropperBlock`: a dispenser that drops (or inserts into the container in front).
use mc_game_core::register_block_class;
use super::dispenser_block::DispenserBlock;
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::server::level::ServerLevel;
use crate::world::block_entity::hopper_block_entity::{add_item, get_container_at, with_container};
use mc_game_core::world::block_entity::{BlockEntity, DispenserBlockEntity, changed_log};
use mc_game_core::world::container::Container;
use mc_game_core::world::dispenser::{BlockSource, DispenseItemBehavior, default_behavior};
use mc_game_core::world::level::redstone::Orientation;
use mc_data::BlockEntityType;

pub struct DropperBlock;

impl BlockBehaviour for DropperBlock {
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        DispenserBlock.rotate(state, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        DispenserBlock.mirror(state, mirror)
    }
    fn get_state_for_placement(&self, block: Block, level: &mut ServerLevel, context: &mc_game_core::world::item::BlockPlaceContext) -> Option<BlockState> {
        DispenserBlock.get_state_for_placement(block, level, context)
    }
    fn neighbor_changed(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, block: Block, orientation: Option<Orientation>, moved_by_piston: bool) {
        DispenserBlock.neighbor_changed(state, level, pos, block, orientation, moved_by_piston);
    }
    fn tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        dispense_from(level, state, pos);
    }
    fn use_without_item(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, player: &mut mc_game_core::world::entity::ServerPlayer, hit: &mc_core::phys::BlockHitResult) -> mc_game_core::world::item::InteractionResult {
        DispenserBlock.use_without_item(state, level, pos, player, hit)
    }
    fn affect_neighbors_after_removal(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, moved_by_piston: bool) {
        DispenserBlock.affect_neighbors_after_removal(state, level, pos, moved_by_piston);
    }
    fn get_analog_output_signal(&self, state: BlockState, level: &ServerLevel, pos: BlockPos, direction: Direction) -> i32 {
        DispenserBlock.get_analog_output_signal(state, level, pos, direction)
    }
}

/// `DropperBlock.dispenseFrom(level, state, pos)`.
fn dispense_from(level: &mut ServerLevel, state: BlockState, pos: BlockPos) {
    let taken = level.take_block_entity(pos).filter(|be| be.get_type() == BlockEntityType::DROPPER);
    let Some(mut be) = taken else {
        log::warn!("Ignoring dispensing attempt for Dropper without matching block entity at {}", pos.to_short_string());
        return;
    };
    be.unpack_loot_table(level, None);
    if let Some(dropper) = be.as_any_mut().downcast_mut::<DispenserBlockEntity>() {
        let slot = dropper.get_random_slot(level.random());
        if slot < 0 {
            level.level_event(None, 1001, pos, 0);
        } else {
            let slot = slot as usize;
            let item_stack = dropper.get_item(slot).clone();
            if !item_stack.is_empty() {
                let direction = BlockGetter::get_block_state(level, pos).get(props::FACING);
                let remaining = match get_container_at(level, pos.relative(direction)) {
                    None => {
                        let mut source = BlockSource { level, pos, state, block_entity: dropper };
                        default_behavior().dispense(&mut source, item_stack)
                    }
                    Some(into) => {
                        let moved = with_container(level, &into, |level, container| {
                            add_item(level, Some(&*dropper as &dyn Container), container, item_stack.copy_with_count(1), Some(direction.get_opposite()))
                        })
                        .map_or(false, |remaining| remaining.is_empty());
                        // `with_container` returning `None` (owner vanished) cannot happen: it was just resolved
                        let mut remaining = item_stack.copy();
                        if moved {
                            remaining.shrink(1);
                        }
                        remaining
                    }
                };
                dropper.set_item(slot, remaining);
            }
        }
    }
    changed_log::flush(level);
    level.put_back_block_entity(pos, be);
}

register_block_class!("DropperBlock", |_| Box::new(DropperBlock));
