//! `Drowned` (a [`Zombie`](super::Zombie) of kind [`ZombieKind::Drowned`](super::ZombieKind::Drowned)): amphibious
//! navigation, swimming, trident throwing, nautilus shell / zombie nautilus jockey.

use super::base;
use super::{Zombie, ZombieKind};
use crate::register_spawn_placement;
use crate::server::level::ServerLevel;
use crate::world::difficulty_instance::DifficultyInstance;
use crate::world::entity::ai::control::move_control::{MoveControl, MoveControlBase, Operation};
use crate::world::entity::ai::goal::{Flag, Goal, GoalFlags};
use crate::world::entity::ai::util::default_random_pos;
use crate::world::entity::equipment::EquipmentSlot;
use crate::world::entity::factory::EntitySpawnRequest;
use crate::world::entity::living_entity::util::l;
use crate::world::entity::mob::{self, navigation};
use crate::world::entity::spawn_group_data::SpawnGroupData;
use crate::world::entity::spawn_predicates as sp;
use crate::world::entity::{Entity, MoverType};
use crate::world::item::ItemStack;
use crate::world::level::server_level_accessor::ServerLevelAccessor;
use crate::world::level::block::state::BlockStateExt;
use crate::world::level::{BlockGetter, LevelReader};
use crate::world::level::material::FluidStateExt;
use crate::world::level::pathfinder::PathType;
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::Block;
use mc_data::enums::EntitySpawnReason;
use mc_data::{EntityType, Item};
use mc_world::level_storage::Difficulty;

pub const NAUTILUS_SHELL_CHANCE: f32 = 0.03;
pub const ZOMBIE_NAUTILUS_JOCKEY_CHANCE: f32 = 0.5;

/// The fields `Drowned` adds to `Zombie`.
pub struct DrownedData {
    /// `searchingForLand`.
    pub searching_for_land: bool,
    /// `rangedAttackUncertainty` (`-1`: the `RangedAttackMob` default).
    pub ranged_attack_uncertainty: f32,
}

impl Default for DrownedData {
    fn default() -> Self {
        Self { searching_for_land: false, ranged_attack_uncertainty: -1.0 }
    }
}

fn data(zombie: &Zombie) -> &DrownedData {
    match &zombie.kind {
        ZombieKind::Drowned(d) => d,
        _ => unreachable!("not a drowned"),
    }
}

fn data_mut(zombie: &mut Zombie) -> &mut DrownedData {
    match &mut zombie.kind {
        ZombieKind::Drowned(d) => d,
        _ => unreachable!("not a drowned"),
    }
}

/// The body of the `Drowned` constructor after `Zombie`'s.
pub fn init(zombie: &mut Zombie) {
    zombie.mob.move_control = Some(Box::new(DrownedMoveControl { base: MoveControlBase::default() }));
    zombie.mob.set_pathfinding_malus(PathType::Water, 0.0);
}

/// `isSearchingForLand()` of a drowned entity.
pub fn is_searching_for_land(e: &dyn Entity) -> bool {
    e.as_zombie().is_some_and(|z| matches!(z.kind, ZombieKind::Drowned(_)) && data(z).searching_for_land)
}

/// `setSearchingForLand(searchingForLand)`.
pub fn set_searching_for_land(e: &mut dyn Entity, searching_for_land: bool) {
    if let Some(z) = e.as_zombie_mut() {
        if let ZombieKind::Drowned(d) = &mut z.kind {
            d.searching_for_land = searching_for_land;
        }
    }
}

/// `okTarget(target)`.
pub fn ok_target(level: &ServerLevel, target: Option<&dyn Entity>) -> bool {
    target.is_some_and(|t| !level.is_bright_outside() || t.is_in_water())
}

