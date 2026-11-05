//! `AgeableMob`: age, forced age, baby flag, age lock, `AgeableMobGroupData` finalize.

use super::spawn::AgeableMobGroupData;
use super::{AGE_LOCKED, DATA_BABY_ID};
use crate::server::level::ServerLevel;
use crate::world::difficulty_instance::DifficultyInstance;
use crate::world::entity::spawn_group_data::SpawnGroupData;
use crate::world::entity::Entity;
use crate::world::item::InteractionResult;
use crate::world::level::server_level_accessor::ServerLevelAccessor;
use mc_data::enums::EntitySpawnReason;
use mc_data::{EntityType, Item, ParticleType, SoundEvent};
use mc_nbt::CompoundTag;
use mc_protocol::packets::play::ParticleOptions;
use mc_protocol::packets::play::level::SoundSource;

pub const BABY_START_AGE: i32 = -24000;
pub const AGE_LOCK_COOLDOWN_TICKS: i32 = 40;
pub const AGE_LOCK_DOWNWARDS_MOVING_PARTICLE_Y_OFFSET: f32 = 0.2;
pub const FORCED_AGE_PARTICLE_TICKS: i32 = 40;

/// Fields of `AgeableMob`.
#[derive(Default, Debug)]
pub struct AgeableData {
    pub age: i32,
    pub forced_age: i32,
    pub forced_age_timer: i32,
    pub age_lock_particle_timer: i32,
}

fn ad(e: &dyn Entity) -> &AgeableData {
    e.mob().and_then(|m| m.ageable.as_ref()).expect("ageable mob")
}

fn adm(e: &mut dyn Entity) -> &mut AgeableData {
    e.mob_mut().and_then(|m| m.ageable.as_mut()).expect("ageable mob")
}

/// `getAge()` (server side: the stored age).
pub fn get_age(e: &dyn Entity) -> i32 {
    ad(e).age
}

/// `setAge(newAge)`: flips `DATA_BABY_ID` when crossing 0 and runs `onSyncedDataUpdated(DATA_BABY_ID)`
/// (`refreshDimensions()`); returns whether the baby boundary was crossed (then call `age_boundary_reached`).
pub fn set_age(e: &mut dyn Entity, new_age: i32) -> bool {
    let old_age = get_age(e);
    adm(e).age = new_age;
    if old_age < 0 && new_age >= 0 || old_age >= 0 && new_age < 0 {
        e.base_mut().entity_data.set(DATA_BABY_ID, new_age < 0);
        e.on_synced_data_updated(DATA_BABY_ID.id());
        true
    } else {
        false
    }
}

/// `setAge(newAge)` including `ageBoundaryReached()`.
pub fn set_age_in_level(e: &mut dyn Entity, level: &mut ServerLevel, new_age: i32) {
    if set_age(e, new_age) {
        e.age_boundary_reached(level);
    }
}

/// `ageBoundaryReached()`: a grown-up that rides a boat without enough space stops riding.
pub fn age_boundary_reached(e: &mut dyn Entity, level: &mut ServerLevel) {
    if !e.is_baby() && e.base().is_passenger() {
        if let Some(vehicle) = e.base().vehicle {
            let boat_without_space = level.get_entity(vehicle).is_some_and(|v| v.has_enough_space_for(level, e.as_dyn()) == Some(false));
            if boat_without_space {
                level.stop_riding(e);
            }
        }
    }
}

/// `ageUp(seconds, forced)`.
pub fn age_up(e: &mut dyn Entity, level: &mut ServerLevel, seconds: i32, forced: bool) {
    let mut age = get_age(e);
    let old_age = age;
    age += seconds * 20;
    if age > 0 {
        age = 0;
    }
    let delta = age - old_age;
    set_age_in_level(e, level, age);
    if forced {
        let d = adm(e);
        d.forced_age += delta;
        if d.forced_age_timer == 0 {
            d.forced_age_timer = 40;
        }
    }
    if get_age(e) == 0 {
        let forced_age = ad(e).forced_age;
        set_age_in_level(e, level, forced_age);
    }
}

/// `getSpeedUpSecondsWhenFeeding(ticksUntilAdult)`.
pub fn get_speed_up_seconds_when_feeding(ticks_until_adult: i32) -> i32 {
    (((ticks_until_adult / 20) as f32) * 0.1f32) as i32
}

/// `isAgeLocked()`.
pub fn is_age_locked(e: &dyn Entity) -> bool {
    e.base().entity_data.get(AGE_LOCKED)
}

/// `setAgeLocked(locked)`.
pub fn set_age_locked(e: &mut dyn Entity, locked: bool) {
    e.base_mut().entity_data.set(AGE_LOCKED, locked);
}

/// `canAgeUp()`.
pub fn can_age_up(e: &dyn Entity) -> bool {
    e.is_baby() && !is_age_locked(e)
}

/// `isBaby()` (`canBeABaby() && getAge() < 0`).
pub fn is_baby(e: &dyn Entity) -> bool {
    e.can_be_a_baby() && get_age(e) < 0
}

/// `setBaby(baby)`.
pub fn set_baby(e: &mut dyn Entity, baby: bool) {
    if e.can_be_a_baby() {
        let start = if baby { e.get_baby_start_age() } else { 0 };
        set_age(e, start);
    }
}

