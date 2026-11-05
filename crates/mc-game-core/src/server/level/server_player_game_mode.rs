//! `ServerPlayerGameMode`: the digging state machine and `Player.getDestroySpeed`. The state lives in
//! `ServerPlayer.game_mode`; the logic is `impl ServerPlayer` because it needs the player and the level at once.
//!
//! Block hooks (`playerWillDestroy`/`destroy`/`playerDestroy`/`attack`/`useItemOn`/`useWithoutItem`) are the
//! `BlockBehaviour` trait methods (`block_break.rs` holds their base behaviour); item hooks are `ItemBehaviour::{use_on, use_item}`.

use super::block_break;
use crate::advancements::triggers::CriteriaTriggers;
use crate::world::entity::player::server_player::ServerPlayer;
use crate::world::entity::{Entity, EntityId};
use crate::world::item::behaviour;
use crate::server::level::ServerLevel;
use crate::world::item::{InteractionHand, InteractionResult, ItemStack, ItemStackExt, UseOnContext, ContextPlayer};
use crate::world::level::block::behaviour as block_behaviour;
use crate::world::level::{BlockGetter, LevelAccessor, LevelReader};
use crate::world::entity::equipment::EquipmentSlot;
use mc_core::direction::Direction;
use mc_core::pos::BlockPos;
use mc_data::block::{Block, BlockState};
use mc_data::enums::GameType;
use mc_data::{Attribute, MobEffect};
use mc_protocol::packets::common::ChatVisibility;
use mc_protocol::packets::play::{
    ClientboundBlockUpdatePacket, ClientboundGameEventPacket, ClientboundPlayerAbilitiesPacket, ClientboundSystemChatPacket, ServerboundPlayerActionAction as Action,
};
use mc_text::{ChatFormatting, Component};
use mc_world::level_storage::FeatureFlagSet;

/// `Block.isEnabled(enabledFeatures)`.
fn block_is_enabled(block: Block, enabled_features: FeatureFlagSet) -> bool {
    let required: Vec<mc_core::ident::Identifier> = block.data().required_features.iter().filter_map(|n| mc_core::ident::Identifier::try_parse(n)).collect();
    FeatureFlagSet::from_names(required.iter(), |_| {}).is_subset_of(enabled_features)
}
impl ServerPlayer {
    /// `MobEffectUtil.getDigSpeedAmplification` (haste / conduit power), `None` when neither is active.
    fn dig_speed_amplification(&self) -> Option<i32> {
        let eff = &self.player.living.active_effects;
        let a = eff.get(&MobEffect::HASTE).map(|e| e.amplifier);
        let b = eff.get(&MobEffect::CONDUIT_POWER).map(|e| e.amplifier);
        a.into_iter().chain(b).max()
    }

    /// `Player.getDestroySpeed(state)`.
    pub fn get_destroy_speed(&self, state: BlockState) -> f32 {
        let stack = self.player.inventory.get_selected_item();
        let mut speed = if stack.is_empty() { 1.0 } else { behaviour(stack.item()).get_destroy_speed(stack, state) };
        let living = &self.player.living;
        if speed > 1.0 {
            speed += living.get_attribute_value(Attribute::MINING_EFFICIENCY) as f32;
        }
        if let Some(amp) = self.dig_speed_amplification() {
            speed *= 1.0 + (amp + 1) as f32 * 0.2;
        }
        if let Some(e) = living.active_effects.get(&MobEffect::MINING_FATIGUE) {
            speed *= 0.3f64.powi(e.amplifier + 1) as f32;
        }
        speed *= living.get_attribute_value(Attribute::BLOCK_BREAK_SPEED) as f32;
        if self.base().eye_in_fluid[0] {
            speed *= living.get_attribute_value(Attribute::SUBMERGED_MINING_SPEED) as f32;
        }
        if !self.base().on_ground() {
            speed /= 5.0;
        }
        speed
    }

