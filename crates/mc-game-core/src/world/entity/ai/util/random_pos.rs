//! `RandomPos`.

use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use mc_core::direction::Direction;
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;

const RANDOM_POS_ATTEMPTS: i32 = 10;

/// A `Supplier<BlockPos>` that may use the mob's RNG and the level.
pub type PosSupplier<'a> = &'a mut dyn FnMut(&mut dyn Entity, &mut ServerLevel) -> Option<BlockPos>;

/// `generateRandomDirection(random, horizontalDist, verticalDist)`.
pub fn generate_random_direction(random: &mut dyn RandomSource, horizontal_dist: i32, vertical_dist: i32) -> BlockPos {
    let xt = random.next_int_bounded(2 * horizontal_dist + 1) - horizontal_dist;
    let yt = random.next_int_bounded(2 * vertical_dist + 1) - vertical_dist;
    let zt = random.next_int_bounded(2 * horizontal_dist + 1) - horizontal_dist;
    BlockPos::new(xt, yt, zt)
}

/// `generateRandomDirectionWithinRadians(...)`.
#[allow(clippy::too_many_arguments)]
pub fn generate_random_direction_within_radians(
    random: &mut dyn RandomSource,
    min_horizontal_dist: f64,
    max_horizontal_dist: f64,
    vertical_dist: i32,
    flying_height: i32,
    x_dir: f64,
    z_dir: f64,
    max_xz_radians_from_dir: f64,
) -> Option<BlockPos> {
    let y_radians_center = mth::atan2(z_dir, x_dir) - f64::from((std::f64::consts::PI / 2.0) as f32);
    let y_radians = y_radians_center + f64::from(2.0f32 * random.next_float() - 1.0f32) * max_xz_radians_from_dir;
    let dist = mth::lerp_f64(random.next_double().sqrt(), min_horizontal_dist, max_horizontal_dist) * f64::from(mth::SQRT_OF_TWO);
    let xt = -dist * y_radians.sin();
    let zt = dist * y_radians.cos();
    if !(xt.abs() > max_horizontal_dist) && !(zt.abs() > max_horizontal_dist) {
        let yt = random.next_int_bounded(2 * vertical_dist + 1) - vertical_dist + flying_height;
        Some(BlockPos::containing(xt, f64::from(yt), zt))
    } else {
        None
    }
}

/// `moveUpOutOfSolid(pos, maxY, solidityTester)`.
pub fn move_up_out_of_solid(pos: BlockPos, max_y: i32, mut solidity_tester: impl FnMut(BlockPos) -> bool) -> BlockPos {
    if !solidity_tester(pos) {
        return pos;
    }
    let mut on_ground_pos = pos.relative(Direction::Up);
    while on_ground_pos.y() <= max_y && solidity_tester(on_ground_pos) {
        on_ground_pos = on_ground_pos.relative(Direction::Up);
    }
    on_ground_pos
}

/// `moveUpToAboveSolid(pos, aboveSolidAmount, maxY, solidityTester)`.
pub fn move_up_to_above_solid(pos: BlockPos, above_solid_amount: i32, max_y: i32, mut solidity_tester: impl FnMut(BlockPos) -> bool) -> BlockPos {
    assert!(above_solid_amount >= 0, "aboveSolidAmount was {above_solid_amount}, expected >= 0");
    if !solidity_tester(pos) {
        return pos;
    }
    let mut p = pos.relative(Direction::Up);
    while p.y() <= max_y && solidity_tester(p) {
        p = p.relative(Direction::Up);
    }
    let first_non_solid_y = p.y();
    while p.y() <= max_y && p.y() - first_non_solid_y < above_solid_amount {
        p = p.relative(Direction::Up);
        if solidity_tester(p) {
            p = p.relative(Direction::Down);
            break;
        }
    }
    p
}

/// `generateRandomPos(mob, posSupplier)`: weights with `mob.getWalkTargetValue`.
pub fn generate_random_pos(mob: &mut dyn Entity, level: &mut ServerLevel, supplier: PosSupplier<'_>) -> Option<Vec3> {
    generate_random_pos_weighted(mob, level, supplier, &mut |mob, level, pos| f64::from(mob.get_walk_target_value(pos, &*level)))
}

/// `generateRandomPos(posSupplier, positionWeightFunction)`.
pub fn generate_random_pos_weighted(
    mob: &mut dyn Entity,
    level: &mut ServerLevel,
    supplier: PosSupplier<'_>,
    weight: &mut dyn FnMut(&dyn Entity, &ServerLevel, BlockPos) -> f64,
) -> Option<Vec3> {
    let mut best_weight = f64::NEG_INFINITY;
    let mut best_pos: Option<BlockPos> = None;
    for _ in 0..RANDOM_POS_ATTEMPTS {
        if let Some(pos) = supplier(mob, level) {
            let value = weight(mob, level, pos);
            if value > best_weight {
                best_weight = value;
                best_pos = Some(pos);
            }
        }
    }
    best_pos.map(|p| Vec3::at_bottom_center_of(&p))
}

/// `generateRandomPosTowardDirection(mob, xzDist, random, direction)`.
pub fn generate_random_pos_toward_direction(mob: &mut dyn Entity, xz_dist: f64, direction: BlockPos) -> BlockPos {
    let mut xt = f64::from(direction.x());
    let mut zt = f64::from(direction.z());
    let (mx, mz) = (mob.base().x(), mob.base().z());
    let home = mob.mob().filter(|m| m.has_home()).map(|m| m.home_position);
    if let (Some(center), true) = (home, xz_dist > 1.0) {
        let random = &mut mob.base_mut().random;
        if mx > f64::from(center.x()) {
            xt -= random.next_double() * xz_dist / 2.0;
        } else {
            xt += random.next_double() * xz_dist / 2.0;
        }
        if mz > f64::from(center.z()) {
            zt -= random.next_double() * xz_dist / 2.0;
        } else {
            zt += random.next_double() * xz_dist / 2.0;
        }
    }
    BlockPos::containing(xt + mx, f64::from(direction.y()) + mob.base().y(), zt + mz)
}
