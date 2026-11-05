//! `net.minecraft.world.level.block.TestBlock`.
use super::prelude::*;
use crate::server::level::ServerLevel;
use crate::world::block_entity::test_block_entity;
use crate::world::item::{InteractionResult, ItemStack};
use mc_data::enums::TestBlockMode;
use mc_protocol::components::values::containers::{BLOCK_STATE, BlockItemStateProperties};

pub struct TestBlock;

impl BlockBehaviour for TestBlock {
    /// `useWithoutItem`: the screen opens client side; the server only decides whether the click is consumed.
    fn use_without_item(
        &self,
        _state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        player: &mut crate::world::entity::ServerPlayer,
        _hit: &mc_core::phys::BlockHitResult,
    ) -> InteractionResult {
        if test_block_entity::get(level, pos).is_some() {
            if !player.can_use_game_master_blocks() { InteractionResult::PASS } else { InteractionResult::SUCCESS }
        } else {
            InteractionResult::PASS
        }
    }

    fn tick(&self, _state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        test_block_entity::reset(level, pos);
    }

    fn neighbor_changed(
        &self,
        _state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        _block: Block,
        _orientation: Option<crate::world::level::redstone::Orientation>,
        _moved_by_piston: bool,
    ) {
        let Some(test_block) = test_block_entity::get(level, pos) else { return };
        if test_block.get_mode() != TestBlockMode::Start {
            let is_powered = test_block.is_powered();
            let should_trigger = crate::world::level::signal_getter::has_neighbor_signal(&*level, pos);
            if should_trigger && !is_powered {
                if let Some(be) = test_block_entity::get(level, pos) {
                    be.set_powered(true);
                }
                test_block_entity::trigger(level, pos);
            } else if !should_trigger && is_powered {
                if let Some(be) = test_block_entity::get(level, pos) {
                    be.set_powered(false);
                }
            }
        }
    }

    fn own_signal(&self, state: BlockState, level: &dyn BlockGetter, pos: BlockPos) -> i32 {
        if state.get(props::TEST_BLOCK_MODE) != TestBlockMode::Start {
            return 0;
        }
        match level.get_block_entity(pos).and_then(|be| be.as_any().downcast_ref::<crate::world::block_entity::TestBlockEntity>()) {
            Some(test_block) if test_block.is_powered() => 15,
            _ => 0,
        }
    }

    fn get_clone_item_stack(&self, state: BlockState, _level: &ServerLevel, _pos: BlockPos, _include_data: bool) -> ItemStack {
        let mut stack = ItemStack::new(Block::TEST_BLOCK.item().expect("test block item"));
        set_mode_on_stack(&mut stack, state.get(props::TEST_BLOCK_MODE));
        stack
    }
}

/// `setModeOnStack(itemStack, mode)`.
pub fn set_mode_on_stack(stack: &mut ItemStack, mode: TestBlockMode) {
    let mut properties: BlockItemStateProperties = stack.get(&BLOCK_STATE).cloned().unwrap_or_default();
    let value = mode.serialized_name().expect("serialized").to_owned();
    match properties.properties.iter_mut().find(|(k, _)| k == "mode") {
        Some(entry) => entry.1 = value,
        None => properties.properties.push(("mode".to_owned(), value)),
    }
    stack.set(BLOCK_STATE, properties);
}

register_block_class!("TestBlock", |_| Box::new(TestBlock));