    /// `Player.hasCorrectToolForDrops(state)`.
    pub fn has_correct_tool_for_drops(&self, state: BlockState) -> bool {
        let stack = self.player.inventory.get_selected_item();
        !state.requires_correct_tool_for_drops() || (!stack.is_empty() && behaviour(stack.item()).is_correct_tool_for_drops(stack, state))
    }

    /// `BlockState.getDestroyProgress(player, level, pos)`.
    pub fn get_destroy_progress(&self, state: BlockState) -> f32 {
        let destroy_speed = state.get_destroy_speed();
        if destroy_speed == -1.0 {
            0.0
        } else {
            let modifier = if self.has_correct_tool_for_drops(state) { 30 } else { 100 };
            self.get_destroy_speed(state) / destroy_speed / modifier as f32
        }
    }

    /// `ServerPlayerGameMode.handleBlockBreakAction(pos, action, direction, maxY, sequence)`. `under_spawn_protection` is
    /// `server.isUnderSpawnProtection(level, pos, player)` (only read for `START_DESTROY_BLOCK`; the sequence ack is the
    /// listener's).
    pub fn handle_block_break_action(
        &mut self,
        level: &mut ServerLevel,
        pos: BlockPos,
        action: Action,
        direction: Direction,
        max_y: i32,
        under_spawn_protection: bool,
    ) {
        if !self.is_within_block_interaction_range(pos, 1.0) {
            if self.game_mode.is_destroying_block && action == Action::AbortDestroyBlock {
                self.abort_destroy_block(level, pos);
            }
            return;
        }
        if pos.y() > max_y {
            self.send_block_update(level, pos);
            return;
        }
        match action {
            Action::StartDestroyBlock => {
                if under_spawn_protection {
                    self.send_spawn_protection_message(pos);
                    return;
                }
                // `level.mayInteract(player, pos)`: spawn protection (checked above) and the world border
                if !level.is_within_world_border(pos) {
                    self.send_block_update(level, pos);
                    return;
                }
                if self.player.abilities.instabuild {
                    self.destroy_and_ack(level, pos);
                    return;
                }
                if self.block_action_restricted(level, pos, self.game_mode.game_mode_for_player) {
                    self.send_block_update(level, pos);
                    return;
                }
                self.game_mode.destroy_progress_start = self.game_mode.game_ticks;
                let mut progress = 1.0f32;
                let state = level.get_block_state(pos);
                if !state.is_air() {
                    self.enchantment_on_hit_block(level, pos, state);
                    block_behaviour(state.block()).attack(state, level, pos, self);
                    progress = self.get_destroy_progress(state);
                }
                if !state.is_air() && progress >= 1.0 {
                    self.destroy_and_ack(level, pos);
                } else {
                    if self.game_mode.is_destroying_block {
                        let destroy_pos = self.game_mode.destroy_pos;
                        self.send_block_update(level, destroy_pos);
                    }
                    self.game_mode.is_destroying_block = true;
                    self.game_mode.destroy_pos = pos;
                    self.game_mode.destroy_direction = direction;
                    let state = (progress * 10.0) as i32;
                    level.destroy_block_progress(self.base_id(), pos, state);
                    self.game_mode.last_sent_state = state;
                }
            }
            Action::ChangeDestroyDirection => self.game_mode.destroy_direction = direction,
            Action::StopDestroyBlock => {
                if pos == self.game_mode.destroy_pos {
                    let ticks_spent = self.game_mode.game_ticks - self.game_mode.destroy_progress_start;
                    let state = level.get_block_state(pos);
                    if !state.is_air() {
                        let progress = self.get_destroy_progress(state) * (ticks_spent + 1) as f32;
                        if progress >= 0.7 {
                            self.game_mode.is_destroying_block = false;
                            level.destroy_block_progress(self.base_id(), pos, -1);
                            self.destroy_and_ack(level, pos);
                            return;
                        }
                        if !self.game_mode.has_delayed_destroy {
                            self.game_mode.is_destroying_block = false;
                            self.game_mode.has_delayed_destroy = true;
                            self.game_mode.delayed_destroy_pos = pos;
                            self.game_mode.delayed_tick_start = self.game_mode.destroy_progress_start;
                        }
                    }
                }
            }
            Action::AbortDestroyBlock => self.abort_destroy_block(level, pos),
            _ => {}
        }
    }

