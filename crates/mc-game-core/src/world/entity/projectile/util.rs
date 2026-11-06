//! `net.minecraft.world.entity.projectile.ProjectileUtil` (hit results). `getWeaponHoldingHand` / `getMobArrow` belong
//! to the arrow classes.

use super::{EntityHitResult, HitResult, RAD_TO_DEG};
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::movement::collision_context;
use crate::world::level::collision::{
    BlockGetterExt, BlockHitResultExt, ClipBlock, ClipContext, ClipFluid,
};
use mc_core::direction::Direction;
use mc_core::phys::{AABB, BlockHitResult, Vec3};
use mc_core::pos::BlockPos;
use mc_protocol::components::AttackRange;

pub const DEFAULT_ENTITY_HIT_RESULT_MARGIN: f32 = 0.3;

/// `CollisionGetter.clipIncludingBorder(c)`.
pub fn clip_including_border(level: &ServerLevel, c: &ClipContext) -> BlockHitResult {
    let hit = level.clip(c);
    if let Some(border) = level.world_border_ref()
        && border.is_within_bounds_vec3(&c.get_from())
        && !border.is_within_bounds_vec3(&hit.location)
    {
        let delta = hit.location.subtract(c.get_from());
        let delta_direction = Direction::get_approximate_nearest(delta.x, delta.y, delta.z);
        let clamped = border.clamp_vec3_to_bound(hit.location.x, hit.location.y, hit.location.z);
        return BlockHitResult {
            location: clamped,
            direction: delta_direction,
            block_pos: BlockPos::containing_position(&clamped),
            miss: false,
            inside: false,
            world_border_hit: true,
        };
    }
    hit
}

/// `getHitResultOnMoveVector(source, matching)`.
pub fn get_hit_result_on_move_vector(
    level: &ServerLevel,
    source: &dyn Entity,
    matching: &dyn Fn(&dyn Entity) -> bool,
) -> HitResult {
    get_hit_result_on_move_vector_clip(level, source, matching, ClipBlock::Collider)
}

/// `getHitResultOnMoveVector(source, matching, clipType)`.
pub fn get_hit_result_on_move_vector_clip(
    level: &ServerLevel,
    source: &dyn Entity,
    matching: &dyn Fn(&dyn Entity) -> bool,
    clip_type: ClipBlock,
) -> HitResult {
    let movement = source.base().delta_movement();
    let from = source.position();
    get_hit_result(
        level,
        from,
        source,
        matching,
        movement,
        compute_margin(source),
        clip_type,
    )
}

/// `getHitResultOnViewVector(source, matching, distance)`.
pub fn get_hit_result_on_view_vector(
    level: &ServerLevel,
    source: &dyn Entity,
    matching: &dyn Fn(&dyn Entity) -> bool,
    distance: f64,
) -> HitResult {
    let view_vector = source.get_view_vector(0.0).scale(distance);
    let from = source.base().get_eye_position();
    get_hit_result(
        level,
        from,
        source,
        matching,
        view_vector,
        0.0,
        ClipBlock::Collider,
    )
}

fn get_hit_result(
    level: &ServerLevel,
    from: Vec3,
    source: &dyn Entity,
    matching: &dyn Fn(&dyn Entity) -> bool,
    delta: Vec3,
    entity_margin: f32,
    clip_type: ClipBlock,
) -> HitResult {
    let mut to = from.add(delta);
    let mut hit_result = HitResult::Block(clip_including_border(
        level,
        &ClipContext::new(
            from,
            to,
            clip_type,
            ClipFluid::None,
            collision_context(source),
        ),
    ));
    if hit_result.get_type() != super::HitType::Miss {
        to = hit_result.get_location();
    }
    let search = source
        .get_bounding_box()
        .expand_towards_vec3(delta)
        .inflate_scalar(1.0);
    if let Some(entity_hit) =
        get_entity_hit_result_margin(level, source, from, to, &search, matching, entity_margin)
    {
        hit_result = HitResult::Entity(entity_hit);
    }
    hit_result
}

/// `getHitEntitiesAlong(attacker, attackRange, matching, blockClipType)`: `Err(block hit)` is vanilla's `Either.left`.
pub fn get_hit_entities_along(
    level: &ServerLevel,
    attacker: &dyn Entity,
    attack_range: &AttackRange,
    matching: &dyn Fn(&dyn Entity) -> bool,
    block_clip_type: ClipBlock,
) -> Result<Vec<EntityHitResult>, BlockHitResult> {
    let look = attacker.get_head_look_angle();
    let eye_position = attacker.base().get_eye_position();
    let creative = attacker.player().map(|p| p.abilities.instabuild);
    let (min_range, max_range) = match creative {
        Some(true) => (
            attack_range.min_creative_reach,
            attack_range.max_creative_reach,
        ),
        Some(false) => (attack_range.min_reach, attack_range.max_reach),
        None => (
            attack_range.min_reach * attack_range.mob_factor,
            attack_range.max_reach * attack_range.mob_factor,
        ),
    };
    let from = eye_position.add(look.scale(f64::from(min_range)));
    let movement_component = attacker.get_known_movement(level).dot(look);
    let to = eye_position.add(look.scale(f64::from(max_range) + movement_component.max(0.0)));
    get_hit_entities_along_impl(
        level,
        attacker,
        eye_position,
        from,
        matching,
        to,
        attack_range.hitbox_margin,
        block_clip_type,
    )
}

