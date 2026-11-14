//! The level-mutating `onMobRemoved` / `onMobHurt` reactions of `WeavingMobEffect`, `InfestedMobEffect` and
//! `WindChargedMobEffect`.

use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::factory::{self, EntitySpawnRequest};
use crate::world::entity::projectile::hurting::windcharge::{
    EXPLOSION_DAMAGE_CALCULATOR, explode_with,
};
use crate::world::level::block::BlockStateExt;
use crate::world::level::level_access::{BlockGetter, LevelAccessor};
use crate::world::level::levelgen::feature::tree::java_hash_set::BlockPosHashSet;
use mc_core::direction::Direction;
use mc_core::math::{mth, rotate_y};
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::enums::EntitySpawnReason;
use mc_data::{EntityType, SoundEvent};
use mc_protocol::packets::play::SoundSource;

/// `OozingMobEffect.SLIME_SIZE`.
const OOZING_SLIME_SIZE: i32 = 2;

/// `WeavingMobEffect.onMobRemoved` (KILLED): `mob instanceof Player || mobGriefing`.
pub fn weaving(level: &mut ServerLevel, mob: &mut dyn Entity) {
    if !(mob.is_player() || level.mob_griefing()) {
        return;
    }
    let pos = mob.block_position();
    let random = &mut mob.base_mut().random;
    // `maxCobwebs = random -> Mth.randomBetweenInclusive(random, 2, 3)`
    let cobweb_count = mth::random_between_inclusive(random, 2, 3) as usize;
    let mut positions = BlockPosHashSet::new();
    for block_pos in BlockPos::random_in_cube(random, 15, &pos, 1) {
        let below = block_pos.below();
        if !positions.contains(&block_pos)
            && level.get_block_state(block_pos).can_be_replaced()
            && level
                .get_block_state(below)
                .is_face_sturdy_at(&*level, below, Direction::Up)
        {
            positions.insert(block_pos);
            if positions.len() >= cobweb_count {
                break;
            }
        }
    }
    for block_pos in positions.iter() {
        level.set_block_and_update(block_pos, mc_data::Block::COBWEB.default_state());
        level.level_event(None, 3018, block_pos, 0);
    }
}

/// `InfestedMobEffect.onMobHurt`: `chanceToSpawn = 0.1F`, `spawnedCount = random -> randomBetweenInclusive(1, 2)`.
pub fn infested(level: &mut ServerLevel, mob: &mut dyn Entity) {
    if mob.base_mut().random.next_float() > 0.1 {
        return;
    }
    let count = mth::random_between_inclusive(&mut mob.base_mut().random, 1, 2);
    let pos = mob.base().position();
    let y = pos.y + f64::from(mob.base().get_bb_height()) / 2.0;
    for _ in 0..count {
        spawn_silverfish(level, mob, pos.x, y, pos.z);
    }
}

/// `InfestedMobEffect.spawnSilverfish`.
fn spawn_silverfish(level: &mut ServerLevel, mob: &mut dyn Entity, x: f64, y: f64, z: f64) {
    let Some(mut silverfish) = factory::create(
        EntityType::SILVERFISH,
        level,
        EntitySpawnRequest {
            reason: EntitySpawnReason::Triggered,
            ignore_checks: false,
        },
    ) else {
        return;
    };
    let random_angle = mth::random_between(
        &mut mob.base_mut().random,
        -std::f32::consts::FRAC_PI_2,
        std::f32::consts::FRAC_PI_2,
    );
    let look = mob.get_look_angle();
    let (vx, vy, vz) = rotate_y(
        look.x as f32 * 0.3,
        look.y as f32 * 0.3 * 1.5,
        look.z as f32 * 0.3,
        random_angle,
    );
    let yaw = level.tick_state.random.next_float() * 360.0;
    silverfish.snap_to(x, y, z, yaw, 0.0);
    silverfish.base_mut().set_delta_movement(Vec3::new(
        f64::from(vx),
        f64::from(vy),
        f64::from(vz),
    ));
    level.add_fresh_entity(silverfish);
    level.play_sound_at(
        None,
        x,
        y,
        z,
        SoundEvent::ENTITY_SILVERFISH_HURT,
        SoundSource::Hostile,
        1.0,
        1.0,
    );
}

/// `OozingMobEffect.onMobRemoved` (KILLED): `spawnedCount = random -> 2`.
pub fn oozing(level: &mut ServerLevel, mob: &mut dyn Entity) {
    let requested = 2;
    let cramming = level.max_entity_cramming();
    let slimes_to_spawn = if cramming < 1 {
        requested
    } else {
        // `NearbySlimes.closeTo(mob).count(cramming)`: the search stops at `cramming` results
        let nearby = level
            .get_entities_of_class(
                &mob.base().bounding_box().inflate_scalar(2.0),
                |e| e.get_type() == EntityType::SLIME,
                |_| true,
            )
            .len() as i32;
        mth::clamp_i32(0, cramming - nearby.min(cramming), requested)
    };
    let pos = mob.base().position();
    for _ in 0..slimes_to_spawn {
        spawn_slime_offspring(level, pos.x, pos.y + 0.5, pos.z);
    }
}

/// `OozingMobEffect.spawnSlimeOffspring`.
fn spawn_slime_offspring(level: &mut ServerLevel, x: f64, y: f64, z: f64) {
    let Some(mut slime) = factory::create(
        EntityType::SLIME,
        level,
        EntitySpawnRequest {
            reason: EntitySpawnReason::Triggered,
            ignore_checks: false,
        },
    ) else {
        return;
    };
    if let Some(cube) = slime.as_cube_mob_mut() {
        cube.set_size(OOZING_SLIME_SIZE, true);
    }
    let yaw = level.tick_state.random.next_float() * 360.0;
    slime.snap_to(x, y, z, yaw, 0.0);
    level.add_fresh_entity(slime);
}

/// `WindChargedMobEffect.onMobRemoved` (KILLED).
pub fn wind_charged(level: &mut ServerLevel, mob: &mut dyn Entity) {
    let pos = mob.base().position();
    let y = pos.y + f64::from(mob.base().get_bb_height() / 2.0);
    let gust_strength = 3.0 + mob.base_mut().random.next_float() * 2.0;
    explode_with(
        mob,
        level,
        &EXPLOSION_DAMAGE_CALCULATOR,
        Vec3::new(pos.x, y, pos.z),
        gust_strength,
        SoundEvent::ENTITY_BREEZE_WIND_BURST,
    );
}
