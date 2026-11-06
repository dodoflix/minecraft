//! Movement: `handleMovePlayer`/`handlePlayerPositionChange`, `handleMoveVehicle`, `handleAcceptTeleportPacket`,
//! `teleport`, the `tick()` floating/keep-alive/idle logic and the support-block resend. Every threshold, log line
//! and ordering is vanilla's `ServerGamePacketListenerImpl`.

use super::{PlayCtx, seam};
use crate::world::entity::{Entity, MoverType};
use crate::world::level::BlockGetter;
use mc_core::math::mth;
use mc_core::phys::shapes::{BooleanOp, Shapes};
use mc_core::phys::{AABB, Vec3};
use mc_core::pos::BlockPos;
use mc_protocol::packets::play::{
    ClientboundBlockChangedAckPacket, ClientboundBlockUpdatePacket, ClientboundMoveVehiclePacket,
    PositionAndRotation, ServerboundAcceptTeleportationPacket, ServerboundMovePlayerPosPacket,
    ServerboundMovePlayerPosRotPacket, ServerboundMovePlayerRotPacket,
    ServerboundMovePlayerStatusOnlyPacket, ServerboundMoveVehiclePacket,
    ServerboundPlayerLoadedPacket,
};
use mc_text::Component;

/// The abstract `ServerboundMovePlayerPacket` view over its four subclasses (`getX(default)` etc.).
pub struct MovePlayer {
    pos: Option<Vec3>,
    rot: Option<(f32, f32)>,
    on_ground: bool,
    horizontal_collision: bool,
}

impl MovePlayer {
    fn get_x(&self, fallback: f64) -> f64 {
        self.pos.map_or(fallback, |p| p.x)
    }
    fn get_y(&self, fallback: f64) -> f64 {
        self.pos.map_or(fallback, |p| p.y)
    }
    fn get_z(&self, fallback: f64) -> f64 {
        self.pos.map_or(fallback, |p| p.z)
    }
    fn get_y_rot(&self, fallback: f32) -> f32 {
        self.rot.map_or(fallback, |r| r.0)
    }
    fn get_x_rot(&self, fallback: f32) -> f32 {
        self.rot.map_or(fallback, |r| r.1)
    }
    fn has_position(&self) -> bool {
        self.pos.is_some()
    }
}

impl From<&ServerboundMovePlayerPosPacket> for MovePlayer {
    fn from(p: &ServerboundMovePlayerPosPacket) -> Self {
        Self {
            pos: Some(Vec3::new(p.x, p.y, p.z)),
            rot: None,
            on_ground: p.on_ground,
            horizontal_collision: p.horizontal_collision,
        }
    }
}
impl From<&ServerboundMovePlayerPosRotPacket> for MovePlayer {
    fn from(p: &ServerboundMovePlayerPosRotPacket) -> Self {
        Self {
            pos: Some(Vec3::new(p.x, p.y, p.z)),
            rot: Some((p.y_rot, p.x_rot)),
            on_ground: p.on_ground,
            horizontal_collision: p.horizontal_collision,
        }
    }
}
impl From<&ServerboundMovePlayerRotPacket> for MovePlayer {
    fn from(p: &ServerboundMovePlayerRotPacket) -> Self {
        Self {
            pos: None,
            rot: Some((p.y_rot, p.x_rot)),
            on_ground: p.on_ground,
            horizontal_collision: p.horizontal_collision,
        }
    }
}
impl From<&ServerboundMovePlayerStatusOnlyPacket> for MovePlayer {
    fn from(p: &ServerboundMovePlayerStatusOnlyPacket) -> Self {
        Self {
            pos: None,
            rot: None,
            on_ground: p.on_ground,
            horizontal_collision: p.horizontal_collision,
        }
    }
}

/// `ClientboundMoveVehiclePacket.fromEntity`.
fn move_vehicle_packet(entity: &dyn Entity) -> ClientboundMoveVehiclePacket {
    let base = entity.base();
    ClientboundMoveVehiclePacket {
        moving_to: PositionAndRotation {
            position: base.position(),
            y_rot: base.y_rot(),
            x_rot: base.x_rot(),
        },
    }
}

/// `containsInvalidValues`.
fn contains_invalid_values(x: f64, y: f64, z: f64, y_rot: f32, x_rot: f32) -> bool {
    x.is_nan() || y.is_nan() || z.is_nan() || !x_rot.is_finite() || !y_rot.is_finite()
}

