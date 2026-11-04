//! Player interaction packets: `handlePlayerAction`, `handleUseItemOn`, `handleUseItem`, `handleInteract`,
//! `handleAttack`, `handlePunch`, `handleSetCarriedItem`, `handleSpectatorAction`, `handleTeleportToEntityPacket`,
//! `handlePaddleBoat` and `handlePlayerCommand`. The listener does the validation vanilla does here (reach,
//! hit-location sanity, build limits, spawn protection, sequence acknowledgement) and hands over to the game mode /
//! player / item systems.

use super::PlayCtx;
use crate::world::entity::Hand;
use mc_core::direction::Direction;
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_protocol::packets::play::{
    ClientboundBlockUpdatePacket, InteractionHand, ServerboundAttackPacket, ServerboundInteractPacket, ServerboundPaddleBoatPacket,
    ServerboundPlayerActionAction, ServerboundPlayerActionPacket, ServerboundPlayerCommandAction, ServerboundPlayerCommandPacket,
    ServerboundPunchPacket, ServerboundSetCarriedItemPacket, ServerboundSpectatorActionPacket, ServerboundTeleportToEntityPacket,
    ServerboundUseItemOnPacket, ServerboundUseItemPacket,
};
use mc_text::Component;

/// What the handlers read from `level.getEntityOrPart(id)`.
pub(super) struct TargetInfo {
    pub id: i32,
    pub block_position: mc_core::pos::BlockPos,
    pub bounding_box: mc_core::phys::AABB,
    pub is_item_entity: bool,
    pub is_experience_orb: bool,
    /// `AbstractArrow && !isAttackable()`.
    pub is_unattackable_arrow: bool,
    pub is_pickable: bool,
}

impl TargetInfo {
    fn of(entity: &dyn crate::world::entity::Entity) -> Self {
        let ty = entity.get_type();
        Self {
            id: entity.id(),
            block_position: entity.block_position(),
            bounding_box: entity.base().bounding_box,
            is_item_entity: ty == mc_data::EntityType::ITEM,
            is_experience_orb: ty == mc_data::EntityType::EXPERIENCE_ORB,
            is_unattackable_arrow: entity.is_unattackable_arrow(),
            is_pickable: entity.is_pickable(),
        }
    }
}

/// `Inventory.getSelectionSize()`.
const SELECTION_SIZE: i32 = 9;

