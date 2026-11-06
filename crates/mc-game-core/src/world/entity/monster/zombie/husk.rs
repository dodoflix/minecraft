//! `Husk` (a `Zombie` of kind `ZombieKind::Husk`): hunger on hit, no sun
//! burning (it is not in the `burn_in_daylight` tag), the camel husk + parched jockey spawn.

use super::Zombie;
use super::base::{self, ZombieGroupData};
use crate::impl_spawn_group_data;
use crate::server::level::ServerLevel;
use crate::world::difficulty_instance::DifficultyInstance;
use crate::world::entity::Entity;
use crate::world::entity::dimensions::EntityDimensions;
use crate::world::entity::effect::MobEffectInstance;
use crate::world::entity::equipment::EquipmentSlot;
use crate::world::entity::factory::EntitySpawnRequest;
use crate::world::entity::mob;
use crate::world::entity::spawn_group_data::SpawnGroupData;
use crate::world::item::ItemStack;
use crate::world::level::server_level_accessor::ServerLevelAccessor;
use mc_core::phys::AABB;
use mc_data::enums::EntitySpawnReason;
use mc_data::{EntityType, Item, MobEffect};

/// `Husk.HuskGroupData`.
pub struct HuskGroupData {
    pub base: ZombieGroupData,
    pub tried_to_spawn_camel_husk: bool,
}

impl HuskGroupData {
    /// `new HuskGroupData(groupData)`.
    pub fn new(group_data: &ZombieGroupData) -> Self {
        Self {
            base: ZombieGroupData::new(group_data.is_baby, group_data.can_spawn_jockey),
            tried_to_spawn_camel_husk: false,
        }
    }
}

impl_spawn_group_data!(HuskGroupData);

/// The part of `Husk.doHurtTarget` after `super.doHurtTarget`.
pub fn do_hurt_target_after(
    zombie: &mut Zombie,
    level: &mut ServerLevel,
    target: &mut dyn Entity,
    result: bool,
) -> bool {
    if result
        && zombie.get_item_by_slot(EquipmentSlot::MainHand).is_empty()
        && target.living().is_some()
    {
        let difficulty =
            ServerLevelAccessor::get_current_difficulty_at(&*level, zombie.block_position())
                .get_effective_difficulty();
        let source = zombie.id();
        target.add_effect(
            level,
            MobEffectInstance::of(MobEffect::HUNGER, 140 * difficulty as i32, 0),
            Some(source),
        );
    }
    result
}

/// `EntityType.getSpawnAABB(x, y, z)`.
fn get_spawn_aabb(ty: EntityType, x: f64, y: f64, z: f64) -> AABB {
    let dims = EntityDimensions::of_type(ty);
    let w = dims.width / 2.0;
    AABB::new(
        x - f64::from(w),
        y,
        z - f64::from(w),
        x + f64::from(w),
        y + f64::from(dims.height),
        z + f64::from(w),
    )
}

/// `Husk.finalizeSpawn(level, difficulty, spawnReason, groupData)`.
pub fn finalize_spawn(
    zombie: &mut Zombie,
    level: &mut dyn ServerLevelAccessor,
    difficulty: DifficultyInstance,
    reason: EntitySpawnReason,
    group_data: Option<Box<dyn SpawnGroupData>>,
) -> Option<Box<dyn SpawnGroupData>> {
    let group_data = base::finalize_spawn(zombie, level, difficulty, reason, group_data);
    let difficulty_modifier = difficulty.get_special_multiplier();
    if reason != EntitySpawnReason::Conversion {
        let can_pick_up = level.random().next_float() < 0.55 * difficulty_modifier;
        zombie.set_can_pick_up_loot(can_pick_up);
    }
    let mut group_data = group_data;
    if let Some(inner) = group_data.as_deref_mut().and_then(base::as_zombie_group) {
        let mut husk_data = HuskGroupData::new(inner);
        husk_data.tried_to_spawn_camel_husk = reason != EntitySpawnReason::Natural;
        group_data = Some(Box::new(husk_data));
    }
    let wants_camel = group_data
        .as_deref()
        .and_then(|g| g.as_any().downcast_ref::<HuskGroupData>())
        .is_some_and(|h| !h.tried_to_spawn_camel_husk);
    if wants_camel {
        let pos = zombie.block_position();
        if level.no_collision_aabb(&get_spawn_aabb(
            EntityType::CAMEL_HUSK,
            f64::from(pos.x()) + 0.5,
            f64::from(pos.y()),
            f64::from(pos.z()) + 0.5,
        )) {
            if let Some(h) = group_data
                .as_deref_mut()
                .and_then(|g| g.as_any_mut().downcast_mut::<HuskGroupData>())
            {
                h.tried_to_spawn_camel_husk = true;
            }
            if level.random().next_float() < 0.1 {
                mob::equipment::set_item_slot_spawn(
                    zombie,
                    EquipmentSlot::MainHand,
                    ItemStack::new(Item::IRON_SPEAR),
                );
                spawn_camel_husk_jockeys(zombie, level, difficulty, reason);
            }
        }
    }
    group_data
}

/// The camel husk (ridden by the husk) and its parched rider.
fn spawn_camel_husk_jockeys(
    zombie: &mut Zombie,
    level: &mut dyn ServerLevelAccessor,
    difficulty: DifficultyInstance,
    reason: EntitySpawnReason,
) {
    let Some(mut camel_husk) = level.create_entity(
        EntityType::CAMEL_HUSK,
        EntitySpawnRequest {
            reason: EntitySpawnReason::Natural,
            ignore_checks: false,
        },
    ) else {
        return;
    };
    let (x, y, z, y_rot) = (
        zombie.base().x(),
        zombie.base().y(),
        zombie.base().z(),
        zombie.base().y_rot(),
    );
    camel_husk.set_pos(x, y, z);
    camel_husk.finalize_spawn(level, difficulty, reason, None);
    base::start_riding_fresh_vehicle(zombie, &mut *camel_husk, true, true, level);
    let camel_id = camel_husk.id();
    level.add_fresh_entity_with_passengers(camel_husk);
    let Some(mut parched) = level.create_entity(
        EntityType::PARCHED,
        EntitySpawnRequest {
            reason: EntitySpawnReason::Natural,
            ignore_checks: false,
        },
    ) else {
        return;
    };
    parched.snap_to(x, y, z, y_rot, 0.0);
    parched.finalize_spawn(level, difficulty, reason, None);
    if let Some(sl) = level.as_server_level_mut() {
        sl.start_riding(&mut *parched, camel_id, false, false);
    }
    level.add_fresh_entity_with_passengers(parched);
}
