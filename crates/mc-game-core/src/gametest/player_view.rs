//! What `/test` reads off the command's player: position, dimension, view direction (`Entity.pick`).

use crate::commands::CommandSourceStack;
use crate::server::MinecraftServer;
use crate::world::level::DimensionKey;
use crate::world::level::collision::{BlockGetterExt, ClipBlock, ClipContext, ClipFluid, CollisionContext};
use mc_core::math::mth;
use mc_core::phys::{BlockHitResult, Vec3};

pub struct PlayerView {
    /// `player.position()`.
    pub position: Vec3,
    /// `player.level().dimension()`.
    pub dimension: DimensionKey,
    /// `player.getEyePosition()`.
    pub eye: Vec3,
    /// `player.getLookAngle()`.
    pub look: Vec3,
    pub y_rot: f32,
    pub x_rot: f32,
}

/// `Entity.calculateViewVector(xRot, yRot)`.
fn calculate_view_vector(x_rot: f32, y_rot: f32) -> Vec3 {
    let x = x_rot * (std::f32::consts::PI / 180.0);
    let y = -y_rot * (std::f32::consts::PI / 180.0);
    let (y_cos, y_sin) = (mth::cos(f64::from(y)), mth::sin(f64::from(y)));
    let (x_cos, x_sin) = (mth::cos(f64::from(x)), mth::sin(f64::from(x)));
    Vec3::new(f64::from(y_sin * x_cos), f64::from(-x_sin), f64::from(y_cos * x_cos))
}

impl PlayerView {
    /// The player of `source` (`source.getPlayer()`); the rotation comes from the live entity when it is reachable
    /// (a player is taken out of its level while its own command runs), else from the source's rotation.
    pub fn of(server: &MinecraftServer, source: &CommandSourceStack) -> Option<PlayerView> {
        let entity = source.get_player()?;
        let handle = entity.player.as_ref()?;
        let shared = handle.shared.lock().clone();
        let live = server.level(&shared.dimension).and_then(|level| level.get_entity(entity.id));
        let (y_rot, x_rot) = match live {
            Some(e) => (e.base().y_rot(), e.base().x_rot()),
            None => {
                let rotation = source.get_rotation();
                (rotation.y, rotation.x)
            }
        };
        let eye = Vec3::new(shared.position.x, shared.position.y + f64::from(entity.eye_height), shared.position.z);
        Some(PlayerView { position: shared.position, dimension: shared.dimension, eye, look: calculate_view_vector(x_rot, y_rot), y_rot, x_rot })
    }

    /// `player.pick(range, 1.0F, false)`: the block the player looks at (a miss answers the end point's block).
    pub fn pick(&self, server: &MinecraftServer, range: f64) -> Option<BlockHitResult> {
        let level = server.level(&self.dimension)?;
        let to = self.eye.add(self.look.scale(range));
        Some(level.clip(&ClipContext::new(self.eye, to, ClipBlock::Outline, ClipFluid::None, CollisionContext::empty())))
    }
}
