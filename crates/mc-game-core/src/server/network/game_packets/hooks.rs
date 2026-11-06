//! Extension points of the play-state dispatcher for subsystems that are not ported yet.
//!
//! Vanilla's listener methods for these packets are thin: a `resetLastActionTime()`/permission check followed by a call
//! into the subsystem (menus, recipe book, advancements, block entities, entities, inventory). The listener-level part
//! is implemented in the dispatcher ([`PlayCtx::dispatch_hook`]); the subsystem part is a method of [`PlayHandlers`].
//! A subsystem registers one implementor in [`PLAY_HANDLERS`] (a `linkme` slice); all registered implementors are
//! called for every packet and each overrides only the methods of the packets it owns, so the defaults are no-ops
//! (what vanilla does when the player has no matching open menu / block entity).
//!
//! Every method receives the [`PlayCtx`], i.e. the server, the player's level and the player (taken out of the
//! level), exactly what vanilla reaches through `this.player` / `this.server`.

use super::PlayCtx;
use linkme::distributed_slice;
use mc_protocol::packets::play::*;

#[distributed_slice]
pub static PLAY_HANDLERS: [&'static dyn PlayHandlers] = [..];

/// Player-side systems the listener calls into but that belong to the entity/item/menu slices (vanilla calls them as
/// `this.player.xxx(...)`). One implementor registers in [`PLAYER_SYSTEMS`]; the defaults are what the listener
/// observes when the system is absent (no effect / `false` / `Pass`). Methods that need the player's level get it
/// through the [`PlayCtx`].
#[distributed_slice]
pub static PLAYER_SYSTEMS: [&'static dyn PlayerSystems] = [..];

#[expect(
    unused_variables,
    reason = "default no-op bodies; the parameters document the vanilla call shape"
)]
pub trait PlayerSystems: Sync {
    /// `ServerPlayer.jumpFromGround` (jump power, sprint boost, `Stats.JUMP`, food exhaustion).
    fn jump_from_ground(&self, ctx: &mut PlayCtx) {}
    /// `LivingEntity.onClimbable()` (the player itself).
    fn on_climbable(&self, ctx: &mut PlayCtx) -> bool {
        false
    }
    /// `Entity.onClimbable()` for a controlled vehicle that is a `LivingEntity` (then `resetFallDistance`).
    fn vehicle_on_climbable(
        &self,
        ctx: &mut PlayCtx,
        vehicle: &dyn crate::world::entity::Entity,
    ) -> bool {
        false
    }
    /// `ServerPlayer.checkMovementStatistics(dx, dy, dz)` (walk/swim/climb/fly distance stats + exhaustion).
    fn check_movement_statistics(&self, ctx: &mut PlayCtx, dx: f64, dy: f64, dz: f64) {}
    /// `PlayerActionPacket` item actions: `STAB` (`PiercingWeapon.attack`), `SWAP_ITEM_WITH_OFFHAND`, `DROP_ITEM`/`DROP_ALL_ITEMS`.
    fn stab_with_piercing_weapon(&self, ctx: &mut PlayCtx) {}
    /// `Player.cannotAttackWithItem(stack, tolerance)` for the main hand.
    fn cannot_attack_with_main_hand(&self, ctx: &mut PlayCtx, tolerance_ticks: i32) -> bool {
        false
    }
    fn reset_attack_strength_ticker(&self, ctx: &mut PlayCtx) {}
    /// `Player.isWithinAttackRange(mainHandItem, targetBounds, buffer)`.
    fn is_within_attack_range(
        &self,
        ctx: &mut PlayCtx,
        target: &mc_core::phys::AABB,
        buffer: f64,
    ) -> bool {
        false
    }
    fn main_hand_has_piercing_weapon(&self, ctx: &mut PlayCtx) -> bool {
        false
    }
    /// `ServerPlayer.attack(target)`.
    fn attack(&self, ctx: &mut PlayCtx, target: crate::world::entity::EntityId) {}
    /// `ServerPlayer.interactOn(target, hand, location)`.
    fn interact_on(
        &self,
        ctx: &mut PlayCtx,
        target: crate::world::entity::EntityId,
        hand: InteractionHand,
        location: mc_core::phys::Vec3,
    ) -> InteractionResult {
        InteractionResult::PASS
    }
    /// `CriteriaTriggers.ANY_BLOCK_USE` / `PLAYER_INTERACTED_WITH_ENTITY` triggers.
    fn trigger_any_block_use(
        &self,
        ctx: &mut PlayCtx,
        pos: mc_core::pos::BlockPos,
        hand: InteractionHand,
    ) {
    }
    fn trigger_player_interacted_with_entity(
        &self,
        ctx: &mut PlayCtx,
        awarded_for_stack: crate::world::item::ItemStack,
        target: crate::world::entity::EntityId,
    ) {
    }
    /// `ServerPlayer.sendBuildLimitMessage(upper, limit)` / `sendSpawnProtectionMessage(pos)`.
    fn send_build_limit_message(&self, ctx: &mut PlayCtx, upper: bool, limit: i32) {}
    fn send_spawn_protection_message(&self, ctx: &mut PlayCtx, pos: mc_core::pos::BlockPos) {}
    /// `ServerPlayer.setCamera(target)`.
    fn set_camera(&self, ctx: &mut PlayCtx, target: crate::world::entity::EntityId) {}
    /// `getStats().sendStats(player)`.
    fn send_stats(&self, ctx: &mut PlayCtx) {}
    /// Riding: `PlayerRideableJumping` start/stop jump, `HasCustomInventoryScreen`, boat paddle state.
    fn start_riding_jump(&self, ctx: &mut PlayCtx, data: i32) {}
    fn stop_riding_jump(&self, ctx: &mut PlayCtx) {}
    fn open_vehicle_inventory(&self, ctx: &mut PlayCtx) {}
    fn set_controlled_boat_paddle_state(&self, ctx: &mut PlayCtx, left: bool, right: bool) {}
    /// `tryToStartFallFlying` / `stopFallFlying` / `stopSleepInBed(false, true)`.
    fn try_to_start_fall_flying(&self, ctx: &mut PlayCtx) -> bool {
        false
    }
    fn stop_fall_flying(&self, ctx: &mut PlayCtx) {}
    fn stop_sleep_in_bed(
        &self,
        ctx: &mut PlayCtx,
        was_slept_long_enough: bool,
        update_level_list: bool,
    ) {
    }
    /// `ServerPlayer.teleportTo(level, x, y, z, relatives, yRot, xRot, setCamera)` across levels.
    fn teleport_to(
        &self,
        ctx: &mut PlayCtx,
        dimension: &crate::world::level::DimensionKey,
        x: f64,
        y: f64,
        z: f64,
        y_rot: f32,
        x_rot: f32,
        set_camera: bool,
    ) {
    }
    /// `ServerPlayer.createCommandSourceStack()` permission level / `permissions().hasPermission(COMMANDS_GAMEMASTER)`.
    fn has_gamemaster_permission(&self, ctx: &mut PlayCtx) -> Option<bool> {
        None
    }
}