/// `AgeableMob.finalizeSpawn(...)` up to `super.finalizeSpawn` (which the caller runs next).
pub fn finalize_spawn(
    e: &mut dyn Entity,
    level: &mut dyn ServerLevelAccessor,
    _difficulty: DifficultyInstance,
    _reason: EntitySpawnReason,
    group_data: Option<Box<dyn SpawnGroupData>>,
) -> Option<Box<dyn SpawnGroupData>> {
    let mut group_data = group_data.unwrap_or_else(|| Box::new(AgeableMobGroupData::with_baby(true)));
    {
        let data = group_data.as_ageable_mut().expect("AgeableMobGroupData");
        if data.is_should_spawn_baby() && data.get_group_size() > 0 && level.random().next_float() <= data.get_baby_spawn_chance() {
            let start = e.get_baby_start_age();
            set_age(e, start);
        }
        data.increase_group_size_by_one();
    }
    Some(group_data)
}

/// `AgeableMob.aiStep()` after `super.aiStep()`.
pub fn ai_step_after(e: &mut dyn Entity, level: &mut ServerLevel) {
    if ad(e).forced_age_timer > 0 {
        if ad(e).forced_age_timer % 4 == 0 {
            let x = e.base_mut().get_random_x(1.0);
            let y = e.base_mut().get_random_y() + 0.5;
            let z = e.base_mut().get_random_z(1.0);
            level.send_particles(ParticleOptions::Simple(ParticleType::HAPPY_VILLAGER), x, y, z, 1, 0.0, 0.0, 0.0, 0.0);
        }
        adm(e).forced_age_timer -= 1;
    }
    if e.is_alive() {
        let mut age = get_age(e);
        if e.can_age_up() {
            age += 1;
            set_age_in_level(e, level, age);
        } else if age > 0 {
            age -= 1;
            set_age_in_level(e, level, age);
        }
    }
    let timer = ad(e).age_lock_particle_timer;
    let locked = is_age_locked(e);
    let new_timer = make_age_locked_particle(level, e, timer, locked);
    adm(e).age_lock_particle_timer = new_timer;
}

/// `makeAgeLockedParticle(level, mob, ageLockParticleTimer, isAgeLocked)`.
pub fn make_age_locked_particle(level: &mut ServerLevel, mob: &mut dyn Entity, mut age_lock_particle_timer: i32, is_age_locked: bool) -> i32 {
    if age_lock_particle_timer > 0 {
        if age_lock_particle_timer % 2 == 0 {
            let y_particle_offset = if is_age_locked { 0.2f32 } else { 0.0f32 };
            let x = mob.base_mut().get_random_x(1.0);
            let y = mob.base_mut().get_random_y_spread(0.2) + f64::from(mob.base().get_bb_height()) + f64::from(y_particle_offset);
            let z = mob.base_mut().get_random_z(1.0);
            let particle = if is_age_locked { ParticleType::PAUSE_MOB_GROWTH } else { ParticleType::RESET_MOB_GROWTH };
            level.send_particles(ParticleOptions::Simple(particle), x, y, z, 1, 0.0, 0.0, 0.0, 0.0);
        }
        age_lock_particle_timer -= 1;
    }
    age_lock_particle_timer
}

/// `canUseGoldenDandelion(itemInHand, isBaby, cooldown, mob)`.
pub fn can_use_golden_dandelion(item_in_hand: &crate::world::item::ItemStack, is_baby: bool, cooldown: i32, mob: &dyn Entity) -> bool {
    item_in_hand.item() == Item::GOLDEN_DANDELION
        && is_baby
        && cooldown == 0
        && !mc_data::tags::get::<EntityType>("cannot_be_age_locked").is_some_and(|t| t.contains(mob.get_type()))
}

/// `AgeableMob.mobInteract(player, hand)` golden dandelion part; `None` when the item does not apply.
pub fn golden_dandelion_interact(e: &mut dyn Entity, level: &mut ServerLevel, held: &mut crate::world::item::ItemStack, player_creative: bool) -> Option<InteractionResult> {
    let timer = ad(e).age_lock_particle_timer;
    if can_use_golden_dandelion(held, e.is_baby(), timer, e) {
        // setAgeLockedData
        let locked = !is_age_locked(e);
        set_age_locked(e, locked);
        let start = e.get_baby_start_age();
        set_age_in_level(e, level, start);
        adm(e).age_lock_particle_timer = AGE_LOCK_COOLDOWN_TICKS;
        // setAgeLocked(...): itemInHand.consume(1, player)
        if !player_creative {
            held.shrink(1);
        }
        let now_locked = is_age_locked(e);
        if now_locked {
            e.mob_mut().expect("mob").persistence_required = true;
        }
        let pos = e.block_position();
        let sound = if now_locked { SoundEvent::ITEM_GOLDEN_DANDELION_USE } else { SoundEvent::ITEM_GOLDEN_DANDELION_UNUSE };
        level.play_sound(None, pos, sound, SoundSource::Players, 1.0, 1.0);
        Some(InteractionResult::SUCCESS)
    } else {
        None
    }
}

/// `AgeableMob.addAdditionalSaveData`.
pub fn add_additional_save_data(e: &dyn Entity, out: &mut CompoundTag) {
    if e.can_be_a_baby() {
        out.put_int("Age", get_age(e));
        out.put_int("ForcedAge", ad(e).forced_age);
        out.put_boolean("AgeLocked", is_age_locked(e));
    }
}

/// `AgeableMob.readAdditionalSaveData`.
pub fn read_additional_save_data(e: &mut dyn Entity, input: &CompoundTag) {
    if e.can_be_a_baby() {
        set_age(e, input.get_int_or("Age", 0));
        adm(e).forced_age = input.get_int_or("ForcedAge", 0);
        set_age_locked(e, input.get_boolean_or("AgeLocked", false));
    }
}
