//! `net.minecraft.world.entity.monster.zombie`: `Zombie` and its subclasses `Husk`, `Drowned`, `ZombieVillager`,
//! `ZombifiedPiglin`.
//!
//! The whole family is one Rust type ([`Zombie`]) with a [`ZombieKind`] for the class-specific state: vanilla's
//! overrides become `match self.kind` arms in the one `Entity` impl and the "super" calls are the plain functions of this
//! module. Registered for the five entity types in [`register`].

mod base;
mod goals;
pub mod drowned;
pub mod husk;
pub mod zombie_villager;
pub mod zombified_piglin;

pub use base::{ZombieGroupData, get_spawn_as_baby_odds, start_riding_fresh_vehicle};

use crate::define_accessor;
use crate::registry::RegistryAccess;
use crate::server::level::ServerLevel;
use crate::world::damagesource::{DamageEntity, DamageSource};
use crate::world::difficulty_instance::DifficultyInstance;
use crate::world::entity::dimensions::{EntityAttachments, EntityDimensions};
use crate::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use crate::world::entity::mob::conversion_tracker::{ConversionTracker, ConversionTrackerConfig};
use crate::world::entity::mob::{self, MobData, MobKind};
use crate::world::entity::spawn_group_data::SpawnGroupData;
use crate::world::entity::{Entity, EntityId};
use crate::world::item::ItemStack;
use crate::world::level::server_level_accessor::ServerLevelAccessor;
use linkme::distributed_slice;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::enums::{EntitySpawnReason, Pose};
use mc_data::{EntityType, SoundEvent};
use mc_nbt::CompoundTag;

define_accessor!(pub DATA_BABY_ID: Boolean = Zombie);
define_accessor!(pub DATA_SPECIAL_TYPE_ID: Int = Zombie);
define_accessor!(pub DATA_DROWNED_CONVERSION_ID: Boolean = Zombie);

/// `Zombie.ZOMBIE_LEADER_CHANCE`.
pub const ZOMBIE_LEADER_CHANCE: f32 = 0.05;
pub const REINFORCEMENT_ATTEMPTS: i32 = 50;
pub const REINFORCEMENT_RANGE_MAX: i32 = 40;
pub const REINFORCEMENT_RANGE_MIN: i32 = 7;
pub const BREAK_DOOR_CHANCE: f32 = 0.1;

/// The class of a [`Zombie`] and the state only that class has.
pub enum ZombieKind {
    Zombie,
    Husk,
    Drowned(drowned::DrownedData),
    ZombieVillager(zombie_villager::ZombieVillagerData),
    ZombifiedPiglin(zombified_piglin::ZombifiedPiglinData),
}

/// `Zombie` (and, through [`ZombieKind`], `Husk`, `Drowned`, `ZombieVillager`, `ZombifiedPiglin`).
pub struct Zombie {
    pub mob: MobData,
    pub kind: ZombieKind,
    /// `canBreakDoors`.
    can_break_doors: bool,
    /// The `BreakDoorGoal` instance while it is not registered with the goal selector (`breakDoorGoal` is one object that is
    /// added and removed by `setCanBreakDoors`, its state survives).
    parked_break_door_goal: Option<Box<dyn crate::world::entity::ai::goal::Goal>>,
    /// Goals removed while running (`setCanBreakDoors(false)` outside a tick): stopped by the next tick.
    goals_to_stop: Vec<Box<dyn crate::world::entity::ai::goal::Goal>>,
    /// `drowningTracker`.
    drowning_tracker: ConversionTracker,
}

/// The `ConversionTracker` constants of `Zombie.drowningTracker`.
const DROWNING_TRACKER: ConversionTrackerConfig = ConversionTrackerConfig {
    conversion_id: DATA_DROWNED_CONVERSION_ID,
    affliction_time_tag: "InWaterTime",
    total_affliction_time: 600,
    conversion_time_tag: "DrownedConversionTime",
    total_conversion_time: 300,
};