/// Item/hand related player systems the listener calls into (implemented by the item code, `world::item::player_systems`).
#[distributed_slice]
pub static ITEM_SYSTEMS: [&'static dyn ItemSystems] = [..];

#[expect(
    unused_variables,
    reason = "default no-op bodies; the parameters document the vanilla call shape"
)]
pub trait ItemSystems: Sync {
    /// `LivingEntity.stopUsingItem` / `releaseUsingItem` / `getUsedItemHand`.
    fn stop_using_item(&self, ctx: &mut PlayCtx) {}
    fn release_using_item(&self, ctx: &mut PlayCtx) {}
    fn get_used_item_hand(&self, ctx: &mut PlayCtx) -> Option<InteractionHand> {
        None
    }
    fn swap_item_with_offhand(&self, ctx: &mut PlayCtx) {}
    fn drop_selected_item(&self, ctx: &mut PlayCtx, all: bool) {}
    /// `itemStack.isItemEnabled(level.enabledFeatures())` for the item in `hand`.
    fn item_in_hand_enabled(&self, ctx: &mut PlayCtx, hand: InteractionHand) -> bool {
        true
    }
    fn item_in_hand_is_empty(&self, ctx: &mut PlayCtx, hand: InteractionHand) -> bool {
        false
    }
    /// `wasBlockPlacementAttempt(player, mainHandItem)`.
    fn was_block_placement_attempt(&self, ctx: &mut PlayCtx, hand: InteractionHand) -> bool {
        false
    }
    /// `itemStack.getInteractAnimation()` / `getAttackAnimation()` (a `SwingAnimation`, opaque to the listener).
    fn swing_animation(
        &self,
        ctx: &mut PlayCtx,
        hand: InteractionHand,
        attack: bool,
    ) -> SwingAnimationRef {
        SwingAnimationRef::default()
    }
    fn swing(
        &self,
        ctx: &mut PlayCtx,
        hand: InteractionHand,
        animation: SwingAnimationRef,
        send_to_swinging_entity: bool,
    ) {
    }
    fn swing_and_reset_attack_strength(
        &self,
        ctx: &mut PlayCtx,
        hand: InteractionHand,
        animation: SwingAnimationRef,
        send_to_swinging_entity: bool,
    ) {
    }
    /// `ServerPlayerGameMode.useItemOn(player, level, stack, hand, hit)` / `useItem(player, level, stack, hand)`.
    fn use_item_on(
        &self,
        ctx: &mut PlayCtx,
        hand: InteractionHand,
        hit: &ServerboundBlockHitResult,
    ) -> InteractionResult {
        InteractionResult::PASS
    }
    fn use_item(&self, ctx: &mut PlayCtx, hand: InteractionHand) -> InteractionResult {
        InteractionResult::PASS
    }
}