/// `wantsToSwim()`.
pub fn wants_to_swim(e: &dyn Entity, level: &ServerLevel) -> bool {
    if is_searching_for_land(e) {
        return true;
    }
    mob::tick::get_target_checked(e, level).and_then(|id| level.get_entity(id)).is_some_and(|t| t.is_in_water())
}

/// `closeToNextPos()`.
pub fn close_to_next_pos(e: &dyn Entity, level: &ServerLevel) -> bool {
    let path = navigation::with_navigation_ref(e, level, |n| n.get_path()).flatten();
    if let Some(path) = path {
        let pos = path.get_target();
        let sqr_dist_to_next_pos = e.base().distance_to_sqr_xyz(f64::from(pos.x()), f64::from(pos.y()), f64::from(pos.z()));
        if sqr_dist_to_next_pos < 4.0 {
            return true;
        }
    }
    false
}

/// `populateDefaultEquipmentSlots(random, difficulty)`.
pub fn populate_default_equipment_slots(zombie: &mut Zombie, random: &mut dyn RandomSource, _difficulty: &DifficultyInstance) {
    if f64::from(random.next_float()) > 0.9 {
        let rand = random.next_int_bounded(16);
        let item = if rand < 10 { Item::TRIDENT } else { Item::FISHING_ROD };
        mob::equipment::set_item_slot_spawn(zombie, EquipmentSlot::MainHand, ItemStack::new(item));
    }
}

/// `finalizeSpawn(level, difficulty, spawnReason, groupData)`.
pub fn finalize_spawn(
    zombie: &mut Zombie,
    level: &mut dyn ServerLevelAccessor,
    difficulty: DifficultyInstance,
    reason: EntitySpawnReason,
    group_data: Option<Box<dyn SpawnGroupData>>,
) -> Option<Box<dyn SpawnGroupData>> {
    let group_data = base::finalize_spawn(zombie, level, difficulty, reason, group_data);
    if zombie.get_item_by_slot(EquipmentSlot::OffHand).is_empty() && level.random().next_float() < NAUTILUS_SHELL_CHANCE {
        mob::equipment::set_item_slot_spawn(zombie, EquipmentSlot::OffHand, ItemStack::new(Item::NAUTILUS_SHELL));
        mob::equipment::set_guaranteed_drop(zombie, EquipmentSlot::OffHand);
    }
    if (reason == EntitySpawnReason::Natural || reason == EntitySpawnReason::Structure)
        && zombie.get_item_by_slot(EquipmentSlot::MainHand).is(Item::TRIDENT)
        && level.random().next_float() < ZOMBIE_NAUTILUS_JOCKEY_CHANCE
        && !zombie.is_baby()
        && !level.biome_is_tag(zombie.block_position(), "more_frequent_drowned_spawns")
    {
        if let Some(mut zombie_nautilus) = level.create_entity(EntityType::ZOMBIE_NAUTILUS, EntitySpawnRequest { reason: EntitySpawnReason::Jockey, ignore_checks: false }) {
            if reason == EntitySpawnReason::Structure {
                zombie_nautilus.set_persistence_required();
            }
            let (x, y, z, y_rot) = (zombie.base().x(), zombie.base().y(), zombie.base().z(), zombie.base().y_rot());
            zombie_nautilus.snap_to(x, y, z, y_rot, 0.0);
            zombie_nautilus.finalize_spawn(level, difficulty, reason, None);
            base::start_riding_fresh_vehicle(zombie, &mut *zombie_nautilus, false, false, level);
            level.add_fresh_entity_with_passengers(zombie_nautilus);
        }
    }
    group_data
}