fn clamp_horizontal(value: f64) -> f64 {
    mth::clamp_f64(value, -3.0E7, 3.0E7)
}

fn clamp_vertical(value: f64) -> f64 {
    mth::clamp_f64(value, -2.0E7, 2.0E7)
}

fn invalid_player_movement() -> Component {
    Component::translatable("multiplayer.disconnect.invalid_player_movement", vec![])
}

impl PlayCtx<'_> {
    /// `tick()`.
    pub fn tick(&mut self) {
        if self.lis().ack_block_changes_up_to > -1 {
            let sequence = self.lis().ack_block_changes_up_to;
            self.send(&ClientboundBlockChangedAckPacket { sequence });
            self.lis().ack_block_changes_up_to = -1;
        }

        if self.server.is_paused() || !self.tick_player() {
            let now = self.now_ms();
            self.keep_connection_alive(now);
            let lis = self.lis();
            lis.chat_spam_throttler.tick();
            lis.command_spam_throttler.tick();
            lis.drop_spam_throttler.tick();
            self.command_suggestions_tick();
            let idle_minutes = self.server.player_idle_timeout();
            let last_action_time = self.player.get_last_action_time();
            if last_action_time > 0
                && idle_minutes > 0
                && now - last_action_time > i64::from(idle_minutes) * 60_000
                && !self.player.won_game
            {
                self.disconnect(Component::translatable(
                    "multiplayer.disconnect.idling",
                    vec![],
                ));
            }
        }
    }

    /// `keepConnectionAlive()`.
    fn keep_connection_alive(&mut self, now_ms: i64) {
        use crate::server::network::common_listener::{
            KeepAliveAction, timeout_disconnection_message,
        };
        let action = self.lis().common.poll_keep_alive(now_ms);
        match action {
            KeepAliveAction::Nothing => {}
            KeepAliveAction::Send(id) => {
                self.send(&mc_protocol::packets::play::ClientboundKeepAlivePacket { id })
            }
            KeepAliveAction::Timeout => self.disconnect(timeout_disconnection_message()),
        }
    }

    /// `tickPlayer()`: returns whether the player was kicked.
    fn tick_player(&mut self) -> bool {
        self.reset_position();
        let p = self.player.position();
        self.player.player.living.entity.set_old_pos(p);
        self.player.do_tick(self.level);
        let (fx, fy, fz) = {
            let lis = self.lis();
            (lis.first_good_x, lis.first_good_y, lis.first_good_z)
        };
        let (y_rot, x_rot) = (self.player.get_y_rot(), self.player.get_x_rot());
        self.player.abs_snap_to(fx, fy, fz, y_rot, x_rot);
        self.lis().tick_count += 1;
        let received = self.lis().received_move_packet_count;
        self.lis().known_move_packet_count = received;
        if self.lis().client_is_floating
            && !self.player.is_sleeping()
            && !self.player.is_passenger()
            && !self.player.is_dead_or_dying()
        {
            self.lis().above_ground_tick_count += 1;
            if self.lis().above_ground_tick_count
                > self.maximum_flying_ticks(self.player.get_gravity())
            {
                log::warn!(
                    "{} was kicked for floating too long!",
                    self.player.get_plain_text_name()
                );
                self.disconnect(Component::translatable(
                    "multiplayer.disconnect.flying",
                    vec![],
                ));
                return true;
            }
        } else {
            let lis = self.lis();
            lis.client_is_floating = false;
            lis.above_ground_tick_count = 0;
        }

        let root_vehicle = self.root_vehicle();
        let player_id = self.player.id();
        match root_vehicle {
            Some(vehicle_id)
                if vehicle_id != player_id
                    && self.controlling_passenger(vehicle_id) == Some(player_id) =>
            {
                self.lis().last_vehicle = Some(vehicle_id);
                let pos = self
                    .level
                    .get_entity(vehicle_id)
                    .map_or(Vec3::ZERO, |e| e.position());
                let gravity = self
                    .level
                    .get_entity(vehicle_id)
                    .map_or(0.08, |e| e.get_gravity());
                let max_ticks = self.maximum_flying_ticks(gravity);
                let lis = self.lis();
                lis.vehicle_first_good_x = pos.x;
                lis.vehicle_first_good_y = pos.y;
                lis.vehicle_first_good_z = pos.z;
                lis.vehicle_last_good_x = pos.x;
                lis.vehicle_last_good_y = pos.y;
                lis.vehicle_last_good_z = pos.z;
                if lis.client_vehicle_is_floating {
                    lis.above_ground_vehicle_tick_count += 1;
                    if lis.above_ground_vehicle_tick_count > max_ticks {
                        log::warn!(
                            "{} was kicked for floating a vehicle too long!",
                            self.player.get_plain_text_name()
                        );
                        self.disconnect(Component::translatable(
                            "multiplayer.disconnect.flying",
                            vec![],
                        ));
                        return true;
                    }
                } else {
                    lis.client_vehicle_is_floating = false;
                    lis.above_ground_vehicle_tick_count = 0;
                }
            }
            _ => {
                let lis = self.lis();
                lis.last_vehicle = None;
                lis.client_vehicle_is_floating = false;
                lis.above_ground_vehicle_tick_count = 0;
            }
        }
        false
    }

    /// `getMaximumFlyingTicks(entity)` given the entity's gravity.
    fn maximum_flying_ticks(&self, gravity: f64) -> i32 {
        if gravity < f64::from(1.0E-5f32) {
            i32::MAX
        } else {
            let gravity_modifier = 0.08 / gravity;
            mth::ceil_f64(80.0 * gravity_modifier.max(1.0))
        }
    }

    /// `resetPosition()`.
    pub fn reset_position(&mut self) {
        let pos = self.player.position();
        self.lis().reset_position(pos);
    }

    /// `isSingleplayerOwner()`.
    pub(super) fn is_singleplayer_owner(&self) -> bool {
        false /* dedicated server: no singleplayer owner */
    }

    /// `handlePlayerInput`.
    pub fn handle_player_input(
        &mut self,
        packet: &mc_protocol::packets::play::ServerboundPlayerInputPacket,
    ) {
        self.player.set_last_client_input(packet.input);
        if self.has_client_loaded() {
            self.player.reset_last_action_time();
            self.player.set_shift_key_down(packet.input.shift);
        }
    }

    /// `handleMoveVehicle`.
    pub fn handle_move_vehicle(&mut self, packet: &ServerboundMoveVehiclePacket) {
        let moving_to = packet.moving_to;
        let to = moving_to.position;
        if contains_invalid_values(to.x, to.y, to.z, moving_to.y_rot, moving_to.x_rot) {
            self.disconnect(Component::translatable(
                "multiplayer.disconnect.invalid_vehicle_movement",
                vec![],
            ));
            return;
        }
        if self.update_awaiting_teleport() || !self.has_client_loaded() {
            return;
        }
        let player_id = self.player.id();
        let Some(vehicle_id) = self.root_vehicle() else {
            return;
        };
        let controlling = self.controlling_passenger(vehicle_id) == Some(player_id);
        let is_last_vehicle = self.lis().last_vehicle == Some(vehicle_id);
        if vehicle_id != player_id && controlling && is_last_vehicle {
            // The vehicle is taken out of the level for the duration of its handling (queries exclude it).
            let Some(mut vehicle) = self.level.take_entity(vehicle_id) else {
                return;
            };
            self.handle_vehicle_movement(&mut *vehicle, packet);
            self.level.put_back_entity(vehicle_id, vehicle);
        } else if vehicle_id != player_id && !controlling {
            let tick_count = self.lis().tick_count;
            let reset_at = self.lis().vehicle_position_last_reset_at;
            let distance_sqr = self
                .level
                .get_entity(vehicle_id)
                .map_or(0.0, |e| e.position().distance_to_sqr(to));
            if tick_count - reset_at > 20 || distance_sqr > mth::square_f64(1.0) {
                log::warn!(
                    "{} was expected to be controlling vehicle {} but was not. Resetting vehicle position.",
                    self.player.get_plain_text_name(),
                    self.level
                        .get_entity(vehicle_id)
                        .map(entity_plain_name)
                        .unwrap_or_default()
                );
                if let Some(vehicle) = self.level.get_entity(vehicle_id) {
                    let packet = move_vehicle_packet(vehicle);
                    self.send(&packet);
                }
                self.lis().vehicle_position_last_reset_at = tick_count;
            }
        }
    }

    fn handle_vehicle_movement(
        &mut self,
        vehicle: &mut dyn Entity,
        packet: &ServerboundMoveVehiclePacket,
    ) {
        let moving_to = packet.moving_to;
        let old_pos = vehicle.position();
        let (old_x, old_y, old_z) = (old_pos.x, old_pos.y, old_pos.z);
        let target_x = clamp_horizontal(moving_to.position.x);
        let target_y = clamp_vertical(moving_to.position.y);
        let target_z = clamp_horizontal(moving_to.position.z);
        let target_y_rot = mth::wrap_degrees_f32(moving_to.y_rot);
        let target_x_rot = mth::wrap_degrees_f32(moving_to.x_rot);
        let (fx, fy, fz, lx, ly, lz) = {
            let l = self.lis();
            (
                l.vehicle_first_good_x,
                l.vehicle_first_good_y,
                l.vehicle_first_good_z,
                l.vehicle_last_good_x,
                l.vehicle_last_good_y,
                l.vehicle_last_good_z,
            )
        };
        let mut x_dist = target_x - fx;
        let mut y_dist = target_y - fy;
        let mut z_dist = target_z - fz;
        let expected_dist = vehicle.base().delta_movement().length_sqr();
        let mut moved_dist = x_dist * x_dist + y_dist * y_dist + z_dist * z_dist;
        if moved_dist - expected_dist > 100.0 && !self.is_singleplayer_owner() {
            log::warn!(
                "{} (vehicle of {}) moved too quickly! {},{},{}",
                entity_plain_name(&*vehicle),
                self.player.get_plain_text_name(),
                java_double(x_dist),
                java_double(y_dist),
                java_double(z_dist)
            );
            self.send(&move_vehicle_packet(&*vehicle));
            return;
        }

        let old_aabb = vehicle.base().bounding_box();
        x_dist = target_x - lx;
        y_dist = target_y - ly;
        z_dist = target_z - lz;
        let vehicle_rests_on_something = vehicle.base().vertical_collision_below;
        if self.sys().vehicle_on_climbable(self, &*vehicle) {
            vehicle.reset_fall_distance();
        }

        vehicle.r#move(
            &mut *self.level,
            MoverType::Player,
            Vec3::new(x_dist, y_dist, z_dist),
        );
        let now_pos = vehicle.position();
        x_dist = target_x - now_pos.x;
        // vanilla: `if (yDist > -0.5 || yDist < 0.5)` is always true
        y_dist = 0.0;
        z_dist = target_z - now_pos.z;
        moved_dist = x_dist * x_dist + y_dist * y_dist + z_dist * z_dist;
        let mut fail = false;
        if moved_dist > 0.0625 {
            fail = true;
            log::warn!(
                "{} (vehicle of {}) moved wrongly! {}",
                entity_plain_name(&*vehicle),
                self.player.get_plain_text_name(),
                java_double(moved_dist.sqrt())
            );
        }

        if (fail && seam::no_collision(self.level, &*vehicle, &old_aabb))
            || self.is_entity_colliding_with_anything_new(
                &*vehicle, &old_aabb, target_x, target_y, target_z,
            )
        {
            vehicle.abs_snap_to(old_x, old_y, old_z, target_y_rot, target_x_rot);
            self.send(&move_vehicle_packet(&*vehicle));
            vehicle.base_mut().movement_this_tick.pop_back();
            return;
        }

        vehicle.abs_snap_to(target_x, target_y, target_z, target_y_rot, target_x_rot);
        self.chunk_source_move();
        let p = vehicle.position();
        let client_delta_movement = Vec3::new(p.x - old_x, p.y - old_y, p.z - old_z);
        self.handle_player_known_movement(client_delta_movement);
        let hc = vehicle.base().horizontal_collision;
        vehicle.set_on_ground_with_movement(
            &*self.level,
            packet.on_ground,
            hc,
            client_delta_movement,
        );
        vehicle.do_check_fall_damage(
            &mut *self.level,
            client_delta_movement.x,
            client_delta_movement.y,
            client_delta_movement.z,
            packet.on_ground,
        );
        self.sys().check_movement_statistics(
            self,
            client_delta_movement.x,
            client_delta_movement.y,
            client_delta_movement.z,
        );
        let floating = y_dist >= -0.03125
            && !vehicle_rests_on_something
            && !self.server.allow_flight()
            && !vehicle.base().is_no_gravity()
            && self.no_blocks_around(vehicle);
        let l = self.lis();
        l.client_vehicle_is_floating = floating;
        let p = vehicle.position();
        l.vehicle_last_good_x = p.x;
        l.vehicle_last_good_y = p.y;
        l.vehicle_last_good_z = p.z;
    }

    /// `noBlocksAround(entity)`.
    fn no_blocks_around(&self, entity: &dyn Entity) -> bool {
        let aabb = entity
            .base()
            .bounding_box()
            .inflate_scalar(0.0625)
            .expand_towards(0.0, -0.55, 0.0);
        seam::all_blocks_in_are_air(self.level, &aabb)
    }

    /// `handleAcceptTeleportPacket`.
    pub fn handle_accept_teleport_packet(&mut self, packet: &ServerboundAcceptTeleportationPacket) {
        if packet.id != self.lis().awaiting_teleport {
            return;
        }
        let awaiting = self.lis().awaiting_position_from_client;
        let Some(awaiting) = awaiting.filter(|_| {
            !contains_invalid_values(packet.x, packet.y, packet.z, packet.y_rot, packet.x_rot)
        }) else {
            self.disconnect(invalid_player_movement());
            return;
        };
        let (y_rot, x_rot) = (self.player.get_y_rot(), self.player.get_x_rot());
        self.player
            .abs_snap_to(awaiting.x, awaiting.y, awaiting.z, y_rot, x_rot);
        let lis = self.lis();
        lis.last_good_x = awaiting.x;
        lis.last_good_y = awaiting.y;
        lis.last_good_z = awaiting.z;
        self.player.is_changing_dimension = false;
        self.lis().awaiting_position_from_client = None;
        self.handle_player_position_change(
            packet.x,
            packet.y,
            packet.z,
            packet.y_rot,
            packet.x_rot,
            false,
            false,
        );
    }

    /// `handleAcceptPlayerLoad`.
    pub fn handle_accept_player_load(&mut self, _packet: &ServerboundPlayerLoadedPacket) {
        self.lis().mark_client_loaded();
    }

    /// `handleMovePlayer`.
    pub fn handle_move_player(&mut self, packet: &MovePlayer) {
        if contains_invalid_values(
            packet.get_x(0.0),
            packet.get_y(0.0),
            packet.get_z(0.0),
            packet.get_y_rot(0.0),
            packet.get_x_rot(0.0),
        ) {
            self.disconnect(invalid_player_movement());
            return;
        }
        if packet.has_position() {
            if self.lis().received_position_this_tick {
                self.disconnect(invalid_player_movement());
                return;
            }
            self.lis().received_position_this_tick = true;
        }

        if !self.player.won_game {
            if self.lis().tick_count == 0 {
                self.reset_position();
            }

            if self.has_client_loaded() {
                let requested_y_rot = packet.get_y_rot(self.player.get_y_rot());
                let requested_x_rot = packet.get_x_rot(self.player.get_x_rot());
                if self.update_awaiting_teleport() {
                    self.player.abs_snap_rotation_to(
                        mth::wrap_degrees_f32(requested_y_rot),
                        mth::wrap_degrees_f32(requested_x_rot),
                    );
                } else {
                    let pos = self.player.position();
                    self.handle_player_position_change(
                        packet.get_x(pos.x),
                        packet.get_y(pos.y),
                        packet.get_z(pos.z),
                        requested_y_rot,
                        requested_x_rot,
                        packet.on_ground,
                        packet.horizontal_collision,
                    );
                }
            }
        }
    }

    /// `handlePlayerPositionChange`.
    pub(super) fn handle_player_position_change(
        &mut self,
        requested_x: f64,
        requested_y: f64,
        requested_z: f64,
        requested_y_rot: f32,
        requested_x_rot: f32,
        is_on_ground: bool,
        horizontal_collision: bool,
    ) {
        let target_x = clamp_horizontal(requested_x);
        let target_y = clamp_vertical(requested_y);
        let target_z = clamp_horizontal(requested_z);
        let target_y_rot = mth::wrap_degrees_f32(requested_y_rot);
        let target_x_rot = mth::wrap_degrees_f32(requested_x_rot);
        if self.player.is_passenger() {
            let p = self.player.position();
            self.player
                .abs_snap_to(p.x, p.y, p.z, target_y_rot, target_x_rot);
            self.chunk_source_move();
            return;
        }

        let start = self.player.position();
        let (start_x, start_y, start_z) = (start.x, start.y, start.z);
        let (fx, fy, fz) = {
            let l = self.lis();
            (l.first_good_x, l.first_good_y, l.first_good_z)
        };
        let mut x_dist = target_x - fx;
        let mut y_dist = target_y - fy;
        let mut z_dist = target_z - fz;
        let expected_dist = self.player.base().delta_movement().length_sqr();
        let mut moved_dist = x_dist * x_dist + y_dist * y_dist + z_dist * z_dist;
        if self.player.is_sleeping() {
            if moved_dist > 1.0 {
                let (yr, xr) = (target_y_rot, target_x_rot);
                self.teleport(start_x, start_y, start_z, yr, xr);
            }
            return;
        }

        let is_fall_flying = self.player.is_fall_flying();
        if self.runs_normally() {
            self.lis().received_move_packet_count += 1;
            let mut delta_packets =
                self.lis().received_move_packet_count - self.lis().known_move_packet_count;
            if delta_packets > 5 {
                log::debug!(
                    "{} is sending move packets too frequently ({} packets since last tick)",
                    self.player.get_plain_text_name(),
                    delta_packets
                );
                delta_packets = 1;
            }

            if self.should_check_player_movement(is_fall_flying) {
                let meters_per_tick: f32 = if is_fall_flying { 300.0 } else { 100.0 };
                // `float * int` is float arithmetic, compared against a double
                if moved_dist - expected_dist > f64::from(meters_per_tick * delta_packets as f32) {
                    log::warn!(
                        "{} moved too quickly! {},{},{}",
                        self.player.get_plain_text_name(),
                        java_double(x_dist),
                        java_double(y_dist),
                        java_double(z_dist)
                    );
                    let (yr, xr) = (self.player.get_y_rot(), self.player.get_x_rot());
                    self.teleport(start_x, start_y, start_z, yr, xr);
                    return;
                }
            }
        }

        let old_aabb = self.player.base().bounding_box();
        let (lx, ly, lz) = {
            let l = self.lis();
            (l.last_good_x, l.last_good_y, l.last_good_z)
        };
        x_dist = target_x - lx;
        y_dist = target_y - ly;
        z_dist = target_z - lz;
        let moved_upwards = y_dist > 0.0;
        if self.player.base().on_ground() && !is_on_ground && moved_upwards {
            self.sys().jump_from_ground(self);
        }

        let player_stands_on_something = self.player.base().vertical_collision_below;
        if is_on_ground && !player_stands_on_something {
            self.force_send_player_support_blocks();
        }

        self.player.r#move(
            &mut *self.level,
            MoverType::Player,
            Vec3::new(x_dist, y_dist, z_dist),
        );
        let now_pos = self.player.position();
        x_dist = target_x - now_pos.x;
        // vanilla: `if (yDist > -0.5 || yDist < 0.5)` is always true
        y_dist = 0.0;
        z_dist = target_z - now_pos.z;
        moved_dist = x_dist * x_dist + y_dist * y_dist + z_dist * z_dist;
        let mut fail = false;
        if !self.player.is_changing_dimension()
            && moved_dist > 0.0625
            && !self.player.is_sleeping()
            && !self.player.is_creative()
            && !self.player.is_spectator()
            && !self.player.is_in_post_impulse_grace_time()
        {
            fail = true;
            log::warn!("{} moved wrongly!", self.player.get_plain_text_name());
        }

        let accepted = self.player.base().no_physics
            || self.player.is_sleeping()
            || (!fail || !seam::no_collision(self.level, &*self.player, &old_aabb))
                && !self.is_entity_colliding_with_anything_new(
                    &*self.player,
                    &old_aabb,
                    target_x,
                    target_y,
                    target_z,
                );
        if accepted {
            self.player
                .abs_snap_to(target_x, target_y, target_z, target_y_rot, target_x_rot);
            let is_auto_spin_attack = self.player.is_auto_spin_attack();
            let floating = y_dist >= -0.03125
                && !player_stands_on_something
                && !self.player.is_spectator()
                && !self.server.allow_flight()
                && !self.player.player.abilities.mayfly
                && !self.player.has_levitation()
                && !is_fall_flying
                && !is_auto_spin_attack
                && self.no_blocks_around(&*self.player);
            self.lis().client_is_floating = floating;
            self.chunk_source_move();
            let p = self.player.position();
            let client_delta_movement = Vec3::new(p.x - start_x, p.y - start_y, p.z - start_z);
            self.player.set_on_ground_with_movement(
                &*self.level,
                is_on_ground,
                horizontal_collision,
                client_delta_movement,
            );
            self.player.do_check_fall_damage(
                &mut *self.level,
                client_delta_movement.x,
                client_delta_movement.y,
                client_delta_movement.z,
                is_on_ground,
            );
            self.handle_player_known_movement(client_delta_movement);
            if moved_upwards {
                self.player.reset_fall_distance();
            }

            if is_on_ground
                || self.player.has_landed_in_liquid()
                || self.sys().on_climbable(self)
                || self.player.is_spectator()
                || is_fall_flying
                || is_auto_spin_attack
            {
                self.player.try_reset_current_impulse_context();
            }

            let p = self.player.position();
            self.sys()
                .check_movement_statistics(self, p.x - start_x, p.y - start_y, p.z - start_z);
            let l = self.lis();
            l.last_good_x = p.x;
            l.last_good_y = p.y;
            l.last_good_z = p.z;
        } else {
            self.teleport(start_x, start_y, start_z, target_y_rot, target_x_rot);
            let p = self.player.position();
            self.player.do_check_fall_damage(
                &mut *self.level,
                p.x - start_x,
                p.y - start_y,
                p.z - start_z,
                is_on_ground,
            );
            self.player
                .player
                .living
                .entity
                .movement_this_tick
                .pop_back();
        }
    }

    /// `shouldCheckPlayerMovement`.
    fn should_check_player_movement(&mut self, is_fall_flying: bool) -> bool {
        if self.is_singleplayer_owner() || self.player.is_changing_dimension() {
            return false;
        }
        use crate::world::level::gamerules::GameRules;
        self.server.game_rule(GameRules::PLAYER_MOVEMENT_CHECK)
            && (!is_fall_flying || self.server.game_rule(GameRules::ELYTRA_MOVEMENT_CHECK))
    }

    /// `forceSendPlayerSupportBlocks`.
    fn force_send_player_support_blocks(&mut self) {
        let tick_count = self.lis().tick_count;
        if tick_count < self.lis().last_supported_blocks_send + 200 {
            return;
        }
        self.lis().last_supported_blocks_send = tick_count;
        log::info!(
            "Player {} standing on air - force-sending blocks below",
            self.player.get_plain_text_name()
        );
        let aabb = self.player.base().bounding_box();
        let just_below = aabb.min_y - f64::from(1.0E-5f32);
        let mut positions: Vec<BlockPos> = Vec::with_capacity(4);
        for (x, z) in [
            (aabb.min_x, aabb.min_z),
            (aabb.max_x, aabb.min_z),
            (aabb.min_x, aabb.max_z),
            (aabb.max_x, aabb.max_z),
        ] {
            let pos = BlockPos::containing(x, just_below, z);
            if !positions.contains(&pos) {
                positions.push(pos);
            }
        }
        for pos in positions {
            let state = self.level.get_block_state(pos);
            self.send(&ClientboundBlockUpdatePacket {
                pos,
                block_state: state,
            });
            if let Some(packet) = self
                .level
                .get_block_entity(pos)
                .and_then(|be| be.get_update_packet())
            {
                self.send(&packet);
            }
        }
    }

    /// `updateAwaitingTeleport`.
    pub(super) fn update_awaiting_teleport(&mut self) -> bool {
        let awaiting = self.lis().awaiting_position_from_client;
        match awaiting {
            Some(pos) => {
                let tick_count = self.lis().tick_count;
                if tick_count - self.lis().awaiting_teleport_time > 20 {
                    self.lis().awaiting_teleport_time = tick_count;
                    let (yr, xr) = (self.player.get_y_rot(), self.player.get_x_rot());
                    self.teleport(pos.x, pos.y, pos.z, yr, xr);
                }
                true
            }
            None => {
                let tick_count = self.lis().tick_count;
                self.lis().awaiting_teleport_time = tick_count;
                false
            }
        }
    }

    /// `isEntityCollidingWithAnythingNew`.
    fn is_entity_colliding_with_anything_new(
        &self,
        entity: &dyn Entity,
        old_aabb: &AABB,
        new_x: f64,
        new_y: f64,
        new_z: f64,
    ) -> bool {
        let pos = entity.position();
        let new_aabb =
            entity
                .base()
                .bounding_box()
                .move_(new_x - pos.x, new_y - pos.y, new_z - pos.z);
        let new_collisions = seam::get_pre_move_collisions(
            self.level,
            entity,
            &new_aabb.deflate_scalar(f64::from(1.0E-5f32)),
            old_aabb.get_bottom_center(),
        );
        let old_shape = Shapes::create(old_aabb.deflate_scalar(f64::from(1.0E-5f32)));
        for shape in new_collisions {
            if !Shapes::join_is_not_empty(&shape, &old_shape, BooleanOp::AND) {
                return true;
            }
        }
        false
    }

    /// `teleport(x, y, z, yRot, xRot)`.
    pub fn teleport(&mut self, x: f64, y: f64, z: f64, y_rot: f32, x_rot: f32) {
        self.player.connection_teleport(x, y, z, y_rot, x_rot);
    }

    /// `handlePlayerKnownMovement`.
    fn handle_player_known_movement(&mut self, movement: Vec3) {
        if movement.length_sqr() > f64::from(1.0E-5f32) {
            self.player.reset_last_action_time();
        }
        self.player.set_known_movement(movement);
        self.lis().received_movement_this_tick = true;
    }

    /// `handleClientTickEnd`.
    pub fn handle_client_tick_end(
        &mut self,
        _packet: &mc_protocol::packets::play::ServerboundClientTickEndPacket,
    ) {
        if !self.lis().received_movement_this_tick {
            self.player.set_known_movement(Vec3::ZERO);
        }
        let l = self.lis();
        l.received_movement_this_tick = false;
        l.received_position_this_tick = false;
    }
}

