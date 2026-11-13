//! Registration of the item/menu parts of the play-packet dispatcher (`ItemSystems`, container/pick-item
//! `PlayHandlers`): the `ServerGamePacketListenerImpl` side of items, hands and menus.

use super::{InteractionHand, InteractionResult, ItemStack, is_item_enabled};
use crate::commands::host::entity_display_name;
use crate::commands::profile_report::{avatar_profile, print_for_avatar};
use crate::server::level::ServerLevel;
use crate::server::network::game_packets::PlayCtx;
use crate::server::network::game_packets::hooks::{
    ITEM_SYSTEMS, ItemSystems, PLAY_HANDLERS, PlayHandlers, ServerboundBlockHitResult,
    SwingAnimationRef,
};
use crate::world::entity::player::menu_ctx::PlayerCtx;
use crate::world::entity::{Entity, ServerPlayer};
use crate::world::inventory::{
    AbstractContainerMenu, CreativeSlotResult, PlayerMenus, handle_container_button_click,
    handle_container_click, handle_container_slot_state_changed, handle_set_creative_mode_slot,
    try_pick_item,
};
use crate::world::level::block::{BlockStateExt, behaviour as block_behaviour};
use crate::world::level::{BlockGetter, LevelReader};
use linkme::distributed_slice;
use mc_core::phys::BlockHitResult;
use mc_data::DataComponentType;
use mc_protocol::components::{
    ATTACK_ANIMATION, BLOCK_ENTITY_DATA, INTERACT_ANIMATION, TypedEntityData,
};
use mc_protocol::packets::play::{
    ClientboundSetHeldSlotPacket, ServerboundContainerButtonClickPacket,
    ServerboundContainerClickPacket, ServerboundContainerClosePacket,
    ServerboundContainerSlotStateChangedPacket, ServerboundPickItemFromBlockPacket,
    ServerboundPickItemFromEntityPacket, ServerboundSelectBundleItemPacket,
    ServerboundSetCreativeModeSlotPacket, SwingAnimation, SwingAnimationType,
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
    SwingAnimation {
        kind,
        duration: (r.0 & 0xFF_FFFF) as i32,
    }
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
        ctx.player
            .set_item_in_hand_slot(ctx.level, InteractionHand::OffHand, in_main);
        ctx.player
            .set_item_in_hand_slot(ctx.level, InteractionHand::MainHand, in_offhand);
    }

    fn drop_selected_item(&self, ctx: &mut PlayCtx, all: bool) {
        ctx.player.drop_selected(ctx.level, all);
    }

    fn item_in_hand_enabled(&self, ctx: &mut PlayCtx, hand: InteractionHand) -> bool {
        is_item_enabled(
            &ctx.player.get_item_in_hand(hand),
            ctx.level.enabled_features(),
        )
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
        (classes.contains(&"BlockItem") || classes.contains(&"BucketItem"))
            && !ctx.player.player.cooldowns.is_on_cooldown(&stack)
    }

    fn swing_animation(
        &self,
        ctx: &mut PlayCtx,
        hand: InteractionHand,
        attack: bool,
    ) -> SwingAnimationRef {
        let stack = ctx.player.get_item_in_hand(hand);
        let key = if attack {
            &ATTACK_ANIMATION
        } else {
            &INTERACT_ANIMATION
        };
        encode_swing(
            stack
                .get(key)
                .copied()
                .unwrap_or(crate::world::entity::player::item_use::SWING_ANIMATION_DEFAULT),
        )
    }

    fn swing(
        &self,
        ctx: &mut PlayCtx,
        hand: InteractionHand,
        animation: SwingAnimationRef,
        send_to_swinging_entity: bool,
    ) {
        ctx.player.swing(
            ctx.level,
            hand,
            decode_swing(animation),
            send_to_swinging_entity,
        );
    }

    fn swing_and_reset_attack_strength(
        &self,
        ctx: &mut PlayCtx,
        hand: InteractionHand,
        animation: SwingAnimationRef,
        send_to_swinging_entity: bool,
    ) {
        if ctx.player.swing(
            ctx.level,
            hand,
            decode_swing(animation),
            send_to_swinging_entity,
        ) {
            ctx.player.reset_attack_strength_ticker();
        }
    }

    fn use_item_on(
        &self,
        ctx: &mut PlayCtx,
        hand: InteractionHand,
        hit: &ServerboundBlockHitResult,
    ) -> InteractionResult {
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
        let result = crate::commands::server_access::enter_server(&mut *ctx.server, || {
            player.use_item_on(level, hand, &hit)
        });
        // A spectator's PASS means the block is enabled and has no menu: vanilla's `Portal` branch comes next.
        if result == InteractionResult::Pass
            && ctx.player.game_mode.game_mode_for_player == mc_data::enums::GameType::Spectator
            && let Some(portal) = crate::server::players::dimension_change::spectator_portal_use(
                ctx.server,
                ctx.level,
                ctx.player,
                hit.block_pos,
            )
        {
            return portal;
        }
        result
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
pub(super) fn with_current_menu<R>(
    ctx: &mut PlayCtx,
    f: impl FnOnce(&mut dyn AbstractContainerMenu, &mut PlayerCtx) -> R,
) -> R {
    let mut pctx = PlayerCtx {
        player: &mut *ctx.player,
        level: &mut *ctx.level,
    };
    PlayerMenus::with_current(&mut pctx, |c| &mut c.player.player.menus, f)
}

impl PlayHandlers for MenuHandlers {
    fn handle_container_close(&self, ctx: &mut PlayCtx, _packet: &ServerboundContainerClosePacket) {
        ctx.player.do_close_container(ctx.level);
    }

    fn handle_container_click(&self, ctx: &mut PlayCtx, packet: &ServerboundContainerClickPacket) {
        let dead = ctx.player.is_dead_or_dying();
        let result =
            with_current_menu(ctx, |menu, p| handle_container_click(menu, p, packet, dead));
        if let Err(e) = result {
            log::warn!(
                "Container click failed: slot {} of {} slots",
                e.slot_index,
                e.slot_count
            );
        }
    }

    fn handle_container_button_click(
        &self,
        ctx: &mut PlayCtx,
        packet: &ServerboundContainerButtonClickPacket,
    ) {
        with_current_menu(ctx, |menu, p| {
            handle_container_button_click(menu, p, packet.container_id, packet.button_id)
        });
    }

    fn handle_container_slot_state_changed(
        &self,
        ctx: &mut PlayCtx,
        packet: &ServerboundContainerSlotStateChangedPacket,
    ) {
        with_current_menu(ctx, |menu, p| {
            handle_container_slot_state_changed(
                menu,
                p,
                packet.container_id,
                packet.slot_id,
                packet.new_state,
            )
        });
    }

    fn handle_bundle_item_selected(
        &self,
        ctx: &mut PlayCtx,
        packet: &ServerboundSelectBundleItemPacket,
    ) {
        with_current_menu(ctx, |menu, p| {
            menu.set_selected_bundle_item_index(p, packet.slot_id, packet.selected_item_index)
        });
    }

    fn handle_set_creative_mode_slot(
        &self,
        ctx: &mut PlayCtx,
        packet: &ServerboundSetCreativeModeSlotPacket,
    ) {
        let slot = i32::from(packet.slot_num);
        let stack = packet.item_stack.clone();
        let mut pctx = PlayerCtx {
            player: &mut *ctx.player,
            level: &mut *ctx.level,
        };
        let mut menus = std::mem::take(&mut pctx.player.player.menus);
        let r =
            handle_set_creative_mode_slot(menus.inventory_menu_mut(), &mut pctx, slot, stack, true);
        pctx.player.player.menus = menus;
        debug_assert!(r != CreativeSlotResult::Throttled);
    }

    fn handle_creative_drop(
        &self,
        ctx: &mut PlayCtx,
        packet: &ServerboundSetCreativeModeSlotPacket,
    ) {
        ctx.player.drop(
            ctx.level,
            packet.item_stack.clone(),
            true,
            crate::world::inventory::Prediction::Predicted,
        );
    }

    fn handle_pick_item_from_block(
        &self,
        ctx: &mut PlayCtx,
        packet: &ServerboundPickItemFromBlockPacket,
    ) {
        pick_item_from_block(ctx.player, ctx.level, packet);
    }

    fn handle_pick_item_from_entity(
        &self,
        ctx: &mut PlayCtx,
        packet: &ServerboundPickItemFromEntityPacket,
    ) {
        // `level.getEntityOrPart(id)`; the player itself is taken out of the level
        // Not implemented: `getEntityOrPart` (the parts of the `EnderDragon` cannot be picked: they are not entities of the level).
        let entity: &dyn Entity = if packet.id == ctx.player.id() {
            &*ctx.player
        } else if let Some(entity) = ctx.level.get_entity(packet.id) {
            entity
        } else {
            return;
        };
        if !ctx
            .player
            .is_within_entity_interaction_range(&entity.base().bounding_box(), 3.0)
        {
            return;
        }
        let stack = entity.get_pick_result().filter(|s| !s.is_empty());
        let avatar = (packet.include_data && ctx.player.can_use_game_master_blocks())
            .then(|| avatar_profile(entity))
            .flatten()
            .map(|profile| (profile, entity_display_name(ctx.server, entity)));
        if let Some(stack) = stack {
            try_pick(ctx.player, ctx.level, stack);
        }
        if let Some((profile, display_name)) = avatar
            && let Some(source) = ctx.create_command_source_stack()
        {
            print_for_avatar(&source, display_name, &profile);
        }
    }
}

/// `ServerGamePacketListenerImpl.handlePickItemFromBlock`.
pub fn pick_item_from_block(
    player: &mut ServerPlayer,
    level: &mut ServerLevel,
    packet: &ServerboundPickItemFromBlockPacket,
) {
    let pos = packet.pos;
    if !player.is_within_block_interaction_range(pos, 1.0) || !level.is_loaded(pos) {
        return;
    }
    let state = level.get_block_state(pos);
    let include_data = player.player.abilities.instabuild && packet.include_data;
    let mut stack =
        block_behaviour(state.block()).get_clone_item_stack(state, level, pos, include_data);
    if stack.is_empty() {
        return;
    }
    if include_data && state.has_block_entity() {
        // `addBlockDataToItem`: the custom data as `block_entity_data` (what the components do not carry) and the
        // collected components
        if let Some(be) = level.get_block_entity_immediate(pos) {
            let mut tag = mc_nbt::CompoundTag::new();
            be.save_additional(&mut tag);
            be.remove_components_from_tag(&mut tag);
            if tag.is_empty() {
                stack.remove(DataComponentType::BLOCK_ENTITY_DATA);
            } else {
                stack.set(BLOCK_ENTITY_DATA, TypedEntityData::of(be.get_type(), tag));
            }
            for (ty, value) in be.collect_components().iter() {
                stack.set_value(ty, Some(value.clone()));
            }
        }
    }
    try_pick(player, level, stack);
}

/// `ServerGamePacketListenerImpl.tryPickItem(stack)`.
fn try_pick(player: &mut ServerPlayer, level: &mut ServerLevel, stack: ItemStack) {
    if !is_item_enabled(&stack, level.enabled_features()) {
        return;
    }
    let mut pctx = PlayerCtx { player, level };
    let slot = try_pick_item(&mut pctx, stack);
    pctx.player.send(&ClientboundSetHeldSlotPacket { slot });
    let mut menus = std::mem::take(&mut pctx.player.player.menus);
    menus.inventory_menu_mut().broadcast_changes(&mut pctx);
    pctx.player.player.menus = menus;
}

#[distributed_slice(PLAY_HANDLERS)]
static MENU_HANDLERS: &dyn PlayHandlers = &MenuHandlers;