#[allow(clippy::too_many_arguments)]
fn get_hit_entities_along_impl(
    level: &ServerLevel,
    source: &dyn Entity,
    origin: Vec3,
    from: Vec3,
    matching: &dyn Fn(&dyn Entity) -> bool,
    mut to: Vec3,
    entity_margin: f32,
    clip_type: ClipBlock,
) -> Result<Vec<EntityHitResult>, BlockHitResult> {
    let hit_result = clip_including_border(
        level,
        &ClipContext::new(
            origin,
            to,
            clip_type,
            ClipFluid::None,
            collision_context(source),
        ),
    );
    if !hit_result.miss {
        to = hit_result.location;
        if origin.distance_to_sqr(to) < origin.distance_to_sqr(from) {
            return Err(hit_result);
        }
    }
    let margin = f64::from(entity_margin);
    let search_area = AABB::of_size(from, margin, margin, margin)
        .expand_towards_vec3(to.subtract(from))
        .inflate_scalar(1.0);
    let entity_hit = get_many_entity_hit_result_margin(
        level,
        source,
        from,
        to,
        &search_area,
        matching,
        entity_margin,
        clip_type,
        true,
        true,
    );
    if entity_hit.is_empty() {
        Err(hit_result)
    } else {
        Ok(entity_hit)
    }
}

/// `getEntityHitResult(except, from, to, box, matching, maxValue)` (picking, with the `getPickRadius` inflation).
pub fn get_entity_hit_result_pick(
    level: &ServerLevel,
    except: &dyn Entity,
    from: Vec3,
    to: Vec3,
    bounds: &AABB,
    matching: &dyn Fn(&dyn Entity) -> bool,
    max_value: f64,
) -> Option<EntityHitResult> {
    let mut nearest = max_value;
    let mut hovered = None;
    let except_root = except.get_root_vehicle(level);
    for id in level.get_entities(Some(except.id()), bounds, |e| matching(e)) {
        let Some(entity) = level.get_entity(id) else {
            continue;
        };
        let bb = entity
            .get_bounding_box()
            .inflate_scalar(f64::from(entity.get_pick_radius()));
        let clip_point = bb.clip(from, to);
        if bb.contains_vec3(from) {
            if nearest >= 0.0 && entity.can_be_picked_from_inside() {
                hovered = Some(EntityHitResult {
                    entity: id,
                    location: clip_point.unwrap_or(from),
                });
                nearest = 0.0;
            }
        } else if let Some(location) = clip_point {
            let dd = from.distance_to_sqr(location);
            if dd < nearest || nearest == 0.0 {
                if entity.get_root_vehicle(level) == except_root {
                    if nearest == 0.0 {
                        hovered = Some(EntityHitResult {
                            entity: id,
                            location,
                        });
                    }
                } else {
                    hovered = Some(EntityHitResult {
                        entity: id,
                        location,
                    });
                    nearest = dd;
                }
            }
        }
    }
    hovered
}

/// `getEntityHitResult(level, projectile, from, to, targetSearchArea, matching)`.
pub fn get_entity_hit_result(
    level: &ServerLevel,
    source: &dyn Entity,
    from: Vec3,
    to: Vec3,
    target_search_area: &AABB,
    matching: &dyn Fn(&dyn Entity) -> bool,
) -> Option<EntityHitResult> {
    get_entity_hit_result_margin(
        level,
        source,
        from,
        to,
        target_search_area,
        matching,
        compute_margin(source),
    )
}

/// `computeMargin(source)`.
pub fn compute_margin(source: &dyn Entity) -> f32 {
    0.0f32.max(0.3f32.min((source.base().tick_count - 2) as f32 / 20.0))
}

