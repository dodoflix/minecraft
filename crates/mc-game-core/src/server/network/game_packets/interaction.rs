//! Player interaction packets: `handlePlayerAction`, `handleUseItemOn`, `handleUseItem`, `handleInteract`,
//! `handleAttack`, `handlePunch`, `handleSetCarriedItem`, `handleSpectatorAction`, `handleTeleportToEntityPacket`,
//! `handlePaddleBoat` and `handlePlayerCommand`. The listener does the validation vanilla does here (reach,
//! hit-location sanity, build limits, spawn protection, sequence acknowledgement); the item / game mode / riding
//! systems are reached through [`PlayerSystems`](super::hooks::PlayerSystems).

use super::PlayCtx;
use super::hooks::InteractionHand as Hand;
use crate::world::entity::{Entity, EntityId};
use mc_core::direction::Direction;
use mc_core::math::mth;
use mc_core::phys::{AABB, Vec3};
use mc_core::pos::BlockPos;
use mc_protocol::packets::play::{
    ClientboundBlockUpdatePacket, InteractionHand, ServerboundAttackPacket,
    ServerboundInteractPacket, ServerboundPaddleBoatPacket, ServerboundPlayerActionAction,
    ServerboundPlayerActionPacket, ServerboundPlayerCommandAction, ServerboundPlayerCommandPacket,
    ServerboundPunchPacket, ServerboundSetCarriedItemPacket, ServerboundSpectatorActionPacket,
    ServerboundTeleportToEntityPacket, ServerboundUseItemOnPacket, ServerboundUseItemPacket,
};
use mc_text::Component;

/// `Inventory.getSelectionSize()`.
const SELECTION_SIZE: i32 = 9;

/// What the handlers read from `level.getEntityOrPart(id)`.
pub(super) struct TargetInfo {
    pub id: EntityId,
    pub block_position: BlockPos,
    pub bounding_box: AABB,
    pub is_item_entity: bool,
    pub is_experience_orb: bool,
    pub is_pickable: bool,
}

impl TargetInfo {
    fn of(entity: &dyn Entity) -> Self {
        let ty = entity.get_type();
        Self {
            id: entity.id(),
            block_position: entity.block_position(),
            bounding_box: entity.base().bounding_box(),
            is_item_entity: ty == mc_data::EntityType::ITEM,
            is_experience_orb: ty == mc_data::EntityType::EXPERIENCE_ORB,
            is_pickable: entity.is_pickable(),
        }
    }
}

fn hand_of(hand: InteractionHand) -> Hand {
    match hand {
        InteractionHand::MainHand => Hand::MainHand,
        InteractionHand::OffHand => Hand::OffHand,
    }
}