impl Zombie {
    /// `new Zombie(type, level)` (and the subclass constructors): `registerGoals`, `createNavigation`, control replacements.
    pub fn new(ty: EntityType, id: EntityId, registries: &RegistryAccess) -> Self {
        let kind = match ty {
            EntityType::HUSK => ZombieKind::Husk,
            EntityType::DROWNED => ZombieKind::Drowned(drowned::DrownedData::default()),
            EntityType::ZOMBIE_VILLAGER => ZombieKind::ZombieVillager(zombie_villager::ZombieVillagerData::default()),
            EntityType::ZOMBIFIED_PIGLIN => ZombieKind::ZombifiedPiglin(zombified_piglin::ZombifiedPiglinData::default()),
            _ => ZombieKind::Zombie,
        };
        let mob = MobData::new(ty, id, MobKind::Monster, |b| {
            b.define(DATA_BABY_ID, false);
            b.define(DATA_SPECIAL_TYPE_ID, 0);
            b.define(DATA_DROWNED_CONVERSION_ID, false);
            if ty == EntityType::ZOMBIE_VILLAGER {
                zombie_villager::define_synched_data(b, registries);
            }
        });
        let mut zombie = Zombie { mob, kind, can_break_doors: false, parked_break_door_goal: None, goals_to_stop: Vec::new(), drowning_tracker: ConversionTracker::new(DROWNING_TRACKER) };
        let navigation = match zombie.kind {
            ZombieKind::Drowned(_) => crate::world::entity::ai::navigation::StandardPathNavigation::amphibious(&zombie),
            _ => crate::world::entity::ai::navigation::StandardPathNavigation::ground(&zombie),
        };
        zombie.mob.set_navigation(Box::new(navigation));
        match zombie.kind {
            ZombieKind::Drowned(_) => drowned::init(&mut zombie),
            ZombieKind::ZombifiedPiglin(_) => zombified_piglin::init(&mut zombie),
            ZombieKind::ZombieVillager(_) => zombie_villager::init(&mut zombie, registries),
            _ => {}
        }
        zombie.register_goals();
        zombie
    }

    /// `registerGoals()` (+ the virtual `addBehaviourGoals()`).
    fn register_goals(&mut self) {
        base::register_goals(self);
    }

    pub fn can_break_doors(&self) -> bool {
        self.can_break_doors
    }

    /// `isUnderWaterConverting()`.
    pub fn is_under_water_converting(&self) -> bool {
        self.drowning_tracker.is_converting(&self.mob.living.entity.entity_data)
    }

    /// `convertsInWater()`.
    pub fn converts_in_water(&self) -> bool {
        matches!(self.kind, ZombieKind::Zombie | ZombieKind::Husk)
    }

    /// `convertsToWhenDrowning()`.
    pub fn converts_to_when_drowning(&self) -> EntityType {
        match self.kind {
            ZombieKind::Husk => EntityType::ZOMBIE,
            _ => EntityType::DROWNED,
        }
    }

    /// `getConversionSound()` (a level event id).
    pub fn get_conversion_sound(&self) -> i32 {
        match self.kind {
            ZombieKind::Husk => 1041,
            _ => 1040,
        }
    }

    /// `canSpawnInLiquids()`.
    pub fn can_spawn_in_liquids(&self) -> bool {
        matches!(self.kind, ZombieKind::Drowned(_))
    }

    /// `getStepSound()`.
    fn get_step_sound(&self) -> SoundEvent {
        match self.kind {
            ZombieKind::Husk => SoundEvent::ENTITY_HUSK_STEP,
            ZombieKind::Drowned(_) => SoundEvent::ENTITY_DROWNED_STEP,
            ZombieKind::ZombieVillager(_) => SoundEvent::ENTITY_ZOMBIE_VILLAGER_STEP,
            _ => SoundEvent::ENTITY_ZOMBIE_STEP,
        }
    }

    /// `BABY_DIMENSIONS` of the class (the adult dimensions are the entity type's).
    fn baby_dimensions(&self) -> EntityDimensions {
        let (eye_height, vehicle_y) = match self.kind {
            ZombieKind::Husk => (0.825, 0.1875),
            ZombieKind::ZombieVillager(_) => (0.67, 0.125),
            ZombieKind::ZombifiedPiglin(_) => (0.78, 0.1875),
            _ => (0.775, 0.1875),
        };
        EntityDimensions::scalable(0.49, 0.98).with_eye_height(eye_height).with_attachments(EntityAttachments::create_default(0.49, 0.98).with_vehicle(Vec3::new(0.0, vehicle_y, 0.0)))
    }

    /// `setCanBreakDoors(canBreakDoors)`.
    pub fn set_can_break_doors(&mut self, can_break_doors: bool) {
        base::set_can_break_doors(self, can_break_doors);
    }

    /// `setInWaterTime(inWaterTime)` (`@VisibleForTesting`).
    pub fn set_in_water_time(&mut self, in_water_time: i32) {
        self.drowning_tracker.set_affliction_time(in_water_time);
    }