impl PlayCtx<'_> {
    /// `level.getEntityOrPart(id)`; the player itself is taken out of the level, so it is answered from `self.player`.
    fn target_info(&self, id: i32) -> Option<TargetInfo> {
        if id == self.player.id() {
            return Some(TargetInfo::of(&*self.player));
        }
        self.level.get_entity_or_part(id).map(TargetInfo::of)
    }

    /// `handlePlayerAction`.
    pub fn handle_player_action(&mut self, packet: &ServerboundPlayerActionPacket) {
        if !self.has_client_loaded() {
            return;
        }
        let pos = packet.pos;
        self.player.reset_last_action_time();
        use ServerboundPlayerActionAction as A;
        match packet.action {
            A::Stab => {
                if self.player.is_spectator() {
                    return;
                }
                if self.player.cannot_attack_with_item(self.level, Hand::MainHand, 5) {
                    return;
                }
                self.player.stab_with_piercing_weapon(self.level);
            }
            A::SwapItemWithOffhand => {
                if !self.player.is_spectator() {
                    self.player.swap_hands();
                    self.player.stop_using_item();
                }
            }
            A::DropItem => {
                if !self.player.is_spectator() {
                    self.player.drop_selected_item(self.level, false);
                }
            }
            A::DropAllItems => {
                if !self.player.is_spectator() {
                    self.player.drop_selected_item(self.level, true);
                }
            }
            A::ReleaseUseItem => self.player.release_using_item(self.level),
            A::StartDestroyBlock | A::ChangeDestroyDirection | A::AbortDestroyBlock | A::StopDestroyBlock => {
                let max_y = self.level.get_max_y();
                self.player.game_mode_handle_block_break_action(self.level, pos, packet.action, packet.direction, max_y, packet.sequence);
                self.lis().ack_block_changes_up_to(packet.sequence);
            }
        }
    }

    /// `wasBlockPlacementAttempt`.
    fn was_block_placement_attempt(&self) -> bool {
        self.player.main_hand_was_block_placement_attempt(Hand::MainHand)
    }

    /// `handleUseItemOn`.
    pub fn handle_use_item_on(&mut self, packet: &ServerboundUseItemOnPacket) {
        if !self.has_client_loaded() {
            return;
        }
        self.lis().ack_block_changes_up_to(packet.sequence);
        let hand = hand_of(packet.hand);
        if !self.player.item_in_hand_enabled(self.level, hand) {
            return;
        }
        let block_hit = &packet.hit_result;
        let location = block_hit.location();
        let pos = block_hit.block_pos;
        if !self.player.is_within_block_interaction_range(pos, 1.0) {
            return;
        }
        let distance = location.subtract(Vec3::at_center_of(&pos));
        let limit = 1.0000001;
        if !(distance.x.abs() < limit && distance.y.abs() < limit && distance.z.abs() < limit) {
            log::warn!(
                "Rejecting UseItemOnPacket from {}: Location {} too far away from hit block {}.",
                self.player.get_profile().name,
                location,
                pos
            );
            return;
        }
        let direction = block_hit.direction;
        self.player.reset_last_action_time();
        let max_y = self.level.get_max_y();
        let min_y = self.level.get_min_y();
        if pos.y() > max_y {
            self.player.send_build_limit_message(true, max_y);
        } else if pos.y() < min_y {
            self.player.send_build_limit_message(false, min_y);
        } else {
            let swing_animation = self.player.get_interact_animation(hand);
            if self.server.is_under_spawn_protection(self.level, pos, &*self.player) {
                self.player.send_spawn_protection_message(pos);
            } else if self.lis().awaiting_position_from_client.is_none() && self.level.may_interact(&*self.player, pos) {
                let result = self.player.game_mode_use_item_on(self.level, hand, block_hit);
                if result.consumes_action() {
                    self.player.trigger_any_block_use(self.level, pos, hand);
                }
                // The decompiled vanilla code swings in both of these branches (once more than needed); the second
                // swing is a no-op for the player but is kept to preserve the packet sequence.
                if direction == Direction::Up && !result.consumes_action() && pos.y() >= max_y && self.was_block_placement_attempt() {
                    self.player.send_build_limit_message(true, max_y);
                } else if let Some(success) = result.success().filter(|s| s.should_swing()) {
                    self.player.swing_and_reset_attack_strength(self.level, hand, swing_animation, !success.swing_predicted());
                }
                if !result.consumes_action() && self.was_block_placement_attempt() {
                    if direction == Direction::Up && pos.y() >= max_y {
                        self.player.send_build_limit_message(true, max_y);
                    } else if direction == Direction::Down && pos.y() <= min_y {
                        self.player.send_build_limit_message(false, min_y);
                    }
                } else if let Some(success) = result.success().filter(|s| s.should_swing()) {
                    self.player.swing_and_reset_attack_strength(self.level, hand, swing_animation, !success.swing_predicted());
                }
            } else {
                self.player.send_build_limit_message(true, max_y);
            }
            let state = self.level.get_block_state_at(pos);
            self.send(&ClientboundBlockUpdatePacket { pos, block_state: state });
            let relative = pos.relative(direction);
            let state = self.level.get_block_state_at(relative);
            self.send(&ClientboundBlockUpdatePacket { pos: relative, block_state: state });
        }
    }

    /// `handleUseItem`.
    pub fn handle_use_item(&mut self, packet: &ServerboundUseItemPacket) {
        if !self.has_client_loaded() {
            return;
        }
        self.lis().ack_block_changes_up_to(packet.sequence);
        let hand = hand_of(packet.hand);
        self.player.reset_last_action_time();
        if self.player.item_in_hand_is_empty(hand) || !self.player.item_in_hand_enabled(self.level, hand) {
            return;
        }
        let swing_animation = self.player.get_interact_animation(hand);
        let target_y_rot = mth::wrap_degrees_f32(packet.y_rot);
        let target_x_rot = mth::wrap_degrees_f32(packet.x_rot);
        if target_x_rot != self.player.get_x_rot() || target_y_rot != self.player.get_y_rot() {
            self.player.abs_snap_rotation_to(target_y_rot, target_x_rot);
        }
        let result = self.player.game_mode_use_item(self.level, hand);
        if let Some(success) = result.success().filter(|s| s.should_swing()) {
            self.player.swing_and_reset_attack_strength(self.level, hand, swing_animation, !success.swing_predicted());
        }
    }

    /// `handlePunch`.
    pub fn handle_punch(&mut self, _packet: &ServerboundPunchPacket) {
        self.player.reset_last_action_time();
        let swing_animation = self.player.get_attack_animation(Hand::MainHand);
        self.player.swing(self.level, Hand::MainHand, swing_animation, false);
        self.player.reset_attack_strength_ticker();
    }

    /// `handleAttack`.
    pub fn handle_attack(&mut self, packet: &ServerboundAttackPacket) {
        if !self.has_client_loaded() || self.player.is_spectator() {
            return;
        }
        let target = self.target_info(packet.entity_id);
        self.player.reset_last_action_time();
        let Some(target) = target else { return };
        if !self.level.world_border().is_within_bounds(target.block_position) {
            return;
        }
        if !self.player.is_within_attack_range(Hand::MainHand, &target.bounding_box, 3.0) || self.player.main_hand_has_piercing_weapon() {
            return;
        }
        if target.is_item_entity || target.is_experience_orb || target.id == self.player.id() || target.is_unattackable_arrow {
            self.disconnect(Component::translatable("multiplayer.disconnect.invalid_entity_attacked", vec![]));
            log::warn!("Player {} tried to attack an invalid entity", self.player.get_plain_text_name());
        } else if self.player.item_in_hand_enabled(self.level, Hand::MainHand) && !self.player.cannot_attack_with_item(self.level, Hand::MainHand, 5) {
            self.player.attack(self.level, target.id);
        }
    }

    /// `handleInteract`.
    pub fn handle_interact(&mut self, packet: &ServerboundInteractPacket) {
        if !self.has_client_loaded() {
            return;
        }
        let target = self.target_info(packet.entity_id);
        self.player.reset_last_action_time();
        self.player.set_shift_key_down(packet.using_secondary_action);
        let Some(target) = target else { return };
        if !self.level.world_border().is_within_bounds(target.block_position) {
            return;
        }
        if !self.player.is_within_entity_interaction_range(&target.bounding_box, 3.0) {
            return;
        }
        let hand = hand_of(packet.hand);
        if !self.player.item_in_hand_enabled(self.level, hand) {
            return;
        }
        let used_item_stack = self.player.copy_item_in_hand(hand);
        let result = self.player.interact_on(self.level, target.id, hand, packet.location);
        if let Some(success) = result.success() {
            let awarded_for_stack = if success.was_item_interaction() { used_item_stack.clone() } else { Default::default() };
            self.player.trigger_player_interacted_with_entity(self.level, awarded_for_stack, target.id);
            if success.should_swing() {
                let swing_animation = used_item_stack.get_interact_animation();
                self.player.swing_and_reset_attack_strength(self.level, hand, swing_animation, !success.swing_predicted());
            }
        }
    }

    /// `handleSpectatorAction`.
    pub fn handle_spectator_action(&mut self, packet: &ServerboundSpectatorActionPacket) {
        if !(self.has_client_loaded() && self.player.is_spectator()) {
            return;
        }
        self.player.reset_last_action_time();
        if let Some(entity_id) = packet.spectate_entity_id {
            let target = self.target_info(entity_id);
            if let Some(target) = target
                && self.level.world_border().is_within_bounds(target.block_position)
                && self.player.is_within_entity_interaction_range(&target.bounding_box, 3.0)
                && target.is_pickable
            {
                self.player.set_camera(self.level, target.id);
            }
        }
    }

    /// `handleTeleportToEntityPacket`.
    pub fn handle_teleport_to_entity_packet(&mut self, packet: &ServerboundTeleportToEntityPacket) {
        if !self.player.is_spectator() {
            return;
        }
        // `for (ServerLevel level : server.getAllLevels()) { entity = packet.getEntity(level); ... }`
        for dimension in self.server.level_keys() {
            let found = if dimension == *self.level.dimension() {
                self.level.get_entity_by_uuid_position(packet.uuid)
            } else {
                self.server.level(&dimension).and_then(|l| l.get_entity_by_uuid_position(packet.uuid))
            };
            if let Some((pos, y_rot, x_rot)) = found {
                self.player.teleport_to_level(self.server, self.level, &dimension, pos.x, pos.y, pos.z, y_rot, x_rot, true);
                return;
            }
        }
    }

    /// `handlePaddleBoat`.
    pub fn handle_paddle_boat(&mut self, packet: &ServerboundPaddleBoatPacket) {
        self.player.set_controlled_boat_paddle_state(self.level, packet.left, packet.right);
    }

    /// `handleSetCarriedItem`.
    pub fn handle_set_carried_item(&mut self, packet: &ServerboundSetCarriedItemPacket) {
        let slot = i32::from(packet.slot);
        if (0..SELECTION_SIZE).contains(&slot) {
            if self.player.get_selected_slot() != slot && self.player.get_used_item_hand() == Some(Hand::MainHand) {
                self.player.stop_using_item();
            }
            self.player.set_selected_slot(slot);
            self.player.reset_last_action_time();
        } else {
            log::warn!("{} tried to set an invalid carried item", self.player.get_plain_text_name());
        }
    }

    /// `handlePlayerCommand`.
    pub fn handle_player_command(&mut self, packet: &ServerboundPlayerCommandPacket) {
        if !self.has_client_loaded() {
            return;
        }
        self.player.reset_last_action_time();
        use ServerboundPlayerCommandAction as A;
        match packet.action {
            A::StartSprinting => self.player.set_sprinting(true),
            A::StopSprinting => self.player.set_sprinting(false),
            A::StopSleeping => {
                if self.player.is_sleeping() {
                    self.player.stop_sleep_in_bed(self.level, false, true);
                    let position = self.player.position();
                    self.lis().awaiting_position_from_client = Some(position);
                }
            }
            A::StartRidingJump => {
                let data = packet.data;
                self.player.start_riding_jump(self.level, data);
            }
            A::StopRidingJump => self.player.stop_riding_jump(self.level),
            A::OpenInventory => self.player.open_vehicle_inventory(self.level),
            A::StartFallFlying => {
                if !self.player.try_to_start_fall_flying(self.level) {
                    self.player.stop_fall_flying();
                }
            }
        }
    }
}

fn hand_of(hand: InteractionHand) -> Hand {
    match hand {
        InteractionHand::MainHand => Hand::MainHand,
        InteractionHand::OffHand => Hand::OffHand,
    }
}