    /// `EnchantmentHelper.onHitBlock(level, mainHandItem, player, player, MAINHAND, Vec3.atCenterOf(pos), state, item -> player.onEquippedItemBroken(item, MAINHAND))`.
    fn enchantment_on_hit_block(&mut self, level: &mut ServerLevel, pos: BlockPos, state: BlockState) {
        use crate::world::entity::equipment::EquipmentSlot as Slot;
        use crate::world::item::enchantment::actors::Actors;
        use std::sync::{Arc, Mutex};
        let mut weapon = self.get_item_in_hand(InteractionHand::MainHand);
        let broken: Arc<Mutex<Vec<ItemStack>>> = Arc::default();
        let sink = broken.clone();
        let id = self.base_id();
        let before = weapon.clone();
        crate::world::item::enchantment_helper::on_hit_block(
            level,
            &mut Actors::of(self),
            &mut weapon,
            Some(id),
            id,
            Some(Slot::MainHand),
            mc_core::phys::Vec3::at_center_of(&pos),
            state,
            Box::new(move |_, item| sink.lock().expect("lock").push(item.clone())),
        );
        if weapon != before {
            self.set_item_in_hand(InteractionHand::MainHand, weapon);
        }
        for item in std::mem::take(&mut *broken.lock().expect("lock")) {
            self.on_equipped_item_broken(level, &item, Slot::MainHand);
        }
    }

    /// `connection.send(new ClientboundBlockUpdatePacket(pos, level.getBlockState(pos)))`.
    fn send_block_update(&self, level: &ServerLevel, pos: BlockPos) {
        self.send(&ClientboundBlockUpdatePacket { pos, block_state: level.get_block_state(pos) });
    }

    fn abort_destroy_block(&mut self, level: &mut ServerLevel, pos: BlockPos) {
        let id = self.base_id();
        self.game_mode.is_destroying_block = false;
        if self.game_mode.destroy_pos != pos {
            log::warn!("Mismatch in destroy block pos: {} {}", self.game_mode.destroy_pos, pos);
            level.destroy_block_progress(id, self.game_mode.destroy_pos, -1);
        }
        level.destroy_block_progress(id, pos, -1);
    }

    /// `destroyAndAck(pos, sequence, exitId)` (the sequence ack packet is sent by the listener).
    pub fn destroy_and_ack(&mut self, level: &mut ServerLevel, pos: BlockPos) {
        if !self.destroy_block(level, pos) {
            self.send_block_update(level, pos);
        }
    }

    /// `Player.blockActionRestricted(level, pos, gameType)`: adventure/spectator rules (`CAN_BREAK` predicate of the
    /// main hand in adventure mode without `mayBuild`).
    pub fn block_action_restricted(&self, level: &ServerLevel, pos: BlockPos, game_type: GameType) -> bool {
        if !matches!(game_type, GameType::Adventure | GameType::Spectator) {
            return false;
        }
        if game_type == GameType::Spectator {
            return true;
        }
        if self.player.abilities.may_build {
            return false;
        }
        let stack = self.player.inventory.get_selected_item();
        stack.is_empty() || !crate::world::item::can_break_block_in_adventure_mode(stack, level, pos)
    }

