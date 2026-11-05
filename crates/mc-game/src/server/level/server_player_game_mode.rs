//! `ServerPlayerGameMode`: the digging state machine and `Player.getDestroySpeed`. The state lives in
//! `ServerPlayer.game_mode`; the logic is `impl ServerPlayer` because it needs the player and the level at once.
//!
//! Block hooks (`playerWillDestroy`/`destroy`/`playerDestroy`/`attack`/`useItemOn`/`useWithoutItem`) are the
//! `BlockBehaviour` trait methods; item hooks are `ItemBehaviour::{use_on, use_item}`. Loot drops are a later wave.

use crate::world::entity::player::server_player::ServerPlayer;
use crate::world::entity::{Entity, EntityId};
use crate::world::item::behaviour;
use crate::server::level::ServerLevel;
use crate::world::item::{InteractionHand, InteractionResult, ItemStack, ItemStackExt, UseOnContext, ContextPlayer};
use crate::world::level::block::behaviour as block_behaviour;
use crate::world::level::{BlockGetter, LevelAccessor};
use crate::world::entity::equipment::EquipmentSlot;
use mc_core::direction::Direction;
use mc_core::pos::BlockPos;
use mc_data::block::BlockState;
use mc_data::enums::GameType;
use mc_data::{Attribute, MobEffect};
use mc_protocol::packets::play::{ClientboundBlockUpdatePacket, ClientboundGameEventPacket, ClientboundPlayerAbilitiesPacket, ServerboundPlayerActionAction as Action};

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

    /// `ServerPlayerGameMode.handleBlockBreakAction(pos, action, direction, maxY, sequence)`.
    pub fn handle_block_break_action(&mut self, level: &mut ServerLevel, pos: BlockPos, action: Action, direction: Direction, max_y: i32, _sequence: i32) {
        let id = self.base_id();
        if !self.is_within_block_interaction_range(pos, 1.0) {
            if self.game_mode.is_destroying_block && action == Action::AbortDestroyBlock {
                self.abort_destroy_block(level, pos);
            }
            return;
        }
        if pos.y() > max_y {
            self.send(&ClientboundBlockUpdatePacket { pos, block_state: level.get_block_state(pos) });
            return;
        }
        match action {
            Action::StartDestroyBlock => {
                if level.is_under_spawn_protection(self, pos) {
                    self.pending_spawn_protection_message = Some(pos);
                    return;
                }
                if !level.may_interact(id, pos) {
                    self.send(&ClientboundBlockUpdatePacket { pos, block_state: level.get_block_state(pos) });
                    return;
                }
                if self.player.abilities.instabuild {
                    self.destroy_and_ack(level, pos);
                    return;
                }
                if self.block_action_restricted(level, pos) {
                    self.send(&ClientboundBlockUpdatePacket { pos, block_state: level.get_block_state(pos) });
                    return;
                }
                self.game_mode.destroy_progress_start = self.game_mode.game_ticks;
                let mut progress = 1.0f32;
                let state = level.get_block_state(pos);
                if !state.is_air() {
                    block_behaviour(state.block()).attack(state, level, pos, self);
                    progress = self.get_destroy_progress(state);
                }
                if !state.is_air() && progress >= 1.0 {
                    self.destroy_and_ack(level, pos);
                } else {
                    if self.game_mode.is_destroying_block {
                        let dp = self.game_mode.destroy_pos;
                        self.send(&ClientboundBlockUpdatePacket { pos: dp, block_state: level.get_block_state(dp) });
                    }
                    self.game_mode.is_destroying_block = true;
                    self.game_mode.destroy_pos = pos;
                    self.game_mode.destroy_direction = direction;
                    let state = (progress * 10.0) as i32;
                    level.destroy_block_progress(id, pos, state);
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
                            level.destroy_block_progress(id, pos, -1);
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
            self.send(&ClientboundBlockUpdatePacket { pos, block_state: level.get_block_state(pos) });
        }
    }

    /// `Player.blockActionRestricted(level, pos, gameType)` (adventure/spectator rules; the adventure `CAN_BREAK`
    /// component check is not modelled: adventure players are always restricted when `mayBuild` is false).
    pub fn block_action_restricted(&self, _level: &ServerLevel, _pos: BlockPos) -> bool {
        match self.game_mode.game_mode_for_player {
            GameType::Spectator => true,
            GameType::Adventure => !self.player.abilities.may_build,
            _ => false,
        }
    }

    /// `ServerPlayerGameMode.destroyBlock(pos)` (drops are a later wave: `player_destroy` only exhausts).
    pub fn destroy_block(&mut self, level: &mut ServerLevel, pos: BlockPos) -> bool {
        let state = level.get_block_state(pos);
        let creative = self.player.abilities.instabuild;
        let main = self.player.inventory.get_selected_item().clone();
        if !main.is_empty() && !crate::world::item::behaviour(main.item()).can_destroy_block(&main, state, creative) {
            return false;
        }
        let block = state.block();
        if block.data().classes.iter().any(|c| *c == "GameMasterBlock") && !self.can_use_game_master_blocks() {
            let st = level.get_block_state(pos);
            level.send_block_updated(pos, st, st, 3);
            return false;
        }
        if self.block_action_restricted(level, pos) {
            return false;
        }
        // `Block.playerWillDestroy`: particles (level event 2001) + game event
        level.level_event(Some(self.base_id()), 2001, pos, state.id() as i32);
        let adjusted = block_behaviour(block).player_will_destroy(state, level, pos, self);
        let changed = level.remove_block(pos, false);
        if changed {
            block_behaviour(block).destroy(level, pos, adjusted);
        }
        if self.player.abilities.instabuild {
            return true;
        }
        let destroyed_with = main.copy();
        let can_destroy = self.has_correct_tool_for_drops(adjusted);
        // `destroyedWith.mineBlock(level, adjustedState, pos, player)`: tool durability (on the main hand stack)
        let mut mined_with = self.get_item_in_hand(InteractionHand::MainHand);
        if !mined_with.is_empty() {
            crate::world::item::mine_block(&mut mined_with, level, adjusted, pos, self);
            self.set_item_in_hand(InteractionHand::MainHand, mined_with);
        }
        if changed && can_destroy {
            block_behaviour(block).player_destroy(level, self, pos, adjusted, &destroyed_with);
        }
        true
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
        ContextPlayer { entity_id: self.base_id(), y_rot: self.base().y_rot(), x_rot: self.base().x_rot(), secondary_use_active: self.is_shift_key_down() }
    }

    /// `ServerPlayerGameMode.useItem(player, level, stack, hand)`.
    pub fn use_item(&mut self, level: &mut ServerLevel, hand: InteractionHand) -> InteractionResult {
        if self.game_mode.game_mode_for_player == GameType::Spectator {
            return InteractionResult::PASS;
        }
        let stack = self.get_item_in_hand(hand);
        if stack.is_empty() {
            return InteractionResult::PASS;
        }
        let (old_count, old_damage) = (stack.count(), stack.get_damage_value());
        let result = crate::world::item::item_stack_use(level, self, hand);
        let result_stack = match &result {
            InteractionResult::Success(s) => s.item_context.held_item_transformed_to.clone().unwrap_or_else(|| self.get_item_in_hand(hand)),
            _ => self.get_item_in_hand(hand),
        };
        if result_stack == stack && result_stack.count() == old_count && result_stack.get_damage_value() == old_damage {
            return result;
        }
        self.set_item_in_hand(hand, if result_stack.is_empty() { ItemStack::empty() } else { result_stack });
        result
    }

    /// `ServerPlayerGameMode.useItemOn(player, level, stack, hand, hitResult)`.
    pub fn use_item_on(&mut self, level: &mut ServerLevel, hand: InteractionHand, hit: &mc_core::phys::BlockHitResult) -> InteractionResult {
        let pos = hit.block_pos;
        let state = level.get_block_state(pos);
        if self.game_mode.game_mode_for_player == GameType::Spectator {
            // menu providers / portals are not modelled
            return InteractionResult::PASS;
        }
        let item_stack = self.get_item_in_hand(hand);
        let have_something = !self.player.inventory.get_selected_item().is_empty() || !self.player.inventory.equipment_get(EquipmentSlot::OffHand).is_empty();
        let suppress_using_block = self.is_shift_key_down() && have_something;
        let behaviour = block_behaviour(state.block());
        if !suppress_using_block {
            let item_use = behaviour.use_item_on(&item_stack, state, level, pos, self, hand, hit);
            if item_use.consumes_action() {
                return item_use;
            }
            if item_use == InteractionResult::TRY_WITH_EMPTY_HAND && hand == InteractionHand::MainHand {
                let r = behaviour.use_without_item(state, level, pos, self, hit);
                if r.consumes_action() {
                    return r;
                }
            }
        }
        if !item_stack.is_empty() {
            let mut context = UseOnContext::new(Some(self.context_player()), hand, item_stack.clone(), *hit);
            let infinite = self.player.abilities.instabuild;
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
            return result;
        }
        InteractionResult::PASS
    }

    /// `ServerPlayerGameMode.tick()` minus the `gameTicks++` (done in `ServerPlayer::tick_server_player`): digging progress broadcast.
    pub fn tick_game_mode(&mut self, level: &mut ServerLevel) {
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
