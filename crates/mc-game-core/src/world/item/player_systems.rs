//! Registration of the item/menu parts of the play-packet dispatcher (`ItemSystems`, container/pick-item
//! `PlayHandlers`): the `ServerGamePacketListenerImpl` side of items, hands and menus.

use super::{InteractionHand, InteractionResult, ItemStack, is_item_enabled};
use crate::server::network::game_packets::PlayCtx;
use crate::server::network::game_packets::hooks::{ITEM_SYSTEMS, ItemSystems, PLAY_HANDLERS, PlayHandlers, ServerboundBlockHitResult, SwingAnimationRef};
use crate::world::entity::player::menu_ctx::PlayerCtx;
use crate::world::inventory::{
    AbstractContainerMenu, CreativeSlotResult, PlayerMenus, handle_container_button_click, handle_container_click, handle_set_creative_mode_slot, try_pick_item,
};
use crate::world::level::block::{BlockStateExt, behaviour as block_behaviour};
use crate::world::level::{BlockGetter, LevelAccessor, LevelReader};
use linkme::distributed_slice;
use mc_core::phys::BlockHitResult;
use mc_protocol::components::{ATTACK_ANIMATION, BLOCK_ENTITY_DATA, INTERACT_ANIMATION, TypedEntityData};
use mc_protocol::packets::play::{
    ClientboundSetHeldSlotPacket, SwingAnimation, SwingAnimationType, ServerboundContainerButtonClickPacket,
    ServerboundContainerClickPacket, ServerboundContainerClosePacket, ServerboundPickItemFromBlockPacket, ServerboundPickItemFromEntityPacket,
    ServerboundSelectBundleItemPacket, ServerboundSetCreativeModeSlotPacket,
};

/// Packs a `SwingAnimation` into the opaque `SwingAnimationRef` (bit 31 = "set", 24..26 = type, 0..24 = duration).
fn encode_swing(a: SwingAnimation) -> SwingAnimationRef {
    SwingAnimationRef(1 << 31 | (a.kind as u32 & 3) << 24 | (a.duration as u32 & 0xFF_FFFF))
}

fn decode_swing(r: SwingAnimationRef) -> SwingAnimation {
    if r.0 >> 31 == 0 {
        return crate::world::entity::player::item_use::SWING_ANIMATION_DEFAULT;
    }
    let kind = match (r.0 >> 24) & 3 {
        1 => SwingAnimationType::Whack,
        2 => SwingAnimationType::Stab,
        _ => SwingAnimationType::None,
    };
    SwingAnimation { kind, duration: (r.0 & 0xFF_FFFF) as i32 }
}

struct ItemPlayerSystems;

impl ItemSystems for ItemPlayerSystems {
    fn stop_using_item(&self, ctx: &mut PlayCtx) {
        ctx.player.stop_using_item(ctx.level);
    }

    fn release_using_item(&self, ctx: &mut PlayCtx) {
        ctx.player.release_using_item(ctx.level);
    }

    fn get_used_item_hand(&self, ctx: &mut PlayCtx) -> Option<InteractionHand> {
        Some(ctx.player.get_used_item_hand())
    }

    fn swap_item_with_offhand(&self, ctx: &mut PlayCtx) {
        let in_offhand = ctx.player.get_item_in_hand(InteractionHand::OffHand);
        let in_main = ctx.player.get_item_in_hand(InteractionHand::MainHand);
        ctx.player.set_item_in_hand_slot(ctx.level, InteractionHand::OffHand, in_main);
        ctx.player.set_item_in_hand_slot(ctx.level, InteractionHand::MainHand, in_offhand);
    }

    fn drop_selected_item(&self, ctx: &mut PlayCtx, all: bool) {
        ctx.player.drop_selected(ctx.level, all);
    }

    fn item_in_hand_enabled(&self, ctx: &mut PlayCtx, hand: InteractionHand) -> bool {
        is_item_enabled(&ctx.player.get_item_in_hand(hand), ctx.level.enabled_features())
    }