    /// `ServerPlayerGameMode.destroyBlock(pos)`.
    pub fn destroy_block(&mut self, level: &mut ServerLevel, pos: BlockPos) -> bool {
        let state = level.get_block_state(pos);
        let mut main = self.get_item_in_hand(InteractionHand::MainHand);
        if !main.is_empty() {
            let can_destroy = behaviour(main.item()).can_destroy_block(&mut main, state, level, pos, self);
            if main != self.get_item_in_hand(InteractionHand::MainHand) {
                self.set_item_in_hand(InteractionHand::MainHand, main.clone());
            }
            if !can_destroy {
                return false;
            }
        }
        let block_entity = block_break::snapshot_block_entity(level, pos);
        let block = state.block();
        if block.data().classes.iter().any(|c| *c == "GameMasterBlock") && !self.can_use_game_master_blocks() {
            level.send_block_updated(pos, state, state, 3);
            return false;
        }
        if self.block_action_restricted(level, pos, self.game_mode.game_mode_for_player) {
            return false;
        }
        let adjusted = block_behaviour(block).player_will_destroy(state, level, pos, self);
        let changed = level.remove_block(pos, false);
        if changed {
            block_behaviour(block).destroy(level, pos, adjusted);
        }
        if self.prevents_block_drops() {
            return true;
        }
        let destroyed_with = main.copy();
        let can_destroy = self.has_correct_tool_for_drops(adjusted);
        // `itemStack.mineBlock(level, adjustedState, pos, player)` on the main hand stack itself
        let mut mined_with = self.get_item_in_hand(InteractionHand::MainHand);
        crate::world::item::mine_block(&mut mined_with, level, adjusted, pos, self);
        self.set_item_in_hand(InteractionHand::MainHand, mined_with);
        if changed && can_destroy {
            block_behaviour(block).player_destroy(level, self, pos, adjusted, block_entity.as_deref(), &destroyed_with);
        }
        true
    }

    /// `Player.preventsBlockDrops()`.
    pub fn prevents_block_drops(&self) -> bool {
        self.player.abilities.instabuild
    }

    /// `Player.hasInfiniteMaterials()`.
    pub fn has_infinite_materials(&self) -> bool {
        self.player.abilities.instabuild
    }

    /// `ServerPlayer.sendSystemMessage(message, overlay)`: hidden chat only accepts overlay messages.
    pub fn send_system_message_overlay(&self, message: Component, overlay: bool) {
        if self.chat_visibility != ChatVisibility::Hidden || overlay {
            self.send(&ClientboundSystemChatPacket { content: message, overlay });
        }
    }

    /// `sendOverlayMessage(message)`.
    pub fn send_overlay_message(&self, message: Component) {
        self.send_system_message_overlay(message, true);
    }

    /// `sendBuildLimitMessage(isTooHigh, limit)`.
    pub fn send_build_limit_message(&self, is_too_high: bool, limit: i32) {
        let key = if is_too_high { "build.tooHigh" } else { "build.tooLow" };
        self.send_overlay_message(Component::translatable(key, vec![limit.into()]).with_format(ChatFormatting::Red));
    }

    /// `sendSpawnProtectionMessage(pos)`.
    pub fn send_spawn_protection_message(&self, pos: BlockPos) {
        self.send_overlay_message(Component::translatable("build.spawn_protection", vec![pos.to_short_string().into()]).with_format(ChatFormatting::Red));
    }

    /// `Player.canUseGameMasterBlocks()`: `abilities.instabuild` and op permission (ops are not modelled yet, so
    /// creative players are allowed).
    pub fn can_use_game_master_blocks(&self) -> bool {
        self.player.abilities.instabuild
    }

    /// `Player.getItemInHand(hand)`.
    pub fn get_item_in_hand(&self, hand: InteractionHand) -> ItemStack {
        match hand {
            InteractionHand::MainHand => self.player.inventory.get_selected_item().clone(),
            InteractionHand::OffHand => self.player.inventory.equipment_get(EquipmentSlot::OffHand).clone(),
        }
    }