    /// `setConversionTime(conversionTime)` (`@VisibleForTesting`).
    pub fn set_conversion_time(&mut self, conversion_time: i32) {
        self.drowning_tracker.set_conversion_time(conversion_time);
    }

    /// `handleAttributes(difficultyModifier, spawnReason)`.
    pub fn handle_attributes(&mut self, difficulty_modifier: f32, spawn_reason: EntitySpawnReason) {
        base::handle_attributes(self, difficulty_modifier, spawn_reason);
    }

    /// `convertVillagerToZombieVillager(level, villager)`.
    pub fn convert_villager_to_zombie_villager(&mut self, level: &mut ServerLevel, villager: &mut dyn Entity) -> bool {
        zombie_villager::convert_villager_to_zombie_villager(self, level, villager)
    }
}

impl Entity for Zombie {
    crate::mob_entity_accessors!(mob);

    fn as_zombie(&self) -> Option<&Zombie> {
        Some(self)
    }

    fn as_zombie_mut(&mut self) -> Option<&mut Zombie> {
        Some(self)
    }

    fn get_dimensions(&self, _pose: Pose) -> EntityDimensions {
        if self.is_baby() { self.baby_dimensions() } else { EntityDimensions::of_type(self.mob.living.entity.entity_type) }
    }

    fn is_baby(&self) -> bool {
        self.mob.living.entity.entity_data.get(DATA_BABY_ID)
    }

    fn set_baby(&mut self, baby: bool) {
        base::set_baby(self, baby);
    }

    fn get_base_experience_reward(&mut self, level: &ServerLevel) -> i32 {
        base::get_base_experience_reward(self, level)
    }

    fn on_synced_data_updated(&mut self, accessor_id: u8) {
        if accessor_id == DATA_BABY_ID.id() {
            self.refresh_dimensions();
        }
        if accessor_id == crate::world::entity::entity::DATA_POSE.id() {
            self.refresh_dimensions();
        }
    }

    fn tick(&mut self, level: &mut ServerLevel) {
        base::tick(self, level);
    }

    fn hurt_server(&mut self, level: &mut ServerLevel, source: &DamageSource, amount: f32) -> bool {
        base::hurt_server(self, level, source, amount)
    }

    fn do_hurt_target(&mut self, level: &mut ServerLevel, target: &mut dyn Entity) -> bool {
        base::do_hurt_target(self, level, target)
    }

    fn killed_entity(&mut self, level: &mut ServerLevel, victim: &DamageEntity, source: &DamageSource) -> bool {
        base::killed_entity(self, level, victim, source)
    }

    fn get_ambient_sound(&self) -> Option<SoundEvent> {
        Some(match &self.kind {
            ZombieKind::Zombie => SoundEvent::ENTITY_ZOMBIE_AMBIENT,
            ZombieKind::Husk => SoundEvent::ENTITY_HUSK_AMBIENT,
            ZombieKind::Drowned(_) => {
                if self.is_in_water() { SoundEvent::ENTITY_DROWNED_AMBIENT_WATER } else { SoundEvent::ENTITY_DROWNED_AMBIENT }
            }
            ZombieKind::ZombieVillager(_) => SoundEvent::ENTITY_ZOMBIE_VILLAGER_AMBIENT,
            // the angry variant is chosen by `play_ambient_sound` (`isAngry()` needs the game time)
            ZombieKind::ZombifiedPiglin(_) => SoundEvent::ENTITY_ZOMBIFIED_PIGLIN_AMBIENT,
        })
    }

