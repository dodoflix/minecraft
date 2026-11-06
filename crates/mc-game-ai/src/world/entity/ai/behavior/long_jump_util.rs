//! `LongJumpUtil`.

use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_data::enums::Pose;
use mc_game_core::world::entity::EntityDimensions;
use mc_game_core::world::entity::entity::Entity;
use mc_game_core::world::entity::movement::collision_context;
use mc_game_core::world::level::collision::CollisionGetter;

/// `calculateJumpVectorForAngle(body, targetPos, maxJumpVelocity, angle, checkCollision)`.
pub fn calculate_jump_vector_for_angle(
    level: &mc_game_core::server::level::ServerLevel,
    body: &dyn Entity,
    target_pos: Vec3,
    max_jump_velocity: f32,
    angle: i32,
    check_collision: bool,
) -> Option<Vec3> {
    let mob_pos = body.position();
    let direction_vector_plane = Vec3::new(target_pos.x - mob_pos.x, 0.0, target_pos.z - mob_pos.z)
        .normalize()
        .scale(0.5);
    let target_position = target_pos.subtract(direction_vector_plane);
    let direction_vector = target_position.subtract(mob_pos);
    let angrad = angle as f32 * std::f32::consts::PI / 180.0_f32;
    let xz_ang = direction_vector.z.atan2(direction_vector.x);
    let r2 = direction_vector
        .subtract_xyz(0.0, direction_vector.y, 0.0)
        .length_sqr();
    let r = r2.sqrt();
    let y = direction_vector.y;
    let g = body.get_gravity();
    let sin2ang = f64::from(2.0_f32 * angrad).sin();
    let cosangsqr = {
        let c = f64::from(angrad).cos();
        c * c
    };
    let sinangrad = f64::from(angrad).sin();
    let cosangrad = f64::from(angrad).cos();
    let sinxz_ang = xz_ang.sin();
    let cosxz_ang = xz_ang.cos();
    let v0sqr = r2 * g / (r * sin2ang - 2.0 * y * cosangsqr);
    if v0sqr < 0.0 {
        return None;
    }
    let v0 = v0sqr.sqrt();
    if v0 > f64::from(max_jump_velocity) {
        return None;
    }
    let v0r = v0 * cosangrad;
    let v0y = v0 * sinangrad;
    if check_collision {
        let samples = mth::ceil_f64(r / v0r) * 2;
        let mut ri = 0.0_f64;
        let mut previous_pos: Option<Vec3> = None;
        let mob_dimensions = body.get_dimensions(Pose::LongJumping);
        for _ in 0..samples - 1 {
            ri += r / f64::from(samples);
            let yi = sinangrad / cosangrad * ri
                - (ri * ri) * g / (2.0 * v0sqr * (cosangrad * cosangrad));
            let xi = ri * cosxz_ang;
            let zi = ri * sinxz_ang;
            let sample_pos = Vec3::new(mob_pos.x + xi, mob_pos.y + yi, mob_pos.z + zi);
            if let Some(previous) = previous_pos
                && !is_clear_transition(level, body, &mob_dimensions, previous, sample_pos)
            {
                return None;
            }
            previous_pos = Some(sample_pos);
        }
    }
    Some(Vec3::new(v0r * cosxz_ang, v0y, v0r * sinxz_ang).scale(f64::from(0.95_f32)))
}

fn is_clear_transition(
    level: &mc_game_core::server::level::ServerLevel,
    body: &dyn Entity,
    entity_dimensions: &EntityDimensions,
    position1: Vec3,
    position2: Vec3,
) -> bool {
    let direction = position2.subtract(position1);
    let min_dimension = f64::from(entity_dimensions.width.min(entity_dimensions.height));
    let checks = mth::ceil_f64(direction.length() / min_dimension);
    let normalized_direction = direction.normalize();
    let mut next_point_to_check = position1;
    for i in 0..checks {
        next_point_to_check = if i == checks - 1 {
            position2
        } else {
            next_point_to_check.add(normalized_direction.scale(min_dimension * f64::from(0.9_f32)))
        };
        if !level.no_collision(
            Some(body.id()),
            Some(&collision_context(body)),
            &entity_dimensions.make_bounding_box_vec(next_point_to_check),
        ) {
            return false;
        }
    }
    true
}