    /// `Player.setItemInHand(hand, stack)`.
    pub fn set_item_in_hand(&mut self, hand: InteractionHand, stack: ItemStack) {
        match hand {
            InteractionHand::MainHand => {
                self.player.inventory.set_selected_item(stack);
            }
            InteractionHand::OffHand => {
                self.player.inventory.equipment_set(EquipmentSlot::OffHand, stack);
            }
        }
    }

    pub fn context_player(&self) -> ContextPlayer {
        ContextPlayer { entity_id: self.base_id(), y_rot: self.base().y_rot(), x_rot: self.base().x_rot(), secondary_use_active: self.is_shift_key_down(), can_use_game_master_blocks: self.can_use_game_master_blocks() }
    }

    /// `ServerPlayerGameMode.useItem(player, level, stack, hand)`.
    pub fn use_item(&mut self, level: &mut ServerLevel, hand: InteractionHand) -> InteractionResult {
        if self.game_mode.game_mode_for_player == GameType::Spectator {
            return InteractionResult::PASS;
        }
        let stack = self.get_item_in_hand(hand);
        if self.player.cooldowns.is_on_cooldown(&stack) {
            return InteractionResult::PASS;
        }
        let (old_count, old_damage) = (stack.count(), stack.get_damage_value());
        let result = crate::world::item::item_stack_use(level, self, hand);
        // `resultStack == itemStack` (identity) holds unless the use replaced the held stack
        let transformed = match &result {
            InteractionResult::Success(s) => s.item_context.held_item_transformed_to.clone(),
            _ => None,
        };
        let same_stack = transformed.is_none();
        let result_stack = transformed.unwrap_or_else(|| self.get_item_in_hand(hand));
        let use_duration = if result_stack.is_empty() { 0 } else { behaviour(result_stack.item()).get_use_duration(&result_stack) };
        if same_stack && result_stack.count() == old_count && use_duration <= 0 && result_stack.get_damage_value() == old_damage {
            return result;
        }
        if result == InteractionResult::FAIL && use_duration > 0 && !self.is_using_item() {
            return result;
        }
        if !same_stack {
            self.set_item_in_hand(hand, result_stack.clone());
        }
        if result_stack.is_empty() {
            self.set_item_in_hand(hand, ItemStack::empty());
        }
        if !self.is_using_item() {
            self.send_inventory_menu_all_data(level);
        }
        result
    }

    /// `inventoryMenu.sendAllDataToRemote()`.
    pub fn send_inventory_menu_all_data(&mut self, level: &mut ServerLevel) {
        use crate::world::entity::player::menu_ctx::PlayerCtx;
        use crate::world::inventory::PlayerMenus;
        let mut menus = std::mem::replace(&mut self.player.menus, PlayerMenus::new());
        {
            let mut ctx = PlayerCtx { player: self, level };
            menus.inventory_menu_mut().base_mut().send_all_data_to_remote(&mut ctx);
        }
        self.player.menus = menus;
    }