/// `Drowned.checkDrownedSpawnRules(type, level, spawnReason, pos, random)`.
pub fn check_drowned_spawn_rules(_ty: EntityType, level: &dyn ServerLevelAccessor, reason: EntitySpawnReason, pos: BlockPos, random: &mut dyn RandomSource) -> bool {
    if !level.get_fluid_state(pos.below()).is_tag("water") && !sp::is_spawner(reason) {
        return false;
    }
    let more_frequent = level.biome_is_tag(pos, "more_frequent_drowned_spawns");
    let can_monster_spawn = level.get_difficulty() != Difficulty::Peaceful
        && (sp::ignores_light_requirements(reason) || sp::is_dark_enough_to_spawn(level, pos, random))
        && (sp::is_spawner(reason) || level.get_fluid_state(pos).is_tag("water"));
    if !can_monster_spawn || (!sp::is_spawner(reason) && reason != EntitySpawnReason::Reinforcement) {
        if more_frequent {
            random.next_int_bounded(15) == 0 && can_monster_spawn
        } else {
            random.next_int_bounded(40) == 0 && is_deep_enough_to_spawn(level, pos) && can_monster_spawn
        }
    } else {
        true
    }
}

register_spawn_placement!(EntityType::DROWNED, check_drowned_spawn_rules);

/// `isDeepEnoughToSpawn(level, pos)`.
fn is_deep_enough_to_spawn(level: &dyn ServerLevelAccessor, pos: BlockPos) -> bool {
    pos.y() < level.get_sea_level() - 5
}

/// `travelInWater(input, baseGravity, isFalling, oldY)`.
pub fn travel_in_water(zombie: &mut Zombie, level: &mut ServerLevel, input: Vec3, base_gravity: f64, is_falling: bool, old_y: f64) {
    if zombie.is_under_water() && wants_to_swim(zombie, level) {
        zombie.move_relative(0.01, input);
        let movement = zombie.base().delta_movement();
        let _ = zombie.r#move(level, MoverType::SelfMover, movement);
        let scaled = zombie.base().delta_movement().scale(0.9);
        zombie.base_mut().set_delta_movement(scaled);
    } else {
        crate::world::entity::living_entity::travel::travel_in_water(zombie, level, input, base_gravity, is_falling, old_y);
    }
}

/// `updateSwimming()`.
pub fn update_swimming(zombie: &mut Zombie, level: &ServerLevel) {
    let swimming = zombie.is_effective_ai() && zombie.is_under_water() && wants_to_swim(zombie, level);
    zombie.set_swimming(swimming);
}

/// `DrownedGoToWaterGoal`.
pub struct DrownedGoToWaterGoal {
    wanted_x: f64,
    wanted_y: f64,
    wanted_z: f64,
    speed_modifier: f64,
}

impl DrownedGoToWaterGoal {
    pub fn new(speed_modifier: f64) -> Self {
        Self { wanted_x: 0.0, wanted_y: 0.0, wanted_z: 0.0, speed_modifier }
    }

    /// `getWaterPos()`.
    fn get_water_pos(mob: &mut dyn Entity, level: &ServerLevel) -> Option<Vec3> {
        let pos = mob.block_position();
        for _ in 0..10 {
            let random = &mut mob.base_mut().random;
            let random_pos = pos.offset(random.next_int_bounded(20) - 10, 2 - random.next_int_bounded(8), random.next_int_bounded(20) - 10);
            if level.get_block_state(random_pos).is_block(Block::WATER) {
                return Some(Vec3::at_bottom_center_of(&random_pos));
            }
        }
        None
    }
}

impl Goal for DrownedGoToWaterGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        if !level.is_bright_outside() {
            return false;
        }
        if mob.is_in_water() {
            return false;
        }
        match Self::get_water_pos(mob, level) {
            None => false,
            Some(pos) => {
                self.wanted_x = pos.x;
                self.wanted_y = pos.y;
                self.wanted_z = pos.z;
                true
            }
        }
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        !navigation::is_done(mob, level)
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        navigation::move_to(mob, level, self.wanted_x, self.wanted_y, self.wanted_z, self.speed_modifier);
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Move])
    }
}

/// `DrownedSwimUpGoal` (the sea level is read from the level when the goal runs: it never changes).
pub struct DrownedSwimUpGoal {
    speed_modifier: f64,
    stuck: bool,
}

