//! `net.minecraft.world.level.block.SignBlock` (abstract base of the sign blocks): the logic shared by
//! `StandingSignBlock`, `WallSignBlock`, `CeilingHangingSignBlock` and `WallHangingSignBlock`.
use super::prelude::*;
use crate::server::level::ServerLevel;
use crate::world::block_entity::BlockEntity;
use crate::world::block_entity::sign_block_entity::{SignBlockEntity, SignTextExt, SignTextSlot};
use crate::world::entity::{Entity, ServerPlayer};
use crate::world::item::{
    InteractionHand, InteractionResult, ItemStack, ItemStackExt, behaviour as item_behaviour,
};
use crate::world::level::block::rotation_segment;
use crate::world::level::gameevent::GameEventContext;
use mc_core::phys::Vec3;
use mc_data::GameEvent;
use mc_protocol::packets::play::ClientboundOpenSignEditorPacket;
use mc_protocol::packets::play::level::{ClientboundBlockUpdatePacket, SoundSource};

/// `SignBlock.updateShape`.
pub fn update_shape(state: BlockState, level: &mut dyn LevelAccessor, pos: BlockPos) -> BlockState {
    if state.get(props::WATERLOGGED) {
        helpers::schedule_water_tick(level, pos);
    }
    state
}

/// `getYRotationDegrees(state)` of the sign classes (`None` for any other block, vanilla's `instanceof SignBlock`).
pub fn y_rotation_degrees(state: BlockState) -> Option<f32> {
    let block = state.block();
    let classes = block.data().classes;
    if classes.contains(&"StandingSignBlock") || classes.contains(&"CeilingHangingSignBlock") {
        Some(rotation_segment::convert_to_degrees(
            state.get(props::ROTATION_16),
        ))
    } else if classes.contains(&"WallSignBlock") || classes.contains(&"WallHangingSignBlock") {
        Some(state.get(props::HORIZONTAL_FACING).to_y_rot())
    } else {
        None
    }
}

/// `getSignHitboxCenterPosition(state)`: `WallSignBlock` uses the centre of its shape's bounds, the others `(0.5, 0.5, 0.5)`.
pub fn sign_hitbox_center_position(state: BlockState) -> Vec3 {
    if state.block().data().classes.contains(&"WallSignBlock") {
        crate::world::level::shapes::voxel_shape(state.shape())
            .bounds()
            .get_center()
    } else {
        Vec3::new(0.5, 0.5, 0.5)
    }
}

/// `otherPlayerIsEditingSign(player, sign)`.
fn other_player_is_editing_sign(player: &ServerPlayer, sign: &SignBlockEntity) -> bool {
    sign.get_player_who_may_edit()
        .is_some_and(|editor| editor != player.uuid())
}

/// `openTextEdit(player, sign, slot)`: `setAllowedPlayerEditor` + `ServerPlayer.openTextEdit`.
pub fn open_text_edit(
    level: &ServerLevel,
    player: &ServerPlayer,
    sign: &mut SignBlockEntity,
    slot: SignTextSlot,
) {
    sign.set_allowed_player_editor(Some(player.uuid()));
    let pos = sign.get_block_pos();
    player.send(&ClientboundBlockUpdatePacket {
        pos,
        block_state: level.get_block_state(pos),
    });
    player.send(&ClientboundOpenSignEditorPacket { pos, slot });
}

/// `SignBlock.useItemOn`. The held stack of the player is consumed through the player (vanilla mutates the shared stack).
pub fn use_item_on(
    stack: &ItemStack,
    level: &mut ServerLevel,
    pos: BlockPos,
    player: &mut ServerPlayer,
    hand: InteractionHand,
) -> InteractionResult {
    let result = level.with_block_entity(pos, |level, be| {
        let sign = be.as_any_mut().downcast_mut::<SignBlockEntity>()?;
        let applicator = item_behaviour(stack.item());
        let applicator = applicator.as_sign_applicator();
        let has_applicator_to_use = applicator.is_some() && player.player.abilities.may_build;
        Some(match applicator {
            Some(applicator)
                if has_applicator_to_use
                    && !sign.is_waxed()
                    && !other_player_is_editing_sign(player, sign) =>
            {
                let slot = sign.get_slot_player_is_facing(player.base().x(), player.base().z());
                let text = sign.get_text(slot).clone();
                if applicator.can_apply_to_sign(&text, stack, player)
                    && applicator.try_apply_to_sign(level, sign, slot, stack, player)
                {
                    sign.execute_click_commands_if_present(level, player, pos, slot);
                    player.award_stat_once(level, crate::stats::Stat::Used(stack.item()));
                    level.game_event_at_block(
                        GameEvent::BLOCK_CHANGE,
                        sign.get_block_pos(),
                        &GameEventContext::of(Some(player.base_id()), Some(sign.get_block_state())),
                    );
                    let mut held = player.get_item_in_hand(hand);
                    held.consume(1, player.player.abilities.instabuild);
                    player.set_item_in_hand(hand, held);
                    InteractionResult::SUCCESS
                } else {
                    InteractionResult::TRY_WITH_EMPTY_HAND
                }
            }
            _ => InteractionResult::TRY_WITH_EMPTY_HAND,
        })
    });
    result.flatten().unwrap_or(InteractionResult::PASS)
}