    fn item_in_hand_is_empty(&self, ctx: &mut PlayCtx, hand: InteractionHand) -> bool {
        ctx.player.get_item_in_hand(hand).is_empty()
    }

    /// `ServerGamePacketListenerImpl.wasBlockPlacementAttempt(player, stack)`.
    fn was_block_placement_attempt(&self, ctx: &mut PlayCtx, hand: InteractionHand) -> bool {
        let stack = ctx.player.get_item_in_hand(hand);
        if stack.is_empty() {
            return false;
        }
        let classes = stack.item().data().classes;
        (classes.contains(&"BlockItem") || classes.contains(&"BucketItem")) && !ctx.player.player.cooldowns.is_on_cooldown(&stack)
    }

    fn swing_animation(&self, ctx: &mut PlayCtx, hand: InteractionHand, attack: bool) -> SwingAnimationRef {
        let stack = ctx.player.get_item_in_hand(hand);
        let key = if attack { &ATTACK_ANIMATION } else { &INTERACT_ANIMATION };
        encode_swing(stack.get(key).copied().unwrap_or(crate::world::entity::player::item_use::SWING_ANIMATION_DEFAULT))
    }

    fn swing(&self, ctx: &mut PlayCtx, hand: InteractionHand, animation: SwingAnimationRef, send_to_swinging_entity: bool) {
        ctx.player.swing(ctx.level, hand, decode_swing(animation), send_to_swinging_entity);
    }

    fn swing_and_reset_attack_strength(&self, ctx: &mut PlayCtx, hand: InteractionHand, animation: SwingAnimationRef, send_to_swinging_entity: bool) {
        if ctx.player.swing(ctx.level, hand, decode_swing(animation), send_to_swinging_entity) {
            ctx.player.reset_attack_strength_ticker();
        }
    }

    fn use_item_on(&self, ctx: &mut PlayCtx, hand: InteractionHand, hit: &ServerboundBlockHitResult) -> InteractionResult {
        let hit = BlockHitResult {
            location: hit.location(),
            direction: hit.direction,
            block_pos: hit.block_pos,
            miss: false,
            inside: hit.inside,
            world_border_hit: hit.world_border_hit,
        };
        // block use code resolves components (written books on a lectern) through the installed server
        let (player, level) = (&mut *ctx.player, &mut *ctx.level);
        crate::commands::server_access::enter_server(&mut *ctx.server, || player.use_item_on(level, hand, &hit))
    }

    fn use_item(&self, ctx: &mut PlayCtx, hand: InteractionHand) -> InteractionResult {
        let result = ctx.player.use_item(ctx.level, hand);
        super::flush_open_item_gui(ctx);
        result
    }
}

#[distributed_slice(ITEM_SYSTEMS)]
static ITEM_PLAYER_SYSTEMS: &dyn ItemSystems = &ItemPlayerSystems;

struct MenuHandlers;

/// Runs `f` on the player's current menu with the player context (`PlayerCtx` implements `MenuPlayer`).
pub(super) fn with_current_menu<R>(ctx: &mut PlayCtx, f: impl FnOnce(&mut dyn AbstractContainerMenu, &mut PlayerCtx) -> R) -> R {
    let mut pctx = PlayerCtx { player: &mut *ctx.player, level: &mut *ctx.level };
    PlayerMenus::with_current(&mut pctx, |c| &mut c.player.player.menus, f)
}

impl PlayHandlers for MenuHandlers {
    fn handle_container_close(&self, ctx: &mut PlayCtx, _packet: &ServerboundContainerClosePacket) {
        ctx.player.do_close_container(ctx.level);
    }

    fn handle_container_click(&self, ctx: &mut PlayCtx, packet: &ServerboundContainerClickPacket) {
        let dead = ctx.player.is_dead_or_dying();
        let result = with_current_menu(ctx, |menu, p| handle_container_click(menu, p, packet, dead));
        if let Err(e) = result {
            log::warn!("Container click failed: slot {} of {} slots", e.slot_index, e.slot_count);
        }
    }