impl DrownedSwimUpGoal {
    pub fn new(speed_modifier: f64) -> Self {
        Self { speed_modifier, stuck: false }
    }
}

impl Goal for DrownedSwimUpGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        !level.is_bright_outside() && mob.is_in_water() && mob.base().y() < f64::from(level.get_sea_level() - 2)
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.can_use(mob, level) && !self.stuck
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        let sea_level = level.get_sea_level();
        if mob.base().y() < f64::from(sea_level - 1) && (navigation::is_done(mob, level) || close_to_next_pos(mob, level)) {
            let towards = Vec3::new(mob.base().x(), f64::from(sea_level - 1), mob.base().z());
            match default_random_pos::get_pos_towards(mob, level, 4, 8, towards, f64::from((std::f64::consts::PI / 2.0) as f32)) {
                None => {
                    self.stuck = true;
                    return;
                }
                Some(next) => {
                    navigation::move_to(mob, level, next.x, next.y, next.z, self.speed_modifier);
                }
            }
        }
    }

    fn start(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) {
        set_searching_for_land(mob, true);
        self.stuck = false;
    }

    fn stop(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) {
        set_searching_for_land(mob, false);
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::NONE
    }
}

/// `DrownedMoveControl`.
pub struct DrownedMoveControl {
    base: MoveControlBase,
}

impl MoveControl for DrownedMoveControl {
    fn base(&self) -> &MoveControlBase {
        &self.base
    }

    fn base_mut(&mut self) -> &mut MoveControlBase {
        &mut self.base
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        let target = mob::tick::get_target_checked(mob, level);
        if wants_to_swim(mob, level) && mob.is_in_water() {
            let target_above = target.and_then(|id| level.get_entity(id)).is_some_and(|t| t.base().y() > mob.base().y());
            if target_above || is_searching_for_land(mob) {
                let m = mob.base().delta_movement();
                mob.base_mut().set_delta_movement(m.add_xyz(0.0, 0.002, 0.0));
            }
            if self.base.operation != Operation::MoveTo || navigation::is_done(mob, level) {
                mob.set_speed(0.0);
                return;
            }
            let xd = self.base.wanted_x - mob.base().x();
            let mut yd = self.base.wanted_y - mob.base().y();
            let zd = self.base.wanted_z - mob.base().z();
            let dd = (xd * xd + yd * yd + zd * zd).sqrt();
            yd /= dd;
            let y_rot_d = (mth::atan2(zd, xd) * f64::from(180.0f32) / f64::from(std::f32::consts::PI)) as f32 - 90.0;
            let new_y_rot = self.base.rotlerp(mob.base().y_rot(), y_rot_d, 90.0);
            mob.set_y_rot(new_y_rot);
            let y_rot = mob.base().y_rot();
            if let Some(living) = mob.living_mut() {
                living.y_body_rot = y_rot;
            }
            let target_speed = (self.base.speed_modifier * mob::attribute_value(mob, mc_data::Attribute::MOVEMENT_SPEED)) as f32;
            let new_speed = mth::lerp_f32(0.125, mob.get_speed(), target_speed);
            mob.set_speed(new_speed);
            let ns = f64::from(new_speed);
            let m = mob.base().delta_movement();
            mob.base_mut().set_delta_movement(m.add_xyz(ns * xd * 0.005, ns * yd * 0.1, ns * zd * 0.005));
        } else {
            if !mob.base().on_ground() {
                let m = mob.base().delta_movement();
                mob.base_mut().set_delta_movement(m.add_xyz(0.0, -0.008, 0.0));
            }
            crate::world::entity::ai::control::move_control::base_tick(&mut self.base, mob, level);
        }
    }
}

#[allow(dead_code)]
fn _unused(zombie: &Zombie) {
    let _ = (data(zombie), l);
    let _ = data_mut;
}
