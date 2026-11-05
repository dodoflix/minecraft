//! `net.minecraft.world.level.block.TestInstanceBlock`.
use super::prelude::*;
use crate::server::level::ServerLevel;
use crate::world::item::InteractionResult;
use mc_data::BlockEntityType;

pub struct TestInstanceBlock;

impl BlockBehaviour for TestInstanceBlock {
    /// `useWithoutItem`: the screen opens client side; the server only decides whether the click is consumed.
    fn use_without_item(
        &self,
        _state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        player: &mut crate::world::entity::ServerPlayer,
        _hit: &mc_core::phys::BlockHitResult,
    ) -> InteractionResult {
        let is_test_instance = level.get_block_entity(pos).is_some_and(|be| be.get_type() == BlockEntityType::TEST_INSTANCE_BLOCK);
        if is_test_instance {
            if !player.can_use_game_master_blocks() { InteractionResult::PASS } else { InteractionResult::SUCCESS }
        } else {
            InteractionResult::PASS
        }
    }
}

register_block_class!("TestInstanceBlock", |_| Box::new(TestInstanceBlock));