/// `SignBlock.useWithoutItem`.
pub fn use_without_item(
    level: &mut ServerLevel,
    pos: BlockPos,
    player: &mut ServerPlayer,
) -> InteractionResult {
    let result = level.with_block_entity(pos, |level, be| {
        let sign = be.as_any_mut().downcast_mut::<SignBlockEntity>()?;
        let slot = sign.get_slot_player_is_facing(player.base().x(), player.base().z());
        let executed_click_command =
            sign.execute_click_commands_if_present(level, player, pos, slot);
        Some(if sign.is_waxed() {
            level.play_sound(
                None,
                sign.get_block_pos(),
                sign.get_sign_interaction_failed_sound_event(),
                SoundSource::Blocks,
                1.0,
                1.0,
            );
            InteractionResult::SUCCESS_SERVER
        } else if executed_click_command {
            InteractionResult::SUCCESS_SERVER
        } else if !other_player_is_editing_sign(player, sign)
            && player.player.abilities.may_build
            && sign
                .get_text(slot)
                .has_editable_text(player.text_filtering_enabled)
        {
            open_text_edit(level, player, sign, slot);
            InteractionResult::SUCCESS_SERVER
        } else {
            InteractionResult::PASS
        })
    });
    result.flatten().unwrap_or(InteractionResult::PASS)
}

/// `SignBlock.setPlacedBy`: a freshly placed, editable sign opens its front side editor for the placing player.
pub fn set_placed_by(level: &mut ServerLevel, pos: BlockPos, placer: Option<&mut ServerPlayer>) {
    let Some(player) = placer else { return };
    level.with_block_entity(pos, |level, be| {
        if let Some(sign) = be.as_any_mut().downcast_mut::<SignBlockEntity>()
            && !sign.is_waxed()
            && sign
                .get_text(SignTextSlot::Front)
                .has_editable_text(player.text_filtering_enabled)
        {
            open_text_edit(level, player, sign, SignTextSlot::Front);
        }
    });
}

/// `SignBlock.isPossibleToRespawnInThis` + `useItemOn` / `useWithoutItem` / `setPlacedBy`, expanded into the `BlockBehaviour`
/// impl of every concrete sign class.
#[macro_export]
macro_rules! sign_block_common {
    () => {
        fn is_possible_to_respawn_in_this(&self, _state: BlockState) -> bool {
            true
        }
        fn use_item_on(
            &self,
            stack: &$crate::world::item::ItemStack,
            _state: BlockState,
            level: &mut $crate::server::level::ServerLevel,
            pos: BlockPos,
            player: &mut $crate::world::entity::ServerPlayer,
            hand: $crate::world::item::InteractionHand,
            _hit: &mc_core::phys::BlockHitResult,
        ) -> $crate::world::item::InteractionResult {
            $crate::world::level::block::impls::sign_block::use_item_on(
                stack, level, pos, player, hand,
            )
        }
        fn use_without_item(
            &self,
            _state: BlockState,
            level: &mut $crate::server::level::ServerLevel,
            pos: BlockPos,
            player: &mut $crate::world::entity::ServerPlayer,
            _hit: &mc_core::phys::BlockHitResult,
        ) -> $crate::world::item::InteractionResult {
            $crate::world::level::block::impls::sign_block::use_without_item(level, pos, player)
        }
        fn set_placed_by(
            &self,
            _state: BlockState,
            level: &mut $crate::server::level::ServerLevel,
            pos: BlockPos,
            placer: Option<&mut $crate::world::entity::ServerPlayer>,
            _stack: &$crate::world::item::ItemStack,
        ) {
            $crate::world::level::block::impls::sign_block::set_placed_by(level, pos, placer)
        }
    };
}
pub use sign_block_common;

/// The `HangingSignItem` check of `shouldTryToChainAnotherHangingSign`.
fn is_hanging_sign_item(stack: &ItemStack) -> bool {
    !stack.is_empty() && stack.item().data().classes.contains(&"HangingSignItem")
}

/// `shouldTryToChainAnotherHangingSign` common part: the facing slot has no click commands and a hanging sign item is
/// held; the caller adds the side condition.
pub fn chains_hanging_sign(
    level: &mut ServerLevel,
    pos: BlockPos,
    player: &ServerPlayer,
    stack: &ItemStack,
    side_condition: impl FnOnce() -> bool,
) -> bool {
    let Some(can_execute) = level
        .get_block_entity(pos)
        .and_then(|be| be.as_any().downcast_ref::<SignBlockEntity>())
        .map(|sign| {
            let slot = sign.get_slot_player_is_facing(player.base().x(), player.base().z());
            sign.can_execute_click_commands(slot, player)
        })
    else {
        return false;
    };
    !can_execute && is_hanging_sign_item(stack) && side_condition()
}

/// `level.getFluidState(pos).is(Fluids.WATER)` (the `WATERLOGGED` value of a sign placed at `pos`).
pub fn waterlogged_by(level: &dyn BlockGetter, pos: BlockPos) -> bool {
    level.get_fluid_state(pos).get_type() == mc_data::fluid::Fluid::WATER
}
