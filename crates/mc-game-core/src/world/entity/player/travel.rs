//! `Player.travel(input)`: swimming pitch steering and the flying vertical damping around `LivingEntity.travel`.

use super::ServerPlayer;
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::living_entity::{travel, util};
use crate::world::level::BlockGetter;
use mc_core::direction::Axis;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;

impl ServerPlayer {
    /// `Player.travel(input)`.
    pub(super) fn player_travel(&mut self, level: &mut ServerLevel, input: Vec3) {
        if self.base().is_passenger() {
            travel::travel(self, level, input);
            return;
        }
        if self.is_swimming() {
            let look_angle_y = util::look_angle(self).y;
            let multiplier = if look_angle_y < -0.2 { 0.085 } else { 0.06 };
            let pos = self.base().position();
            if look_angle_y <= 0.0
                || self.player.living.jumping
                || !level
                    .get_fluid_state(BlockPos::containing(pos.x, pos.y + 1.0 - 0.1, pos.z))
                    .is_empty()
            {
                let movement = self.base().delta_movement();
                self.base_mut().set_delta_movement(movement.add_xyz(
                    0.0,
                    (look_angle_y - movement.y) * multiplier,
                    0.0,
                ));
            }
        }
        if self.player.abilities.flying {
            let original_movement_y = self.base().delta_movement().y;
            travel::travel(self, level, input);
            let dm = self.base().delta_movement();
            self.base_mut()
                .set_delta_movement(dm.with(Axis::Y, original_movement_y * 0.6));
        } else {
            travel::travel(self, level, input);
        }
    }
}
