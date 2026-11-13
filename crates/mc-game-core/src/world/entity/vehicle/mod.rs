//! `net.minecraft.world.entity.vehicle`: `VehicleEntity` (hurt/damage wobble, destroy), `ContainerEntity` (chest vehicles), the
//! boats and the minecarts. Data inheritance by composition like the rest of the entity layer: a vehicle struct embeds its `EntityData`,
//! defines [`vehicle_entity::DATA_ID_HURT`] & co. through [`vehicle_entity::define_synched_data`] and calls the
//! `vehicle_entity::*` free functions from its `Entity` overrides.

pub mod boat;
pub mod container_entity;
pub mod minecart;
pub mod vehicle_entity;

use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::movement::{EntityCollisionQuery, MovementLevel, collision_context};
use crate::world::level::collision::CollisionGetter;
use mc_core::math::mth;
use mc_core::phys::shapes::{BooleanOp, Shapes};
use mc_core::phys::{AABB, Vec3};
use mc_data::EntityType;

/// `Level.noCollision(entity, aabb)`: block, entity and world-border collisions (the entity is excluded from the lookups).
pub fn no_collision(level: &ServerLevel, entity: &dyn Entity, aabb: &AABB) -> bool {
    let context = collision_context(entity);
    level.no_block_collision(Some(&context), aabb)
        && level.entity_collisions(Some(entity), aabb).is_empty()
        && level
            .world_border_collision(entity, aabb)
            .is_none_or(|border| {
                !Shapes::join_is_not_empty(&border, &Shapes::create(*aabb), BooleanOp::AND)
            })
}

/// `Entity.getCollisionHorizontalEscapeVector(colliderWidth, collidingWidth, directionDegrees)`.
pub fn get_collision_horizontal_escape_vector(
    collider_width: f64,
    colliding_width: f64,
    direction_degrees: f32,
) -> Vec3 {
    let distance = (collider_width + colliding_width + f64::from(1.0E-5f32)) / 2.0;
    let radians = direction_degrees * (std::f64::consts::PI / 180.0) as f32;
    let direction_x = -mth::sin(f64::from(radians));
    let direction_z = mth::cos(f64::from(radians));
    let scale = direction_x.abs().max(direction_z.abs());
    Vec3::new(
        f64::from(direction_x) * distance / f64::from(scale),
        0.0,
        f64::from(direction_z) * distance / f64::from(scale),
    )
}

/// `instanceof Animal` by entity type: every entity type of a class below `net.minecraft.world.entity.animal.Animal`
/// (derived from the vanilla class hierarchy; `Animal` itself is not modelled as a class).
pub fn is_animal(ty: EntityType) -> bool {
    matches!(
        ty,
        EntityType::ARMADILLO
            | EntityType::AXOLOTL
            | EntityType::BEE
            | EntityType::CAMEL
            | EntityType::CAMEL_HUSK
            | EntityType::CAT
            | EntityType::CHICKEN
            | EntityType::COW
            | EntityType::DONKEY
            | EntityType::FOX
            | EntityType::FROG
            | EntityType::HAPPY_GHAST
            | EntityType::GOAT
            | EntityType::HOGLIN
            | EntityType::HORSE
            | EntityType::LLAMA
            | EntityType::MOOSHROOM
            | EntityType::MULE
            | EntityType::NAUTILUS
            | EntityType::OCELOT
            | EntityType::PANDA
            | EntityType::PARROT
            | EntityType::PIG
            | EntityType::POLAR_BEAR
            | EntityType::RABBIT
            | EntityType::SHEEP
            | EntityType::SKELETON_HORSE
            | EntityType::SNIFFER
            | EntityType::STRIDER
            | EntityType::TRADER_LLAMA
            | EntityType::TURTLE
            | EntityType::WOLF
            | EntityType::ZOMBIE_HORSE
            | EntityType::ZOMBIE_NAUTILUS
    )
}