/// Java `Double.toString` (what `{}` formats in vanilla log lines).
fn java_double(v: f64) -> String {
    mc_core::math::java::java_double_to_string(v)
}

/// `Entity.getPlainTextName()`: the custom name, else the entity type's name.
pub(super) fn entity_plain_name(entity: &dyn Entity) -> String {
    match entity.base().get_custom_name() {
        Some(name) => name.get_string(),
        None => {
            Component::translatable(entity.get_type().data().description_id, vec![]).get_string()
        }
    }
}

impl PlayCtx<'_> {
    /// `player.getRootVehicle()`: the id of the outermost vehicle (the player itself when not riding).
    fn root_vehicle(&self) -> Option<crate::world::entity::EntityId> {
        let mut id = self.player.id();
        let mut next = self.player.player.living.entity.vehicle;
        while let Some(v) = next {
            id = v;
            next = self.level.get_entity(v).and_then(|e| e.base().vehicle);
        }
        Some(id)
    }

    /// `entity.getControllingPassenger()`.
    fn controlling_passenger(
        &self,
        vehicle: crate::world::entity::EntityId,
    ) -> Option<crate::world::entity::EntityId> {
        self.level
            .get_entity(vehicle)
            .and_then(|e| e.get_controlling_passenger())
    }
}