    /// `ServerPlayerGameMode.useItemOn(player, level, stack, hand, hitResult)`.
    pub fn use_item_on(&mut self, level: &mut ServerLevel, hand: InteractionHand, hit: &mc_core::phys::BlockHitResult) -> InteractionResult {
        let pos = hit.block_pos;
        let state = level.get_block_state(pos);
        if !block_is_enabled(state.block(), level.enabled_features()) {
            return InteractionResult::FAIL;
        }
        if self.game_mode.game_mode_for_player == GameType::Spectator {
            // SEAM(portals): the `Portal` spectator teleport (`teleportToPortalDestination`) is not ported.
            let provider = block_behaviour(state.block()).get_menu_provider(state, level, pos);
            return match provider {
                Some(provider) => {
                    self.open_menu(level, provider.as_ref());
                    InteractionResult::CONSUME
                }
                None => InteractionResult::PASS,
            };
        }
        let item_stack = self.get_item_in_hand(hand);
        let have_something = !self.player.inventory.get_selected_item().is_empty() || !self.player.inventory.equipment_get(EquipmentSlot::OffHand).is_empty();
        let suppress_using_block = self.is_shift_key_down() && have_something;
        let behaviour_of_block = block_behaviour(state.block());
        if !suppress_using_block {
            let item_use = behaviour_of_block.use_item_on(&item_stack, state, level, pos, self, hand, hit);
            if item_use.consumes_action() {
                CriteriaTriggers::ITEM_USED_ON_BLOCK.trigger(level, self, pos, &item_stack);
                return item_use;
            }
            if item_use == InteractionResult::TRY_WITH_EMPTY_HAND && hand == InteractionHand::MainHand {
                let r = behaviour_of_block.use_without_item(state, level, pos, self, hit);
                if r.consumes_action() {
                    CriteriaTriggers::DEFAULT_BLOCK_USE.trigger(level, self, pos);
                    return r;
                }
            }
        }
        if !item_stack.is_empty() && !self.player.cooldowns.is_on_cooldown(&item_stack) {
            let mut context = UseOnContext::new(Some(self.context_player()), hand, item_stack.clone(), *hit);
            let infinite = self.has_infinite_materials();
            let count = item_stack.count();
            let result = crate::world::item::item_stack_use_on(&mut context, level, self);
            let mut s = context.item_stack;
            if infinite {
                s.set_count(count);
                self.set_item_in_hand(hand, s);
            } else {
                if let InteractionResult::Success(success) = &result {
                    if let Some(t) = &success.item_context.held_item_transformed_to {
                        s = t.clone();
                    }
                }
                if s != item_stack {
                    self.set_item_in_hand(hand, s);
                }
            }
            if result.consumes_action() {
                CriteriaTriggers::ITEM_USED_ON_BLOCK.trigger(level, self, pos, &item_stack);
            }
            return result;
        }
        InteractionResult::PASS
    }
    /// `ServerPlayerGameMode.tick()`: `gameTicks++`, the delayed-destroy and digging progress.
    pub fn tick_game_mode(&mut self, level: &mut ServerLevel) {
        self.game_mode.game_ticks += 1;
        let id = self.base_id();
        if self.game_mode.has_delayed_destroy {
            let pos = self.game_mode.delayed_destroy_pos;
            let state = level.get_block_state(pos);
            if state.is_air() {
                self.game_mode.has_delayed_destroy = false;
            } else {
                let ticks = self.game_mode.delayed_tick_start;
                let progress = self.increment_destroy_progress(level, state, pos, ticks);
                if progress >= 1.0 {
                    self.game_mode.has_delayed_destroy = false;
                    self.destroy_block(level, pos);
                }
            }
        } else if self.game_mode.is_destroying_block {
            let pos = self.game_mode.destroy_pos;
            let state = level.get_block_state(pos);
            if state.is_air() {
                level.destroy_block_progress(id, pos, -1);
                self.game_mode.last_sent_state = -1;
                self.game_mode.is_destroying_block = false;
            } else {
                let spent = self.game_mode.game_ticks - self.game_mode.destroy_progress_start;
                let event = if spent % 4 == 0 { 2020 } else { 2019 };
                self.increment_destroy_progress(level, state, pos, spent);
                level.level_event(None, event, pos, self.game_mode.destroy_direction as i32);
            }
        }
    }

    fn increment_destroy_progress(&mut self, level: &mut ServerLevel, state: BlockState, pos: BlockPos, ticks_spent: i32) -> f32 {
        let progress = self.get_destroy_progress(state) * (ticks_spent + 1) as f32;
        let s = (progress * 10.0) as i32;
        if s != self.game_mode.last_sent_state {
            level.destroy_block_progress(self.base_id(), pos, s);
            self.game_mode.last_sent_state = s;
        }
        progress
    }
}

fn game_type_id(g: GameType) -> i32 {
    match g {
        GameType::Survival => 0,
        GameType::Creative => 1,
        GameType::Adventure => 2,
        GameType::Spectator => 3,
    }
}
