//! `ServerPlayerGameMode`: the digging state machine and `Player.getDestroySpeed`. The state lives in
//! `ServerPlayer.game_mode`; the logic is `impl ServerPlayer` because it needs the player and the level at once.
//!
//! Level access goes through [`BlockBreakLevel`] (block get/remove, destroy-progress broadcast, `mayInteract`); the
//! `ServerLevel` implementation of it joins when `ServerLevel` implements `LevelAccessor` (block setting).
//! Drops/`playerDestroy`/`playerWillDestroy` block hooks, spawn protection and `Block.attack` are not run yet.

use crate::world::entity::player::server_player::ServerPlayer;
use crate::world::entity::{Entity, EntityId};
use crate::world::item::behaviour;
use crate::world::level::{BlockGetter, LevelAccessor};
use mc_core::direction::Direction;
use mc_core::pos::BlockPos;
use mc_data::block::BlockState;
use mc_data::enums::GameType;
use mc_data::{Attribute, MobEffect};
use mc_protocol::packets::play::{ClientboundBlockUpdatePacket, ClientboundGameEventPacket, ClientboundPlayerAbilitiesPacket, ServerboundPlayerActionAction as Action};

/// What the digging logic needs from the level.
pub trait BlockBreakLevel: LevelAccessor {
    /// `destroyBlockProgress(breakerId, pos, progress)`: broadcast to nearby players.
    fn destroy_block_progress(&mut self, breaker: EntityId, pos: BlockPos, progress: i32);
    /// `mayInteract(player, pos)` (world border / spawn protection).
    fn may_interact(&self, _player: EntityId, _pos: BlockPos) -> bool {
        true
    }
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

    /// `ServerPlayerGameMode.changeGameModeForPlayer(new)`: false when unchanged. The tab-list broadcast
    /// (`ClientboundPlayerInfoUpdatePacket`) is the caller's (PlayerList).
    pub fn change_game_mode_for_player(&mut self, new: GameType) -> bool {
        let old = self.game_mode.game_mode_for_player;
        if new == old {
            return false;
        }
        self.set_game_mode_for_player(new, Some(old));
        if self.player.abilities.flying && new != GameType::Spectator {
            // `isInRangeOfGround()` needs the level: the caller clears `flying` via `clear_flying_if_near_ground`
        }
        self.on_update_abilities();
        self.send(&ClientboundGameEventPacket { event: ClientboundGameEventPacket::CHANGE_GAME_MODE, param: game_type_id(new) as f32 });
        if new == GameType::Creative {
            self.player.living.current_impulse_impact_pos = None;
            self.player.living.current_impulse_context_reset_grace_time = 0;
        }
        true
    }

    /// `onUpdateAbilities()` (server side: `ClientboundPlayerAbilitiesPacket`).
    pub fn on_update_abilities(&mut self) {
        let a = &self.player.abilities;
        let packet = ClientboundPlayerAbilitiesPacket {
            invulnerable: a.invulnerable,
            is_flying: a.flying,
            can_fly: a.mayfly,
            instabuild: a.instabuild,
            flying_speed: a.get_flying_speed(),
            walking_speed: a.get_walking_speed(),
        };
        self.send(&packet);
    }

    /// `ServerPlayerGameMode.handleBlockBreakAction(pos, action, direction, maxY, sequence)`.
    pub fn handle_block_break_action<L: BlockBreakLevel>(&mut self, level: &mut L, pos: BlockPos, action: Action, direction: Direction, max_y: i32, _sequence: i32) {
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

    fn abort_destroy_block<L: BlockBreakLevel>(&mut self, level: &mut L, pos: BlockPos) {
        let id = self.base_id();
        self.game_mode.is_destroying_block = false;
        if self.game_mode.destroy_pos != pos {
            log::warn!("Mismatch in destroy block pos: {} {}", self.game_mode.destroy_pos, pos);
            level.destroy_block_progress(id, self.game_mode.destroy_pos, -1);
        }
        level.destroy_block_progress(id, pos, -1);
    }

    /// `destroyAndAck(pos, sequence, exitId)` (the sequence ack packet is sent by the listener).
    pub fn destroy_and_ack<L: BlockBreakLevel>(&mut self, level: &mut L, pos: BlockPos) {
        if !self.destroy_block(level, pos) {
            self.send(&ClientboundBlockUpdatePacket { pos, block_state: level.get_block_state(pos) });
        }
    }

    /// `Player.blockActionRestricted(level, pos, gameType)` (adventure/spectator rules; the adventure `CAN_BREAK`
    /// component check is not modelled: adventure players are always restricted when `mayBuild` is false).
    pub fn block_action_restricted<L: BlockGetter>(&self, _level: &L, _pos: BlockPos) -> bool {
        match self.game_mode.game_mode_for_player {
            GameType::Spectator => true,
            GameType::Adventure => !self.player.abilities.may_build,
            _ => false,
        }
    }

    /// `ServerPlayerGameMode.destroyBlock(pos)` without the block `playerWillDestroy`/`destroy`/`playerDestroy`
    /// (drops) hooks.
    pub fn destroy_block<L: BlockBreakLevel>(&mut self, level: &mut L, pos: BlockPos) -> bool {
        if self.block_action_restricted(level, pos) {
            return false;
        }
        level.remove_block(pos, false);
        true
    }

    /// `ServerPlayerGameMode.tick()` minus the `gameTicks++` (done in `ServerPlayer::tick_server_player`): digging progress broadcast.
    pub fn tick_game_mode<L: BlockBreakLevel>(&mut self, level: &mut L) {
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

    fn increment_destroy_progress<L: BlockBreakLevel>(&mut self, level: &mut L, state: BlockState, pos: BlockPos, ticks_spent: i32) -> f32 {
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