/// `getEntityHitResult(level, source, from, to, targetSearchArea, matching, entityMargin)`.
pub fn get_entity_hit_result_margin(
    level: &ServerLevel,
    source: &dyn Entity,
    from: Vec3,
    to: Vec3,
    target_search_area: &AABB,
    matching: &dyn Fn(&dyn Entity) -> bool,
    entity_margin: f32,
) -> Option<EntityHitResult> {
    let mut nearest = f64::MAX;
    let mut hit = None;
    for id in level.get_entities(Some(source.id()), target_search_area, |e| matching(e)) {
        let Some(entity) = level.get_entity(id) else {
            continue;
        };
        let bb = entity
            .get_bounding_box()
            .inflate_scalar(f64::from(entity_margin));
        if let Some(location) = bb.clip(from, to) {
            let dd = from.distance_to_sqr(location);
            if dd < nearest {
                hit = Some(EntityHitResult {
                    entity: id,
                    location,
                });
                nearest = dd;
            }
        }
    }
    hit
}

/// `getManyEntityHitResult(level, source, from, to, targetSearchArea, matching, includeFromEntity, projectSurfaceHitLocation)`.
pub fn get_many_entity_hit_result(
    level: &ServerLevel,
    source: &dyn Entity,
    from: Vec3,
    to: Vec3,
    target_search_area: &AABB,
    matching: &dyn Fn(&dyn Entity) -> bool,
    include_from_entity: bool,
    project_surface_hit_location: bool,
) -> Vec<EntityHitResult> {
    get_many_entity_hit_result_margin(
        level,
        source,
        from,
        to,
        target_search_area,
        matching,
        compute_margin(source),
        ClipBlock::Collider,
        include_from_entity,
        project_surface_hit_location,
    )
}

/// `getManyEntityHitResult(.., entityMargin, clipType, includeFromEntity, projectSurfaceHitLocation)`.
#[allow(clippy::too_many_arguments)]
pub fn get_many_entity_hit_result_margin(
    level: &ServerLevel,
    source: &dyn Entity,
    from: Vec3,
    to: Vec3,
    target_search_area: &AABB,
    matching: &dyn Fn(&dyn Entity) -> bool,
    entity_margin: f32,
    clip_type: ClipBlock,
    include_from_entity: bool,
    project_surface_hit_location: bool,
) -> Vec<EntityHitResult> {
    let mut collector = Vec::new();
    for id in level.get_entities(Some(source.id()), target_search_area, |e| matching(e)) {
        let Some(entity) = level.get_entity(id) else {
            continue;
        };
        let entity_bb = entity.get_bounding_box();
        if include_from_entity && entity_bb.contains_vec3(from) {
            collector.push(EntityHitResult {
                entity: id,
                location: from,
            });
        } else if let Some(exact_hit) = entity_bb.clip(from, to) {
            collector.push(EntityHitResult {
                entity: id,
                location: exact_hit,
            });
        } else if !(entity_margin <= 0.0)
            && let Some(outside_hit_position) = entity_bb
                .inflate_scalar(f64::from(entity_margin))
                .clip(from, to)
        {
            let mut towards_target = entity_bb.get_center();
            let hit_result = clip_including_border(
                level,
                &ClipContext::new(
                    outside_hit_position,
                    towards_target,
                    clip_type,
                    ClipFluid::None,
                    collision_context(source),
                ),
            );
            if !hit_result.miss {
                towards_target = hit_result.location;
            }
            if let Some(surface_hit) = entity
                .get_bounding_box()
                .clip(outside_hit_position, towards_target)
            {
                collector.push(EntityHitResult {
                    entity: id,
                    location: if project_surface_hit_location {
                        surface_hit
                    } else {
                        outside_hit_position
                    },
                });
            }
        }
    }
    collector
}

/// `rotateTowardsMovement(projectile, rotationSpeed)`.
pub fn rotate_towards_movement(projectile: &mut dyn Entity, rotation_speed: f32) {
    use mc_core::math::mth;
    let b = projectile.base_mut();
    let movement = b.delta_movement();
    if movement.length_sqr() != 0.0 {
        let sd = movement.horizontal_distance();
        b.set_y_rot((mth::atan2(movement.z, movement.x) * RAD_TO_DEG) as f32 + 90.0);
        b.set_x_rot((mth::atan2(sd, movement.y) * RAD_TO_DEG) as f32 - 90.0);
        while b.x_rot() - b.x_rot_o < -180.0 {
            b.x_rot_o -= 360.0;
        }
        while b.x_rot() - b.x_rot_o >= 180.0 {
            b.x_rot_o += 360.0;
        }
        while b.y_rot() - b.y_rot_o < -180.0 {
            b.y_rot_o -= 360.0;
        }
        while b.y_rot() - b.y_rot_o >= 180.0 {
            b.y_rot_o += 360.0;
        }
        let x = mth::lerp_f32(rotation_speed, b.x_rot_o, b.x_rot());
        let y = mth::lerp_f32(rotation_speed, b.y_rot_o, b.y_rot());
        b.set_x_rot(x);
        b.set_y_rot(y);
    }
}

#[allow(unused_imports)]
use BlockHitResultExt as _;