    fn handle_container_button_click(&self, ctx: &mut PlayCtx, packet: &ServerboundContainerButtonClickPacket) {
        with_current_menu(ctx, |menu, p| handle_container_button_click(menu, p, packet.container_id, packet.button_id));
    }

    fn handle_bundle_item_selected(&self, ctx: &mut PlayCtx, packet: &ServerboundSelectBundleItemPacket) {
        with_current_menu(ctx, |menu, p| menu.set_selected_bundle_item_index(p, packet.slot_id, packet.selected_item_index));
    }

    fn handle_set_creative_mode_slot(&self, ctx: &mut PlayCtx, packet: &ServerboundSetCreativeModeSlotPacket) {
        let slot = i32::from(packet.slot_num);
        let stack = packet.item_stack.clone();
        let mut pctx = PlayerCtx { player: &mut *ctx.player, level: &mut *ctx.level };
        let mut menus = std::mem::replace(&mut pctx.player.player.menus, PlayerMenus::new());
        let r = handle_set_creative_mode_slot(menus.inventory_menu_mut(), &mut pctx, slot, stack, true);
        pctx.player.player.menus = menus;
        debug_assert!(r != CreativeSlotResult::Throttled);
    }

    fn handle_creative_drop(&self, ctx: &mut PlayCtx, packet: &ServerboundSetCreativeModeSlotPacket) {
        ctx.player.drop(ctx.level, packet.item_stack.clone(), true, crate::world::inventory::Prediction::Predicted);
    }

    fn handle_pick_item_from_block(&self, ctx: &mut PlayCtx, packet: &ServerboundPickItemFromBlockPacket) {
        let pos = packet.pos;
        if !ctx.player.is_within_block_interaction_range(pos, 1.0) || !ctx.level.is_loaded(pos) {
            return;
        }
        let state = ctx.level.get_block_state(pos);
        let include_data = ctx.player.player.abilities.instabuild && packet.include_data;
        let mut stack = block_behaviour(state.block()).get_clone_item_stack(state, ctx.level, pos, include_data);
        if stack.is_empty() {
            return;
        }
        if include_data && state.has_block_entity() {
            // `addBlockDataToItem`: the block entity's custom data as `block_entity_data`
            if let Some(be) = ctx.level.get_block_entity_immediate(pos) {
                let mut tag = mc_nbt::CompoundTag::new();
                be.save_additional(&mut tag);
                if !tag.is_empty() {
                    stack.set(BLOCK_ENTITY_DATA, TypedEntityData::of(be.get_type(), tag));
                }
            }
        }
        try_pick(ctx, stack);
    }

    fn handle_pick_item_from_entity(&self, _ctx: &mut PlayCtx, _packet: &ServerboundPickItemFromEntityPacket) {
        // `Entity.getPickResult()` (spawn egg / boat / minecart item per entity type) belongs to the entity code.
    }
}

/// `ServerGamePacketListenerImpl.tryPickItem(stack)`.
fn try_pick(ctx: &mut PlayCtx, stack: ItemStack) {
    if !is_item_enabled(&stack, ctx.level.enabled_features()) {
        return;
    }
    let mut pctx = PlayerCtx { player: &mut *ctx.player, level: &mut *ctx.level };
    let slot = try_pick_item(&mut pctx, stack);
    pctx.player.send(&ClientboundSetHeldSlotPacket { slot });
    let mut menus = std::mem::replace(&mut pctx.player.player.menus, PlayerMenus::new());
    menus.inventory_menu_mut().broadcast_changes(&mut pctx);
    pctx.player.player.menus = menus;
}

#[distributed_slice(PLAY_HANDLERS)]
static MENU_HANDLERS: &dyn PlayHandlers = &MenuHandlers;
