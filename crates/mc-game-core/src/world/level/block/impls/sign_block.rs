//! `net.minecraft.world.level.block.SignBlock` (abstract base of the sign blocks): the logic shared by
//! `StandingSignBlock`, `WallSignBlock`, `CeilingHangingSignBlock` and `WallHangingSignBlock`.
use super::prelude::*;
use crate::server::level::ServerLevel;
use crate::world::block_entity::BlockEntity;
use crate::world::block_entity::sign_block_entity::{SignBlockEntity, SignTextExt, SignTextSlot};
use crate::world::entity::{Entity, ServerPlayer};
use crate::world::item::{InteractionResult, ItemStack};
use crate::world::level::block::rotation_segment;
use mc_core::phys::Vec3;
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

/// `SignBlock.isPossibleToRespawnInThis` + `useWithoutItem` / `setPlacedBy`, expanded into the `BlockBehaviour` impl of every
/// concrete sign class.
#[macro_export]
macro_rules! sign_block_common {
    () => {
        fn is_possible_to_respawn_in_this(&self, _state: BlockState) -> bool {
            true
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
