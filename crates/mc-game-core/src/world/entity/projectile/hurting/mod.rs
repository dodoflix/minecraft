//! `net.minecraft.world.entity.projectile.hurtingprojectile.AbstractHurtingProjectile`: projectiles that accelerate
//! along their motion (fireballs, wither skulls, wind charges). A concrete type embeds a [`HurtingProjectileData`] and a
//! `ProjectileData`, implements [`Projectile`] + [`AbstractHurtingProjectile`] and puts
//! `impl_projectile_entity!(hurting)` into its `impl Entity` (its `hurtServer` is vanilla's `return false`).

pub mod dragon_fireball;
pub mod fireball;
pub mod large_fireball;
pub mod small_fireball;
pub mod windcharge;
pub mod wither_skull;

use super::util::{get_hit_result_on_move_vector_clip, rotate_towards_movement};
use super::{HitResult, HitType, Projectile, hit_target_or_deflect_self};
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::level::LevelReader;
use crate::world::level::collision::ClipBlock;
use mc_core::phys::Vec3;
use mc_nbt::CompoundTag;

pub const INITIAL_ACCELERATION_POWER: f64 = 0.1;
pub const DEFLECTION_SCALE: f64 = 0.5;

/// The fields `AbstractHurtingProjectile` adds to `Projectile`.
pub struct HurtingProjectileData {
    /// `accelerationPower`.
    pub acceleration_power: f64,
}

impl Default for HurtingProjectileData {
    fn default() -> Self {
        Self {
            acceleration_power: INITIAL_ACCELERATION_POWER,
        }
    }
}

/// `AbstractHurtingProjectile` and its overridable hooks.
pub trait AbstractHurtingProjectile: Projectile {
    fn hurting_data(&self) -> &HurtingProjectileData;
    fn hurting_data_mut(&mut self) -> &mut HurtingProjectileData;
    fn as_hurting_mut(&mut self) -> &mut dyn AbstractHurtingProjectile;

    /// `getClipType()`.
    fn get_clip_type(&self) -> ClipBlock {
        ClipBlock::Collider
    }

    /// `shouldBurn()`.
    fn should_burn(&self) -> bool {
        true
    }

    /// `getInertia()`.
    fn get_inertia(&self) -> f32 {
        0.95
    }

    /// `getLiquidInertia()`.
    fn get_liquid_inertia(&self) -> f32 {
        0.8
    }
}

/// `AbstractHurtingProjectile(type, x, y, z, direction, level)` after the `this(type, level)` part: the position and
/// the initial motion `direction.normalize() * accelerationPower`.
pub fn init_with_direction(
    projectile: &mut dyn AbstractHurtingProjectile,
    x: f64,
    y: f64,
    z: f64,
    direction: Vec3,
) {
    let (y_rot, x_rot) = (projectile.base().y_rot(), projectile.base().x_rot());
    projectile.snap_to(x, y, z, y_rot, x_rot);
    projectile.reapply_position();
    let power = projectile.hurting_data().acceleration_power;
    assign_directional_movement(projectile, direction, power);
}

/// `assignDirectionalMovement(direction, speed)`.
pub fn assign_directional_movement(
    projectile: &mut dyn AbstractHurtingProjectile,
    direction: Vec3,
    speed: f64,
) {
    let b = projectile.base_mut();
    b.set_delta_movement(direction.normalize().scale(speed));
    b.needs_sync = true;
}

/// `applyInertia()`.
fn apply_inertia(projectile: &mut dyn AbstractHurtingProjectile) {
    let movement = projectile.base().delta_movement();
    let inertia = if projectile.is_in_water() {
        projectile.get_liquid_inertia()
    } else {
        projectile.get_inertia()
    };
    let power = projectile.hurting_data().acceleration_power;
    projectile.base_mut().set_delta_movement(
        movement
            .add(movement.normalize().scale(power))
            .scale(f64::from(inertia)),
    );
}

/// `AbstractHurtingProjectile.tick()`.
pub fn tick(projectile: &mut dyn AbstractHurtingProjectile, level: &mut ServerLevel) {
    let owner_removed = projectile.get_owner(level).is_some_and(|o| o.is_removed());
    apply_inertia(projectile);
    let pos = projectile.block_position();
    if !owner_removed && level.has_chunk(pos.x() >> 4, pos.z() >> 4) {
        let hit_result = {
            let p: &dyn AbstractHurtingProjectile = projectile;
            get_hit_result_on_move_vector_clip(
                level,
                p.as_dyn(),
                &|e| p.can_hit_entity(level, e),
                p.get_clip_type(),
            )
        };
        let new_position = if hit_result.get_type() != HitType::Miss {
            hit_result.get_location()
        } else {
            projectile
                .position()
                .add(projectile.base().delta_movement())
        };
        rotate_towards_movement(projectile, 0.2);
        projectile.set_pos_vec(new_position);
        projectile.apply_effects_from_blocks(level);
        super::tick(projectile, level);
        if projectile.should_burn() {
            projectile.ignite_for_seconds(1.0);
        }
        if hit_result.get_type() != HitType::Miss && projectile.is_alive() {
            hit_target_or_deflect_self(projectile, level, &hit_result);
        }
    } else {
        projectile.discard();
    }
}

/// `AbstractHurtingProjectile.canHitEntity(entity)`.
pub fn can_hit_entity(
    projectile: &dyn AbstractHurtingProjectile,
    level: &ServerLevel,
    entity: &dyn Entity,
) -> bool {
    default_can_hit_entity(projectile, level, entity) && !entity.base().no_physics
}

/// `Projectile.canHitEntity(entity)`.
pub fn default_can_hit_entity(
    projectile: &dyn Projectile,
    level: &ServerLevel,
    entity: &dyn Entity,
) -> bool {
    if !entity.can_be_hit_by_projectile() {
        return false;
    }
    let owner = projectile.get_owner(level);
    owner.is_none_or(|o| {
        projectile.projectile_data().left_owner || !level.is_passenger_of_same_vehicle(o, entity)
    })
}

/// `AbstractHurtingProjectile.onDeflection(byAttack)`.
pub fn on_deflection(projectile: &mut dyn AbstractHurtingProjectile, by_attack: bool) {
    let d = projectile.hurting_data_mut();
    if by_attack {
        d.acceleration_power = INITIAL_ACCELERATION_POWER;
    } else {
        d.acceleration_power *= DEFLECTION_SCALE;
    }
}

/// `AbstractHurtingProjectile.addAdditionalSaveData`.
pub fn add_additional_save_data(projectile: &dyn AbstractHurtingProjectile, out: &mut CompoundTag) {
    super::add_additional_save_data(projectile, out);
    out.put_double(
        "acceleration_power",
        projectile.hurting_data().acceleration_power,
    );
}

/// `AbstractHurtingProjectile.readAdditionalSaveData`.
pub fn read_additional_save_data(
    projectile: &mut dyn AbstractHurtingProjectile,
    input: &CompoundTag,
) {
    super::read_additional_save_data(projectile, input);
    projectile.hurting_data_mut().acceleration_power =
        input.get_double_or("acceleration_power", INITIAL_ACCELERATION_POWER);
}

/// `ProjectileUtil.getHitResultOnMoveVector` result type re-export for subclasses.
pub type Hit = HitResult;