struct NoItemSystems;
impl ItemSystems for NoItemSystems {}

/// Opaque `SwingAnimation` (item component) carried from the listener to `swing`.
#[derive(Clone, Copy, Default, Debug)]
pub struct SwingAnimationRef(pub u32);

pub use crate::world::item::{InteractionHand, InteractionResult};
pub use mc_protocol::packets::play::BlockHitResult as ServerboundBlockHitResult;

#[expect(
    unused_variables,
    reason = "default no-op bodies; the parameters document the vanilla call shape"
)]
pub trait PlayHandlers: Sync {
    /// `handleContainerClose` -> `player.doCloseContainer()`.
    fn handle_container_close(&self, ctx: &mut PlayCtx, packet: &ServerboundContainerClosePacket) {}
    /// `handleContainerClick` (after `resetLastActionTime`): spectator/dead resync, `stillValid`, slot index check,
    /// `containerMenu.clicked`, remote slot bookkeeping, `broadcastChanges`/`broadcastFullState`.
    fn handle_container_click(&self, ctx: &mut PlayCtx, packet: &ServerboundContainerClickPacket) {}
    /// `handleContainerButtonClick` (after `resetLastActionTime`).
    fn handle_container_button_click(
        &self,
        ctx: &mut PlayCtx,
        packet: &ServerboundContainerButtonClickPacket,
    ) {
    }
    /// `handleContainerSlotStateChanged` (crafter menu).
    fn handle_container_slot_state_changed(
        &self,
        ctx: &mut PlayCtx,
        packet: &ServerboundContainerSlotStateChangedPacket,
    ) {
    }
    /// `handlePlaceRecipe` (after `resetLastActionTime`).
    fn handle_place_recipe(&self, ctx: &mut PlayCtx, packet: &ServerboundPlaceRecipePacket) {}
    /// `handleRecipeBookSeenRecipePacket`.
    fn handle_recipe_book_seen_recipe(
        &self,
        ctx: &mut PlayCtx,
        packet: &ServerboundRecipeBookSeenRecipePacket,
    ) {
    }
    /// `handleRecipeBookChangeSettingsPacket`.
    fn handle_recipe_book_change_settings(
        &self,
        ctx: &mut PlayCtx,
        packet: &ServerboundRecipeBookChangeSettingsPacket,
    ) {
    }
    /// `handleSeenAdvancements`.
    fn handle_seen_advancements(
        &self,
        ctx: &mut PlayCtx,
        packet: &ServerboundSeenAdvancementsPacket,
    ) {
    }
    /// `handleBundleItemSelectedPacket`.
    fn handle_bundle_item_selected(
        &self,
        ctx: &mut PlayCtx,
        packet: &ServerboundSelectBundleItemPacket,
    ) {
    }
    /// `handleRenameItem` (anvil menu).
    fn handle_rename_item(&self, ctx: &mut PlayCtx, packet: &ServerboundRenameItemPacket) {}
    /// `handleSetBeaconPacket`: on an invalid selection the implementor disconnects with
    /// `multiplayer.disconnect.generic` and logs `Player {} tried to set invalid beacon effects`.
    fn handle_set_beacon(&self, ctx: &mut PlayCtx, packet: &ServerboundSetBeaconPacket) {}
    /// `handleSelectTrade` (merchant menu).
    fn handle_select_trade(&self, ctx: &mut PlayCtx, packet: &ServerboundSelectTradePacket) {}
    /// `handleEditBook` (text is passed through the pass-through text filter first).
    fn handle_edit_book(&self, ctx: &mut PlayCtx, packet: &ServerboundEditBookPacket) {}
    /// `updateSignText` (after `resetLastActionTime`, with `ChatFormatting.stripFormatting` applied to the lines).
    fn update_sign_text(
        &self,
        ctx: &mut PlayCtx,
        packet: &ServerboundSignUpdatePacket,
        lines: &[String],
    ) {
    }
    /// `handleSetCommandBlock` (`player.canUseGameMasterBlocks()` is checked by the implementor, it owns the message).
    fn handle_set_command_block(
        &self,
        ctx: &mut PlayCtx,
        packet: &ServerboundSetCommandBlockPacket,
    ) {
    }
    /// `handleSetCommandMinecart`.
    fn handle_set_command_minecart(
        &self,
        ctx: &mut PlayCtx,
        packet: &ServerboundSetCommandMinecartPacket,
    ) {
    }
    /// `handleSetStructureBlock`.
    fn handle_set_structure_block(
        &self,
        ctx: &mut PlayCtx,
        packet: &ServerboundSetStructureBlockPacket,
    ) {
    }
    /// `handleSetTestBlock`.
    fn handle_set_test_block(&self, ctx: &mut PlayCtx, packet: &ServerboundSetTestBlockPacket) {}
    /// `handleTestInstanceBlockAction`.
    fn handle_test_instance_block_action(
        &self,
        ctx: &mut PlayCtx,
        packet: &ServerboundTestInstanceBlockActionPacket,
    ) {
    }
    /// `handleSetJigsawBlock`.
    fn handle_set_jigsaw_block(&self, ctx: &mut PlayCtx, packet: &ServerboundSetJigsawBlockPacket) {
    }
    /// `handleJigsawGenerate`.
    fn handle_jigsaw_generate(&self, ctx: &mut PlayCtx, packet: &ServerboundJigsawGeneratePacket) {}
    /// `handlePickItemFromBlock`.
    fn handle_pick_item_from_block(
        &self,
        ctx: &mut PlayCtx,
        packet: &ServerboundPickItemFromBlockPacket,
    ) {
    }
    /// `handlePickItemFromEntity`.
    fn handle_pick_item_from_entity(
        &self,
        ctx: &mut PlayCtx,
        packet: &ServerboundPickItemFromEntityPacket,
    ) {
    }
    /// `handleEntityTagQuery` (permission already checked by the dispatcher).
    fn handle_entity_tag_query(&self, ctx: &mut PlayCtx, packet: &ServerboundEntityTagQueryPacket) {
    }
    /// `handleBlockEntityTagQuery` (permission already checked by the dispatcher).
    fn handle_block_entity_tag_query(
        &self,
        ctx: &mut PlayCtx,
        packet: &ServerboundBlockEntityTagQueryPacket,
    ) {
    }
    /// `handleSetCreativeModeSlot`, slot assignment (`validSlot && validData`): `inventoryMenu.getSlot(n).setByPlayer`,
    /// `setRemoteSlot`, `broadcastChanges`.
    fn handle_set_creative_mode_slot(
        &self,
        ctx: &mut PlayCtx,
        packet: &ServerboundSetCreativeModeSlotPacket,
    ) {
    }
    /// `handleSetCreativeModeSlot`, drop (`drop && validData`, already rate limited): `player.drop(stack, true, PREDICTED)`.
    fn handle_creative_drop(
        &self,
        ctx: &mut PlayCtx,
        packet: &ServerboundSetCreativeModeSlotPacket,
    ) {
    }
    /// `handleDebugSubscriptionRequest` -> `player.requestDebugSubscriptions`.
    fn handle_debug_subscription_request(
        &self,
        ctx: &mut PlayCtx,
        packet: &ServerboundDebugSubscriptionRequestPacket,
    ) {
    }
    /// `handleSetGameRule` (permission checked by the dispatcher): apply `entries` to the level's `GameRules`.
    fn handle_set_game_rule(&self, ctx: &mut PlayCtx, packet: &ServerboundSetGameRulePacket) {}
}

