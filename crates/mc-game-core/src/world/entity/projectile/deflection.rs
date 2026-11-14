//! `net.minecraft.world.entity.projectile.ProjectileDeflection` (the four vanilla constants, and the breeze's own: a sound
//! before the reversal).

use super::Projectile;
use mc_core::phys::Vec3;
use mc_core::random::RandomSource;
use mc_data::SoundEvent;

/// What a deflecting entity contributes (`entity.getLookAngle()` / `entity.getKnownSpeed()`), captured by the caller
/// because the projectile is mutated while the deflecting entity stays in the level.
#[derive(Clone, Copy, Debug)]
pub struct Deflector {
    pub look_angle: Vec3,
    pub known_speed: Vec3,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ProjectileDeflection {
    None,
    Reverse,
    AimDeflect,
    MomentumDeflect,
    /// `Breeze.PROJECTILE_DEFLECTION`: the deflecting entity plays the sound, then `REVERSE` runs
    /// ([`deflect_base`](super::deflect_base) plays it: this enum has no level).
    ReverseWithSound(SoundEvent),
}

impl ProjectileDeflection {
    /// `deflect(projectile, entity, random, power)` (the random is the projectile's own).
    pub fn deflect(
        self,
        projectile: &mut dyn Projectile,
        deflector: Option<&Deflector>,
        power: Vec3,
    ) {
        match self {
            ProjectileDeflection::None => {}
            ProjectileDeflection::Reverse | ProjectileDeflection::ReverseWithSound(_) => {
                let rotation = 170.0 + projectile.base_mut().random.next_float() * 20.0;
                let b = projectile.base_mut();
                b.set_delta_movement(b.delta_movement().multiply_xyz(
                    -power.x * 0.5,
                    -power.y * 0.5,
                    -power.z * 0.5,
                ));
                let y_rot = b.y_rot() + rotation;
                b.set_y_rot(y_rot);
                b.y_rot_o += rotation;
                b.needs_sync = true;
            }
            ProjectileDeflection::AimDeflect => {
                if let Some(d) = deflector {
                    let b = projectile.base_mut();
                    b.set_delta_movement(d.look_angle.multiply(power));
                    b.needs_sync = true;
                }
            }
            ProjectileDeflection::MomentumDeflect => {
                if let Some(d) = deflector {
                    let b = projectile.base_mut();
                    b.set_delta_movement(d.known_speed.normalize().multiply(power));
                    b.needs_sync = true;
                }
            }
        }
    }
}
