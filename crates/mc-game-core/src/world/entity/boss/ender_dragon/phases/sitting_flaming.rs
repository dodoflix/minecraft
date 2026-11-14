//! `DragonSittingFlamingPhase`: breathe a lingering cloud of dragon breath in front of the head, for ten seconds.

use super::EnderDragonPhase;
use crate::server::level::ServerLevel;
use crate::world::entity::area_effect_cloud::AreaEffectCloud;
use crate::world::entity::boss::ender_dragon::{EnderDragon, HEAD};
use crate::world::entity::effect::MobEffectInstance;
use crate::world::entity::{Entity, EntityId};
use crate::world::level::LevelReader;
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_data::{MobEffect, ParticleType};
use mc_protocol::packets::play::ParticleOptions;

/// `FLAME_DURATION`.
const FLAME_DURATION: i32 = 200;
/// `SITTING_FLAME_ATTACKS_COUNT`.
const SITTING_FLAME_ATTACKS_COUNT: i32 = 4;
/// `WARMUP_TIME`.
const WARMUP_TIME: i32 = 10;

#[derive(Default)]
pub struct State {
    /// `flameTicks`.
    pub flame_ticks: i32,
    /// `flameCount`.
    pub flame_count: i32,
    /// `flame`.
    pub flame: Option<EntityId>,
    /// Clouds to discard at the dragon's next tick: `end()` ran without a level.
    pub discard_later: Vec<EntityId>,
}

/// `begin()`.
pub fn begin(dragon: &mut EnderDragon) {
    let state = &mut dragon.phases.sitting_flaming;
    state.flame_ticks = 0;
    state.flame_count += 1;
}

/// `end()`: the cloud is discarded.
pub fn end(dragon: &mut EnderDragon, level: Option<&mut ServerLevel>) {
    let Some(flame) = dragon.phases.sitting_flaming.flame.take() else {
        return;
    };
    match level {
        Some(level) => discard(level, flame),
        None => dragon.phases.sitting_flaming.discard_later.push(flame),
    }
}

/// The clouds `end()` could not discard, at the start of the dragon's tick.
pub fn discard_pending(dragon: &mut EnderDragon, level: &mut ServerLevel) {
    for flame in std::mem::take(&mut dragon.phases.sitting_flaming.discard_later) {
        discard(level, flame);
    }
}

fn discard(level: &mut ServerLevel, flame: EntityId) {
    level.try_with_entity(flame, |level, flame| flame.discard_in_level(level));
}

/// `resetFlameCount()`.
pub fn reset_flame_count(dragon: &mut EnderDragon) {
    dragon.phases.sitting_flaming.flame_count = 0;
}

/// `doServerTick(level)`.
pub fn do_server_tick(dragon: &mut EnderDragon, level: &mut ServerLevel) {
    dragon.phases.sitting_flaming.flame_ticks += 1;
    let flame_ticks = dragon.phases.sitting_flaming.flame_ticks;
    if flame_ticks >= FLAME_DURATION {
        if dragon.phases.sitting_flaming.flame_count >= SITTING_FLAME_ATTACKS_COUNT {
            dragon.set_phase(level, EnderDragonPhase::Takeoff);
        } else {
            dragon.set_phase(level, EnderDragonPhase::SittingScanning);
        }
    } else if flame_ticks == WARMUP_TIME {
        let Some(head) = level.dragon_part(dragon.part_ids()[HEAD]) else {
            return;
        };
        let dragon_pos = dragon.base().position();
        let look = Vec3::new(
            head.base().x() - dragon_pos.x,
            0.0,
            head.base().z() - dragon_pos.z,
        )
        .normalize();
        let x = head.base().x() + look.x * 5.0 / 2.0;
        let z = head.base().z() + look.z * 5.0 / 2.0;
        let initial_y = head.base().get_y_progress(0.5);
        let mut y = initial_y;
        let mut pos = BlockPos::containing(x, initial_y, z);
        while level.is_empty_block(pos) {
            y -= 1.0;
            if y < 0.0 {
                y = initial_y;
                break;
            }
            pos = BlockPos::containing(x, y, z);
        }
        y = f64::from(mth::floor_f64(y) + 1);
        let mut flame = AreaEffectCloud::at(level, x, y, z);
        flame.set_owner(Some(&*dragon));
        flame.set_radius(5.0);
        flame.set_duration(200);
        flame.set_custom_particle(Some(ParticleOptions::Power {
            particle_type: ParticleType::DRAGON_BREATH,
            power: 1.0,
        }));
        flame.set_potion_duration_scale(0.25);
        flame.add_effect(&MobEffectInstance::of(MobEffect::INSTANT_DAMAGE, 0, 0));
        let id = flame.id();
        level.add_fresh_entity(Box::new(flame));
        dragon.phases.sitting_flaming.flame = Some(id);
    }
}