impl PlayCtx<'_> {
    /// The registered [`PlayerSystems`] (the first; slices register exactly one).
    pub(super) fn sys(&self) -> &'static dyn PlayerSystems {
        PLAYER_SYSTEMS.first().copied().unwrap_or(&NoPlayerSystems)
    }

    /// The registered [`ItemSystems`].
    pub(super) fn item_sys(&self) -> &'static dyn ItemSystems {
        ITEM_SYSTEMS.first().copied().unwrap_or(&NoItemSystems)
    }

    fn hooks(&mut self, f: impl Fn(&dyn PlayHandlers, &mut PlayCtx)) {
        for handler in PLAY_HANDLERS {
            f(*handler, self);
        }
    }

    /// The remaining serverbound packets: listener-level checks (reset action time, permissions, rate limits), then
    /// the [`PlayHandlers`] hooks. Packets handled elsewhere never reach this function.
    pub(super) fn dispatch_hook(&mut self, packet: ServerboundPlayPacket) {
        use ServerboundPlayPacket as P;
        match packet {
            P::ContainerClose(p) => self.hooks(|h, c| h.handle_container_close(c, &p)),
            P::ContainerClick(p) => {
                self.player.reset_last_action_time();
                self.hooks(|h, c| h.handle_container_click(c, &p));
            }
            P::ContainerButtonClick(p) => {
                self.player.reset_last_action_time();
                self.hooks(|h, c| h.handle_container_button_click(c, &p));
            }
            P::ContainerSlotStateChanged(p) => {
                self.hooks(|h, c| h.handle_container_slot_state_changed(c, &p))
            }
            P::PlaceRecipe(p) => {
                self.player.reset_last_action_time();
                self.hooks(|h, c| h.handle_place_recipe(c, &p));
            }
            P::RecipeBookSeenRecipe(p) => {
                self.hooks(|h, c| h.handle_recipe_book_seen_recipe(c, &p))
            }
            P::RecipeBookChangeSettings(p) => {
                self.hooks(|h, c| h.handle_recipe_book_change_settings(c, &p))
            }
            P::SeenAdvancements(p) => self.hooks(|h, c| h.handle_seen_advancements(c, &p)),
            P::SelectBundleItem(p) => self.hooks(|h, c| h.handle_bundle_item_selected(c, &p)),
            P::RenameItem(p) => self.hooks(|h, c| h.handle_rename_item(c, &p)),
            P::SetBeacon(p) => self.hooks(|h, c| h.handle_set_beacon(c, &p)),
            P::SelectTrade(p) => self.hooks(|h, c| h.handle_select_trade(c, &p)),
            P::EditBook(p) => {
                // `Inventory.isHotbarSlot(slot) || slot == 40`
                if (0..9).contains(&p.slot) || p.slot == 40 {
                    self.hooks(|h, c| h.handle_edit_book(c, &p));
                }
            }
            P::SignUpdate(p) => {
                let lines: Vec<String> = p
                    .lines
                    .iter()
                    .map(|l| mc_text::ChatFormatting::strip_formatting(l))
                    .collect();
                self.player.reset_last_action_time();
                self.hooks(|h, c| h.update_sign_text(c, &p, &lines));
            }
            P::SetCommandBlock(p) => self.hooks(|h, c| h.handle_set_command_block(c, &p)),
            P::SetCommandMinecart(p) => self.hooks(|h, c| h.handle_set_command_minecart(c, &p)),
            P::SetStructureBlock(p) => self.hooks(|h, c| h.handle_set_structure_block(c, &p)),
            P::SetTestBlock(p) => self.hooks(|h, c| h.handle_set_test_block(c, &p)),
            P::TestInstanceBlockAction(p) => {
                self.hooks(|h, c| h.handle_test_instance_block_action(c, &p))
            }
            P::SetJigsawBlock(p) => self.hooks(|h, c| h.handle_set_jigsaw_block(c, &p)),
            P::JigsawGenerate(p) => self.hooks(|h, c| h.handle_jigsaw_generate(c, &p)),
            P::PickItemFromBlock(p) => self.hooks(|h, c| h.handle_pick_item_from_block(c, &p)),
            P::PickItemFromEntity(p) => self.hooks(|h, c| h.handle_pick_item_from_entity(c, &p)),
            P::EntityTagQuery(p) => {
                if self.has_gamemaster_permission() {
                    self.hooks(|h, c| h.handle_entity_tag_query(c, &p));
                }
            }
            P::BlockEntityTagQuery(p) => {
                if self.has_gamemaster_permission() {
                    self.hooks(|h, c| h.handle_block_entity_tag_query(c, &p));
                }
            }
            P::SetCreativeModeSlot(p) => self.handle_set_creative_mode_slot(&p),
            P::DebugSubscriptionRequest(p) => {
                self.hooks(|h, c| h.handle_debug_subscription_request(c, &p))
            }
            P::SetGameRule(p) => {
                if !self.has_gamemaster_permission() {
                    log::warn!(
                        "Player {} tried to set game rule values without required permissions",
                        self.player.get_profile().name
                    );
                } else {
                    self.hooks(|h, c| h.handle_set_game_rule(c, &p));
                }
            }
            other => unreachable!("packet {} is handled by the dispatcher", other.name()),
        }
    }

    /// `handleSetCreativeModeSlot` (listener part: infinite-materials check, slot/data validity, drop rate limit).
    fn handle_set_creative_mode_slot(&mut self, packet: &ServerboundSetCreativeModeSlotPacket) {
        if !self.has_infinite_materials() {
            return;
        }
        let drop = packet.slot_num < 0;
        let item_stack = &packet.item_stack;
        if !crate::server::network::game_packets::seam::item_enabled(self.level, item_stack) {
            return;
        }
        let valid_slot = (1..=45).contains(&packet.slot_num);
        let valid_data =
            item_stack.is_empty() || item_stack.count() <= item_stack.get_max_stack_size();
        if valid_slot && valid_data {
            self.hooks(|h, c| h.handle_set_creative_mode_slot(c, packet));
        } else if drop && valid_data {
            if self.lis().drop_spam_throttler.is_under_threshold() {
                self.lis().drop_spam_throttler.increment();
                self.hooks(|h, c| h.handle_creative_drop(c, packet));
            } else {
                log::warn!(
                    "Player {} was dropping items too fast in creative mode, ignoring.",
                    self.player.get_plain_text_name()
                );
            }
        }
    }
}

struct NoPlayerSystems;
impl PlayerSystems for NoPlayerSystems {}
