//! `ServerLevel.sendParticles(...)`: a `ClientboundLevelParticlesPacket` to every player of this level close enough.

use super::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::player::ServerPlayer;
use mc_core::phys::Vec3;
use mc_protocol::packets::play::{ClientboundLevelParticlesPacket, ParticleOptions, RandomizationType};

impl ServerLevel {
    /// `sendParticles(particle, x, y, z, count, xDist, yDist, zDist, speed)`: returns how many players got the packet.
    #[allow(clippy::too_many_arguments)]
    pub fn send_particles(&self, particle: ParticleOptions, x: f64, y: f64, z: f64, count: i32, x_dist: f64, y_dist: f64, z_dist: f64, speed: f64) -> i32 {
        self.send_particles_full(particle, false, false, x, y, z, count, x_dist, y_dist, z_dist, speed, speed, speed, RandomizationType::Default)
    }

    /// The general `sendParticles(particle, overrideLimiter, alwaysShow, x, y, z, count, xDist, yDist, zDist, xSpeed, ySpeed, zSpeed, randomizationType)`.
    #[allow(clippy::too_many_arguments)]
    pub fn send_particles_full(
        &self,
        particle: ParticleOptions,
        override_limiter: bool,
        always_show: bool,
        x: f64,
        y: f64,
        z: f64,
        count: i32,
        x_dist: f64,
        y_dist: f64,
        z_dist: f64,
        x_speed: f64,
        y_speed: f64,
        z_speed: f64,
        randomization_type: RandomizationType,
    ) -> i32 {
        let packet = ClientboundLevelParticlesPacket {
            particle,
            override_limiter,
            always_show,
            x,
            y,
            z,
            x_dist: x_dist as f32,
            y_dist: y_dist as f32,
            z_dist: z_dist as f32,
            x_max_speed: x_speed as f32,
            y_max_speed: y_speed as f32,
            z_max_speed: z_speed as f32,
            count,
            randomization_type,
        };
        let mut result = 0;
        for &id in &self.players {
            let Some(entity) = self.get_entity(id) else { continue };
            let Some(player) = entity.as_any().downcast_ref::<ServerPlayer>() else { continue };
            if self.send_particles_to(player, override_limiter, x, y, z, &packet) {
                result += 1;
            }
        }
        result
    }

    /// `sendParticles(player, overrideLimiter, x, y, z, packet)`.
    fn send_particles_to(&self, player: &ServerPlayer, override_limiter: bool, x: f64, y: f64, z: f64, packet: &ClientboundLevelParticlesPacket) -> bool {
        let pos = player.base().block_position();
        if pos.closer_to_center_than(&Vec3::new(x, y, z), if override_limiter { 512.0 } else { 32.0 }) {
            player.send(packet);
            true
        } else {
            false
        }
    }
}