impl crate::world::entity::ServerPlayer {
    /// `LivingEntity.hasLandedInLiquid()`.
    pub(super) fn has_landed_in_liquid(&self) -> bool {
        let e = &self.player.living.entity;
        e.delta_movement().y < f64::from(1.0E-5f32)
            && (e.was_touching_water || (!e.first_tick && e.fluid_height[1] > 0.0))
    }

    /// `isInPostImpulseGraceTime()`.
    pub(super) fn is_in_post_impulse_grace_time(&self) -> bool {
        self.player.living.current_impulse_context_reset_grace_time > 0
    }

    /// `tryResetCurrentImpulseContext()`.
    pub(super) fn try_reset_current_impulse_context(&mut self) {
        if self.player.living.current_impulse_context_reset_grace_time == 0 {
            self.player.living.current_impulse_impact_pos = None;
        }
    }

    /// `isAutoSpinAttack()`.
    pub(super) fn is_auto_spin_attack(&self) -> bool {
        self.player.living.get_living_entity_flag(4)
    }

    /// `hasEffect(MobEffects.LEVITATION)`.
    pub(super) fn has_levitation(&self) -> bool {
        self.player
            .living
            .active_effects
            .contains_key(&mc_data::MobEffect::LEVITATION)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_values_match_vanilla() {
        assert!(contains_invalid_values(f64::NAN, 0.0, 0.0, 0.0, 0.0));
        assert!(contains_invalid_values(0.0, 0.0, 0.0, f32::INFINITY, 0.0));
        assert!(contains_invalid_values(0.0, 0.0, 0.0, 0.0, f32::NAN));
        // infinite coordinates are not "invalid": they are clamped
        assert!(!contains_invalid_values(f64::INFINITY, 0.0, 0.0, 0.0, 0.0));
        assert_eq!(clamp_horizontal(f64::INFINITY), 3.0E7);
        assert_eq!(clamp_vertical(-1.0E9), -2.0E7);
    }
}
