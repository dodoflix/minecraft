//! `net.minecraft.world.level.block.VaultBlock`: key insertion, placement facing, `rotate` / `mirror`.
use super::prelude::*;
use crate::server::level::ServerLevel;
use crate::world::block_entity::vault::VaultConfig;
use crate::world::block_entity::vault_block_entity::{VaultBlockEntity, try_insert_key};
use crate::world::item::{BlockPlaceContext, InteractionHand, InteractionResult, ItemStack};
use mc_data::enums::VaultState;

pub struct VaultBlock;

impl BlockBehaviour for VaultBlock {
    fn use_item_on(
        &self,
        stack: &ItemStack,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        player: &mut crate::world::entity::ServerPlayer,
        hand: InteractionHand,
        _hit: &mc_core::phys::BlockHitResult,
    ) -> InteractionResult {
        if stack.is_empty() || state.get(props::VAULT_STATE) != VaultState::Active {
            return InteractionResult::TRY_WITH_EMPTY_HAND;
        }
        // the held stack is the player's own object in vanilla: the consumed copy is written back
        let mut held = stack.clone();
        let handled = level.with_block_entity(pos, |level, be| {
            let Some(vault) = be.as_any_mut().downcast_mut::<VaultBlockEntity>() else { return false };
            let VaultBlockEntity { server_data, shared_data, config, .. } = vault;
            let config: &VaultConfig = config;
            try_insert_key(level, pos, state, config, server_data, shared_data, player, &mut held);
            true
        });
        if handled != Some(true) {
            return InteractionResult::TRY_WITH_EMPTY_HAND;
        }
        if held.count() != stack.count() {
            player.set_item_in_hand(hand, held);
        }
        InteractionResult::SUCCESS_SERVER
    }

    fn get_state_for_placement(&self, block: Block, _level: &mut ServerLevel, context: &BlockPlaceContext) -> Option<BlockState> {
        Some(block.default_state().set(props::HORIZONTAL_FACING, context.get_horizontal_direction().get_opposite()))
    }

    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::HORIZONTAL_FACING, mirror)
    }
}

register_block_class!("VaultBlock", |_| Box::new(VaultBlock));