impl PlayCtx<'_> {
    /// `level.getEntityOrPart(id)`; the player itself is taken out of the level, so it is answered from `self.player`.
    fn target_info(&self, id: EntityId) -> Option<TargetInfo> {
        if id == self.player.id() {
            return Some(TargetInfo::of(&*self.player));
        }
        self.level.get_entity(id).map(TargetInfo::of)
    }

    /// `level.getWorldBorder().isWithinBounds(pos)`.
    fn within_world_border(&self, pos: BlockPos) -> bool {
        self.level.is_within_world_border(pos)
    }

    /// `handlePlayerAction`.
    pub fn handle_player_action(&mut self, packet: &ServerboundPlayerActionPacket) {
        if !self.has_client_loaded() {
            return;
        }
        self.player.reset_last_action_time();
        use ServerboundPlayerActionAction as A;
        match packet.action {
            A::Stab => {
                if self.player.is_spectator() || self.sys().cannot_attack_with_main_hand(self, 5) {
                    return;
                }
                self.sys().stab_with_piercing_weapon(self);
            }
            A::SwapItemWithOffhand => {
                if !self.player.is_spectator() {
                    self.item_sys().swap_item_with_offhand(self);
                    self.item_sys().stop_using_item(self);
                }
            }
            A::DropItem => {
                if !self.player.is_spectator() {
                    self.item_sys().drop_selected_item(self, false);
                }
            }
            A::DropAllItems => {
                if !self.player.is_spectator() {
                    self.item_sys().drop_selected_item(self, true);
                }
            }
            A::ReleaseUseItem => self.item_sys().release_using_item(self),
            A::StartDestroyBlock
            | A::ChangeDestroyDirection
            | A::AbortDestroyBlock
            | A::StopDestroyBlock => {
                let max_y = self.max_y();
                let under_spawn_protection = A::StartDestroyBlock == packet.action
                    && self.is_under_spawn_protection(packet.pos);
                self.player.handle_block_break_action(
                    self.level,
                    packet.pos,
                    packet.action,
                    packet.direction,
                    max_y,
                    under_spawn_protection,
                );
                self.lis().ack_block_changes_up_to(packet.sequence);
            }
        }
    }

    /// `handleUseItemOn`.
    pub fn handle_use_item_on(&mut self, packet: &ServerboundUseItemOnPacket) {
        if !self.has_client_loaded() {
            return;
        }
        self.lis().ack_block_changes_up_to(packet.sequence);
        let hand = hand_of(packet.hand);
        if !self.item_sys().item_in_hand_enabled(self, hand) {
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
        let max_y = self.max_y();
        let min_y = self.min_y();
        if pos.y() > max_y {
            self.sys().send_build_limit_message(self, true, max_y);
        } else if pos.y() < min_y {
            self.sys().send_build_limit_message(self, false, min_y);
        } else {
            let swing_animation = self.item_sys().swing_animation(self, hand, false);
            if self.is_under_spawn_protection(pos) {
                self.sys().send_spawn_protection_message(self, pos);
            } else if self.lis().awaiting_position_from_client.is_none() && self.may_interact(pos) {
                let result = self.item_sys().use_item_on(self, hand, block_hit);
                if result.consumes_action() {
                    self.sys().trigger_any_block_use(self, pos, hand);
                }
                // The decompiled vanilla code swings in both of these branches; the second swing is a no-op for the
                // player but is kept to preserve the packet sequence.
                let success_swing = match &result {
                    super::hooks::InteractionResult::Success(s) if s.should_swing() => {
                        Some(s.swing_source != crate::world::item::SwingSource::Predicted)
                    }
                    _ => None,
                };
                if direction == Direction::Up
                    && !result.consumes_action()
                    && pos.y() >= max_y
                    && self.item_sys().was_block_placement_attempt(self, hand)
                {
                    self.sys().send_build_limit_message(self, true, max_y);
                } else if let Some(send) = success_swing {
                    self.item_sys().swing_and_reset_attack_strength(
                        self,
                        hand,
                        swing_animation,
                        send,
                    );
                }
                if !result.consumes_action()
                    && self.item_sys().was_block_placement_attempt(self, hand)
                {
                    if direction == Direction::Up && pos.y() >= max_y {
                        self.sys().send_build_limit_message(self, true, max_y);
                    } else if direction == Direction::Down && pos.y() <= min_y {
                        self.sys().send_build_limit_message(self, false, min_y);
                    }
                } else if let Some(send) = success_swing {
                    self.item_sys().swing_and_reset_attack_strength(
                        self,
                        hand,
                        swing_animation,
                        send,
                    );
                }
            } else {
                self.sys().send_build_limit_message(self, true, max_y);
            }
            let state = self.block_state_at(pos);
            self.send(&ClientboundBlockUpdatePacket {
                pos,
                block_state: state,
            });
            let relative = pos.relative(direction);
            let state = self.block_state_at(relative);
            self.send(&ClientboundBlockUpdatePacket {
                pos: relative,
                block_state: state,
            });
        }
    }

    /// `DedicatedServer.isUnderSpawnProtection(level, pos, player)`.
    fn is_under_spawn_protection(&mut self, pos: BlockPos) -> bool {
        let respawn = self.server.get_respawn_data();
        if self.level.dimension != respawn.dimension
            || self.server.player_list.ops.is_empty()
            || self.player_is_op()
        {
            return false;
        }
        let radius = self.server.spawn_protection_radius();
        if radius <= 0 {
            return false;
        }
        let xd = (pos.x() - respawn.pos.x()).abs();
        let zd = (pos.z() - respawn.pos.z()).abs();
        xd.max(zd) <= radius
    }

    /// `level.mayInteract(player, pos)`: world border (spawn protection is checked separately).
    fn may_interact(&self, pos: BlockPos) -> bool {
        self.within_world_border(pos)
    }

    /// `handleUseItem`.
    pub fn handle_use_item(&mut self, packet: &ServerboundUseItemPacket) {
        if !self.has_client_loaded() {
            return;
        }
        self.lis().ack_block_changes_up_to(packet.sequence);
        let hand = hand_of(packet.hand);
        self.player.reset_last_action_time();
        if self.item_sys().item_in_hand_is_empty(self, hand)
            || !self.item_sys().item_in_hand_enabled(self, hand)
        {
            return;
        }
        let swing_animation = self.item_sys().swing_animation(self, hand, false);
        let target_y_rot = mth::wrap_degrees_f32(packet.y_rot);
        let target_x_rot = mth::wrap_degrees_f32(packet.x_rot);
        if target_x_rot != self.player.get_x_rot() || target_y_rot != self.player.get_y_rot() {
            self.player.abs_snap_rotation_to(target_y_rot, target_x_rot);
        }
        if let super::hooks::InteractionResult::Success(s) = self.item_sys().use_item(self, hand)
            && s.should_swing()
        {
            let send = s.swing_source != crate::world::item::SwingSource::Predicted;
            self.item_sys()
                .swing_and_reset_attack_strength(self, hand, swing_animation, send);
        }
    }

    /// `handlePunch`.
    pub fn handle_punch(&mut self, _packet: &ServerboundPunchPacket) {
        self.player.reset_last_action_time();
        let animation = self.item_sys().swing_animation(self, Hand::MainHand, true);
        self.item_sys()
            .swing(self, Hand::MainHand, animation, false);
        self.sys().reset_attack_strength_ticker(self);
    }

    /// `handleAttack`.
    pub fn handle_attack(&mut self, packet: &ServerboundAttackPacket) {
        if !self.has_client_loaded() || self.player.is_spectator() {
            return;
        }
        let target = self.target_info(packet.entity_id);
        self.player.reset_last_action_time();
        let Some(target) = target else { return };
        if !self.within_world_border(target.block_position) {
            return;
        }
        if !self
            .sys()
            .is_within_attack_range(self, &target.bounding_box, 3.0)
            || self.sys().main_hand_has_piercing_weapon(self)
        {
            return;
        }
        // `AbstractArrow && !isAttackable()`: not implemented (no arrow entity)
        let unattackable_arrow = false;
        if target.is_item_entity
            || target.is_experience_orb
            || target.id == self.player.id()
            || unattackable_arrow
        {
            self.disconnect(Component::translatable(
                "multiplayer.disconnect.invalid_entity_attacked",
                vec![],
            ));
            log::warn!(
                "Player {} tried to attack an invalid entity",
                self.player.get_plain_text_name()
            );
        } else if self.item_sys().item_in_hand_enabled(self, Hand::MainHand)
            && !self.sys().cannot_attack_with_main_hand(self, 5)
        {
            self.sys().attack(self, target.id);
        }
    }

    /// `handleInteract`.
    pub fn handle_interact(&mut self, packet: &ServerboundInteractPacket) {
        if !self.has_client_loaded() {
            return;
        }
        let target = self.target_info(packet.entity_id);
        self.player.reset_last_action_time();
        self.player
            .set_shift_key_down(packet.using_secondary_action);
        let Some(target) = target else { return };
        if !self.within_world_border(target.block_position)
            || !self
                .player
                .is_within_entity_interaction_range(&target.bounding_box, 3.0)
        {
            return;
        }
        let hand = hand_of(packet.hand);
        if !self.item_sys().item_in_hand_enabled(self, hand) {
            return;
        }
        let swing_animation = self.item_sys().swing_animation(self, hand, false);
        let used_item_stack = self.player.get_item_in_hand(hand);
        if let super::hooks::InteractionResult::Success(s) =
            self.sys()
                .interact_on(self, target.id, hand, packet.location)
        {
            let awarded_for_stack = if s.was_item_interaction() {
                used_item_stack
            } else {
                crate::world::item::ItemStack::empty()
            };
            self.sys()
                .trigger_player_interacted_with_entity(self, awarded_for_stack, target.id);
            if s.should_swing() {
                let send = s.swing_source != crate::world::item::SwingSource::Predicted;
                self.item_sys()
                    .swing_and_reset_attack_strength(self, hand, swing_animation, send);
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
                && self.within_world_border(target.block_position)
                && self
                    .player
                    .is_within_entity_interaction_range(&target.bounding_box, 3.0)
                && target.is_pickable
            {
                self.sys().set_camera(self, target.id);
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
            let found = if dimension == self.level.dimension {
                self.level
                    .get_entity_by_uuid(packet.uuid)
                    .map(|e| (e.position(), e.base().y_rot(), e.base().x_rot()))
            } else {
                self.server.level(&dimension).and_then(|l| {
                    l.get_entity_by_uuid(packet.uuid)
                        .map(|e| (e.position(), e.base().y_rot(), e.base().x_rot()))
                })
            };
            if let Some((pos, y_rot, x_rot)) = found {
                self.sys()
                    .teleport_to(self, &dimension, pos.x, pos.y, pos.z, y_rot, x_rot, true);
                return;
            }
        }
    }

    /// `handlePaddleBoat`.
    pub fn handle_paddle_boat(&mut self, packet: &ServerboundPaddleBoatPacket) {
        self.sys()
            .set_controlled_boat_paddle_state(self, packet.left, packet.right);
    }

    /// `handleSetCarriedItem`.
    pub fn handle_set_carried_item(&mut self, packet: &ServerboundSetCarriedItemPacket) {
        let slot = i32::from(packet.slot);
        if (0..SELECTION_SIZE).contains(&slot) {
            if self.player.player.inventory.get_selected_slot() != slot
                && self.item_sys().get_used_item_hand(self) == Some(Hand::MainHand)
            {
                self.item_sys().stop_using_item(self);
            }
            // In range by the check above, so vanilla's `IllegalArgumentException` cannot happen.
            let _ = self.player.player.inventory.set_selected_slot(slot);
            self.player.reset_last_action_time();
        } else {
            log::warn!(
                "{} tried to set an invalid carried item",
                self.player.get_plain_text_name()
            );
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
                    self.sys().stop_sleep_in_bed(self, false, true);
                    let position = self.player.position();
                    self.lis().awaiting_position_from_client = Some(position);
                }
            }
            A::StartRidingJump => self.sys().start_riding_jump(self, packet.data),
            A::StopRidingJump => self.sys().stop_riding_jump(self),
            A::OpenInventory => self.sys().open_vehicle_inventory(self),
            A::StartFallFlying => {
                if !self.sys().try_to_start_fall_flying(self) {
                    self.sys().stop_fall_flying(self);
                }
            }
        }
    }
}