    fn play_ambient_sound(&mut self, level: &mut ServerLevel) {
        let sound = match self.kind {
            ZombieKind::ZombifiedPiglin(_) if mob::neutral::is_angry(self, level) => Some(SoundEvent::ENTITY_ZOMBIFIED_PIGLIN_ANGRY),
            _ => self.get_ambient_sound(),
        };
        if let Some(sound) = sound {
            crate::world::entity::living_entity::damage::make_sound(self, level, sound);
        }
    }

    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        Some(match &self.kind {
            ZombieKind::Zombie => SoundEvent::ENTITY_ZOMBIE_HURT,
            ZombieKind::Husk => SoundEvent::ENTITY_HUSK_HURT,
            ZombieKind::Drowned(_) => {
                if self.is_in_water() { SoundEvent::ENTITY_DROWNED_HURT_WATER } else { SoundEvent::ENTITY_DROWNED_HURT }
            }
            ZombieKind::ZombieVillager(_) => SoundEvent::ENTITY_ZOMBIE_VILLAGER_HURT,
            ZombieKind::ZombifiedPiglin(_) => SoundEvent::ENTITY_ZOMBIFIED_PIGLIN_HURT,
        })
    }

    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(match &self.kind {
            ZombieKind::Zombie => SoundEvent::ENTITY_ZOMBIE_DEATH,
            ZombieKind::Husk => SoundEvent::ENTITY_HUSK_DEATH,
            ZombieKind::Drowned(_) => {
                if self.is_in_water() { SoundEvent::ENTITY_DROWNED_DEATH_WATER } else { SoundEvent::ENTITY_DROWNED_DEATH }
            }
            ZombieKind::ZombieVillager(_) => SoundEvent::ENTITY_ZOMBIE_VILLAGER_DEATH,
            ZombieKind::ZombifiedPiglin(_) => SoundEvent::ENTITY_ZOMBIFIED_PIGLIN_DEATH,
        })
    }

    fn get_swim_sound(&self) -> SoundEvent {
        match self.kind {
            ZombieKind::Drowned(_) => SoundEvent::ENTITY_DROWNED_SWIM,
            _ => SoundEvent::ENTITY_HOSTILE_SWIM,
        }
    }

    fn play_step_sound(&mut self, level: &mut ServerLevel, _pos: BlockPos, _state: mc_data::block::BlockState) {
        let sound = self.get_step_sound();
        self.play_sound(level, sound, 0.15, 1.0);
    }

    fn get_voice_pitch(&mut self) -> f32 {
        match self.kind {
            ZombieKind::ZombieVillager(_) => zombie_villager::get_voice_pitch(self),
            _ => crate::world::entity::living_entity::damage::voice_pitch(self),
        }
    }

    fn populate_default_equipment_slots(&mut self, random: &mut dyn RandomSource, difficulty: &DifficultyInstance) {
        match self.kind {
            ZombieKind::Drowned(_) => drowned::populate_default_equipment_slots(self, random, difficulty),
            ZombieKind::ZombifiedPiglin(_) => zombified_piglin::populate_default_equipment_slots(self, random, difficulty),
            _ => base::populate_default_equipment_slots(self, random, difficulty),
        }
    }

    fn can_hold_item(&self, stack: &ItemStack) -> bool {
        base::can_hold_item(self, stack)
    }

    fn wants_to_pick_up(&self, level: &ServerLevel, stack: &ItemStack) -> bool {
        match self.kind {
            ZombieKind::ZombifiedPiglin(_) => self.can_hold_item(stack),
            ZombieKind::Drowned(_) => {
                if base::item_is_in_tag(stack, "spears") { false } else { base::wants_to_pick_up(self, level, stack) }
            }
            _ => base::wants_to_pick_up(self, level, stack),
        }
    }

    fn get_preferred_weapon_type(&self) -> Option<&'static str> {
        match self.kind {
            ZombieKind::Drowned(_) => Some("drowned_preferred_weapons"),
            _ => None,
        }
    }

    fn can_replace_current_item(&self, level: &ServerLevel, new: &ItemStack, current: &ItemStack, slot: crate::world::entity::equipment::EquipmentSlot) -> bool {
        match self.kind {
            ZombieKind::Drowned(_) if current.is(mc_data::Item::NAUTILUS_SHELL) => false,
            _ => mob::equipment::can_replace_current_item(self, level, new, current, slot),
        }
    }

    fn finalize_spawn(
        &mut self,
        level: &mut dyn ServerLevelAccessor,
        difficulty: DifficultyInstance,
        reason: EntitySpawnReason,
        group_data: Option<Box<dyn SpawnGroupData>>,
    ) -> Option<Box<dyn SpawnGroupData>> {
        match self.kind {
            ZombieKind::Husk => husk::finalize_spawn(self, level, difficulty, reason, group_data),
            ZombieKind::Drowned(_) => drowned::finalize_spawn(self, level, difficulty, reason, group_data),
            ZombieKind::ZombieVillager(_) => zombie_villager::finalize_spawn(self, level, difficulty, reason, group_data),
            _ => base::finalize_spawn(self, level, difficulty, reason, group_data),
        }
    }

    fn on_offspring_spawned_from_egg(&mut self, level: &mut ServerLevel, _spawner: &mut crate::world::entity::ServerPlayer, offspring: EntityId) {
        base::on_offspring_spawned_from_egg(self, level, offspring);
    }

    fn check_spawn_obstruction(&self, level: &dyn ServerLevelAccessor) -> bool {
        match self.kind {
            ZombieKind::Drowned(_) => level.is_unobstructed_entity(self),
            ZombieKind::ZombifiedPiglin(_) => level.is_unobstructed_entity(self) && !level.contains_any_liquid(&self.base().bounding_box()),
            _ => mob::spawn::check_spawn_obstruction(self, level),
        }
    }

    fn remove_when_far_away(&self, distance_to_closest_player: f64) -> bool {
        match self.kind {
            ZombieKind::ZombieVillager(_) => zombie_villager::remove_when_far_away(self),
            _ => {
                let _ = distance_to_closest_player;
                true
            }
        }
    }

    fn custom_server_ai_step(&mut self, level: &mut ServerLevel) {
        if let ZombieKind::ZombifiedPiglin(_) = self.kind {
            zombified_piglin::custom_server_ai_step(self, level);
        }
    }

    fn set_target(&mut self, level: &ServerLevel, target: Option<EntityId>) {
        if let ZombieKind::ZombifiedPiglin(_) = self.kind {
            zombified_piglin::set_target(self, level, target);
        } else {
            mob::tick::set_target(self, level, target);
        }
    }

    fn start_persistent_anger_timer(&mut self, level: &mut ServerLevel) {
        if let ZombieKind::ZombifiedPiglin(_) = self.kind {
            zombified_piglin::start_persistent_anger_timer(self, level);
        }
    }

    fn is_preventing_player_rest(&self, level: &ServerLevel, player: &dyn Entity) -> bool {
        match self.kind {
            ZombieKind::ZombifiedPiglin(_) => mob::neutral::is_angry_at(self, level, player),
            _ => true,
        }
    }

    fn mob_interact(&mut self, level: &mut ServerLevel, player: &mut crate::world::entity::ServerPlayer, hand: crate::world::item::InteractionHand) -> crate::world::item::InteractionResult {
        match self.kind {
            ZombieKind::ZombieVillager(_) => zombie_villager::mob_interact(self, level, player, hand),
            _ => mob::interact::mob_interact(self, level, player, hand),
        }
    }

    fn is_pushed_by_fluid(&self) -> bool {
        match self.kind {
            ZombieKind::Drowned(_) => !self.is_swimming(),
            _ => true,
        }
    }

    fn travel_in_water(&mut self, level: &mut ServerLevel, input: Vec3, base_gravity: f64, is_falling: bool, old_y: f64) {
        match self.kind {
            ZombieKind::Drowned(_) => drowned::travel_in_water(self, level, input, base_gravity, is_falling, old_y),
            _ => crate::world::entity::living_entity::travel::travel_in_water(self, level, input, base_gravity, is_falling, old_y),
        }
    }

    fn update_swimming(&mut self, level: &ServerLevel) {
        match self.kind {
            ZombieKind::Drowned(_) => drowned::update_swimming(self, level),
            _ => base::entity_update_swimming(self, level),
        }
    }

    fn is_visually_swimming(&self) -> bool {
        match self.kind {
            ZombieKind::Drowned(_) => self.is_swimming() && !self.base().is_passenger(),
            _ => self.base().get_pose() == Pose::Swimming,
        }
    }

    fn conversion_common_extra(&self, to: &mut dyn Entity) {
        if self.can_break_doors {
            if let Some(zombie) = to.as_zombie_mut() {
                zombie.set_can_break_doors(true);
            }
        }
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        base::add_additional_save_data(self, out);
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        base::read_additional_save_data(self, input);
    }
}

/// `EntityType.ZOMBIE` & co.
pub fn register() {}

macro_rules! register_zombie {
    ($name:ident, $ty:ident) => {
        #[distributed_slice(ENTITY_FACTORIES)]
        static $name: EntityRegistration = EntityRegistration { entity_type: EntityType::$ty, create: |ty, id, registries| Box::new(Zombie::new(ty, id, registries)) };
    };
}

register_zombie!(ZOMBIE, ZOMBIE);
register_zombie!(HUSK, HUSK);
register_zombie!(DROWNED, DROWNED);
register_zombie!(ZOMBIE_VILLAGER, ZOMBIE_VILLAGER);
register_zombie!(ZOMBIFIED_PIGLIN, ZOMBIFIED_PIGLIN);
