//! `net.minecraft.world.entity.Mob` and its generic subclasses: `PathfinderMob`, `AgeableMob`, `Animal`,
//! `TamableAnimal`, `Monster`, plus the `NeutralMob` / `OwnableEntity` / `Leashable` interfaces.
//!
//! # How to define a concrete mob (phase-2 author guide)
//!
//! 1. **Struct layout.** Embed [`MobData`] (which embeds `LivingEntityData` ⊂ `EntityData`) plus the fields vanilla's concrete
//!    class adds:
//!    ```ignore
//!    pub struct Chicken { pub mob: MobData, pub flap: f32, /* ... */ }
//!    ```
//!    Build the `MobData` with [`MobData::new`]`(entity_type, id, MobKind::Animal, |b| { /* synched data of THIS class */ })`;
//!    the generic layers (`DATA_MOB_FLAGS_ID`, baby/age-locked flags, tamable flags/owner) are defined for you in vanilla's
//!    order *before* your closure runs. Declare your own accessors with `define_accessor!(DATA_X: Serializer = ClassName)`.
//!    Mob classes of an intermediate Java class (`Zombie` ⊂ `Monster`, `Drowned` ⊂ `Zombie`) write the same: the leaf class
//!    embeds `MobData` and its own parent-class struct fields; "super" calls are plain function calls (see 3).
//! 2. **Entity impl.** `impl Entity for Chicken { crate::mob_entity_accessors!(mob); ... }` — the macro writes `base`,
//!    `base_mut`, `as_any*`, `as_dyn*`, `living*`, `mob`, `mob_mut`. Everything else has a vanilla default: the trait
//!    defaults dispatch on `self.mob()` to this module (tick, aiStep, serverAiStep, checkDespawn, NBT of every generic
//!    layer, interaction, drops, persistence, `getTarget`, `isBaby`, ...). Override only what the Java class overrides.
//! 3. **Calling super.** An override that extends the parent behavior calls the module function of the layer it extends:
//!    [`tick::ai_step`] (= `Animal.aiStep()` chain for any mob; it runs `LivingEntity.aiStep`, then `Mob`, then the
//!    `AgeableMob`/`Animal`/`Monster` additions that apply to this mob), [`nbt::add_additional_save_data`] /
//!    [`nbt::read_additional_save_data`], [`spawn::finalize_spawn`], [`interact::mob_interact`], ... Intermediate concrete
//!    classes expose their own such functions (`zombie::ai_step`), so the chain is explicit.
//! 3b. **Virtual hooks** (all `trait Entity` methods with vanilla snake_case names): `get_ambient_sound`,
//!    `get_ambient_sound_interval`, `custom_server_ai_step`, `get_max_head_x_rot` / `get_max_head_y_rot` /
//!    `get_head_rot_speed`, `can_hold_item`, `wants_to_pick_up`, `remove_when_far_away`, `requires_custom_persistence`,
//!    `populate_default_equipment_slots`, `mob_interact`, `get_walk_target_value`, `is_food`, `get_breed_offspring`,
//!    `can_be_a_baby`, `get_base_experience_reward`, `can_fall_in_love`, ...
//! 4. **Factory registration + attributes.** Register the constructor in `ENTITY_FACTORIES` (`world/entity/factory.rs`):
//!    `#[distributed_slice(ENTITY_FACTORIES)] static CHICKEN: EntityRegistration = EntityRegistration { entity_type:
//!    EntityType::CHICKEN, create: |ty, id, registries| Box::new(Chicken::new(ty, id, registries)) };`
//!    Default attributes (`createAttributes()`) are read from `mc_data` per entity type by `AttributeMap::new`, so there is
//!    nothing to register; a class that adds attribute *instances* conditionally edits `mob.living.attributes` in its ctor.
//! 5. **Constructor order (= `Mob.<init>`):** `MobData::new` (RNG: yRot) → class-specific ctor body (`eggTime = random...`)
//!    in vanilla's statement order → [`MobData::set_navigation`] (`createNavigation`) and any control replacement
//!    (`mob.mob.move_control = Box::new(FlyingMoveControl::new(..))`) → `register_goals(&mut self)` which adds goals to
//!    `mob.goal_selector` / `mob.target_selector` (`addGoal(prio, Box::new(Goal::new(..)))`; goal constructors that touch the
//!    navigation, like `FloatGoal`, take `&mut dyn Entity`).
//! 6. **Goals.** Implement [`ai::goal::Goal`](crate::world::entity::ai::goal::Goal): `(mob: &mut dyn Entity, level: &mut
//!    ServerLevel)` on every callback. Downcast with `mob.as_any().downcast_ref::<Chicken>()` for class-specific state.
//!    Navigation / controls: `navigation::move_to(mob, level, x, y, z, speed)`, `mob.mob_mut().unwrap().look_control_mut()
//!    .set_look_at_xyz(...)`; `mob::set_wanted_position`, [`MobData::jump_control`].
//! 7. **Other entities / level.** `level.get_entity(id)` (the ticking mob itself is taken out of the level while it runs),
//!    `level.get_entities(Some(self_id), &aabb, |e| ...)`, `level.get_nearest_player(..)`, [`ext::get_nearby_entities`] for
//!    `TargetingConditions` queries. Entities are referenced by `EntityId` / `Uuid`, never by pointer.
//! 8. **Children / projectiles / drops.** `animal::spawn_child_from_breeding`, `level.add_fresh_entity(Box::new(..))`,
//!    `crate::world::entity::item::spawn_at_location(level, self, stack, offset)`, [`ext::drop_from_gift_loot_table`].
//! 9. **Sounds.** `entity.play_sound(level, SoundEvent::X, volume, pitch)`; override `get_ambient_sound`, `get_hurt_sound`,
//!    `get_death_sound`, `play_step_sound`, `get_fall_sounds`.
//! 10. **Spawning.** `finalize_spawn` is the `Entity::finalize_spawn` hook (override and call [`spawn::finalize_spawn`]);
//!     spawn predicates are registered with SpawnFramework's `register_spawn_placement!`.

pub mod ageable;
pub mod animal;
pub mod convert;
pub mod drop_chances;
pub mod equipment;
pub mod ext;
pub mod interact;
pub mod leashable;
pub mod monster;
pub mod navigation;
pub mod nbt;
pub mod neutral;
pub mod pathfinder;
pub mod spawn;
pub mod tamable;
pub mod tick;

pub use ageable::AgeableData;
pub use animal::AnimalData;
pub use convert::{ConversionParams, ConversionType};
pub use drop_chances::DropChances;
pub use leashable::LeashData;
pub use neutral::NeutralData;
pub use tamable::TamableData;

use super::EntityId;
use super::ai::control::{
    BodyRotationControl, JumpControl, LookControl, MoveControl, body_rotation_control::StandardBodyRotationControl, jump_control::StandardJumpControl,
    look_control::StandardLookControl, move_control::StandardMoveControl,
};
use super::ai::goal::GoalSelector;
use super::ai::navigation::PathNavigation;
use super::ai::sensing::Sensing;
use super::entity::{Entity, EntityData};
use super::living::LivingEntityData;
use super::synched_data::SynchedEntityDataBuilder;
use crate::define_accessor;
use crate::server::level::ServerLevel;
use crate::world::level::pathfinder::{PathType, PathfindingMalus};
use mc_core::ident::Identifier;
use mc_core::pos::BlockPos;
use mc_data::{Attribute, EntityType};

pub const MOB_FLAG_NO_AI: i32 = 1;
pub const MOB_FLAG_LEFTHANDED: i32 = 2;
pub const MOB_FLAG_AGGRESSIVE: i32 = 4;
pub const PICKUP_REACH: i32 = 1;
pub const MAX_WEARING_ARMOR_CHANCE: f32 = 0.15;
pub const WEARING_ARMOR_UPGRADE_MATERIAL_CHANCE: f32 = 0.1087;
pub const WEARING_ARMOR_UPGRADE_MATERIAL_ATTEMPTS: f32 = 3.0;
pub const MAX_PICKUP_LOOT_CHANCE: f32 = 0.55;
pub const MAX_ENCHANTED_ARMOR_CHANCE: f32 = 0.5;
pub const MAX_ENCHANTED_WEAPON_CHANCE: f32 = 0.25;
pub const UPDATE_GOAL_SELECTOR_EVERY_N_TICKS: i32 = 2;

define_accessor!(pub DATA_MOB_FLAGS_ID: Byte = Mob);
define_accessor!(pub DATA_BABY_ID: Boolean = AgeableMob);
define_accessor!(pub AGE_LOCKED: Boolean = AgeableMob);
define_accessor!(pub DATA_FLAGS_ID: Byte = TamableAnimal);
define_accessor!(pub DATA_OWNERUUID_ID: OptionalLivingEntityReference = TamableAnimal);

/// Which generic vanilla superclass chain a mob belongs to (`Mob` ⊂ `PathfinderMob` ⊂ `AgeableMob` ⊂ `Animal` ⊂
/// `TamableAnimal`; `PathfinderMob` ⊂ `Monster`). Decides which layers of [`MobData`] exist.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MobKind {
    /// Plain `Mob` (ghast, slime, ...).
    Mob,
    /// `PathfinderMob` (fish, creeper through `Monster`'s siblings, golems, ...).
    PathfinderMob,
    /// `AgeableMob` (villagers).
    AgeableMob,
    /// `Animal`.
    Animal,
    /// `TamableAnimal`.
    TamableAnimal,
    /// `Monster` (`Enemy`).
    Monster,
}

impl MobKind {
    pub fn is_pathfinder(self) -> bool {
        self != MobKind::Mob
    }
    pub fn is_ageable(self) -> bool {
        matches!(self, MobKind::AgeableMob | MobKind::Animal | MobKind::TamableAnimal)
    }
    pub fn is_animal(self) -> bool {
        matches!(self, MobKind::Animal | MobKind::TamableAnimal)
    }
}

/// Fields of vanilla `Mob` (+ the data of the generic subclasses, present per [`MobKind`]).
pub struct MobData {
    pub living: LivingEntityData,
    pub kind: MobKind,
    /// `Enemy` marker interface (`Monster`, slimes, ghasts, phantoms, ...): `canBeLeashed()` is false for enemies.
    pub enemy: bool,
    pub ambient_sound_time: i32,
    pub xp_reward: i32,
    pub look_control: Option<Box<dyn LookControl>>,
    pub move_control: Option<Box<dyn MoveControl>>,
    pub jump_control: Option<Box<dyn JumpControl>>,
    pub body_rotation_control: Option<Box<dyn BodyRotationControl>>,
    /// `navigation`: `mem::take`n while it ticks / is used (see [`navigation`]).
    pub navigation: Option<Box<dyn PathNavigation>>,
    pub goal_selector: GoalSelector,
    pub target_selector: GoalSelector,
    /// `target` (unchecked, see `Entity::get_target`).
    pub target: Option<EntityId>,
    pub sensing: Sensing,
    pub drop_chances: DropChances,
    pub can_pick_up_loot: bool,
    pub persistence_required: bool,
    pub pathfinding_malus: PathfindingMalus,
    pub loot_table: Option<Identifier>,
    pub loot_table_seed: i64,
    pub leash_data: Option<LeashData>,
    pub home_position: BlockPos,
    pub home_radius: i32,
    /// `getFirstPassenger() instanceof Mob`, maintained by `add_passenger`/`remove_passenger` (the controlling-passenger and
    /// body-rotation checks of vanilla need the passenger's class without a level).
    pub first_passenger_is_mob: bool,
    pub ageable: Option<AgeableData>,
    pub animal: Option<AnimalData>,
    pub tamable: Option<TamableData>,
    /// `NeutralMob` state (wolves, bees, piglins, ...), present for mobs that implement the interface.
    pub neutral: Option<NeutralData>,
}

impl MobData {
    /// `Mob(type, level)` without `createNavigation()` / `registerGoals()` (see the module docs, step 5).
    /// `define_synched_data` defines the concrete class's accessors after all generic layers.
    pub fn new(entity_type: EntityType, id: EntityId, kind: MobKind, define_synched_data: impl FnOnce(&mut SynchedEntityDataBuilder)) -> Self {
        let living = LivingEntityData::new(entity_type, id, super::entity::TOTAL_AIR_SUPPLY, |b| {
            b.define(DATA_MOB_FLAGS_ID, 0);
            if kind.is_ageable() {
                b.define(DATA_BABY_ID, false);
                b.define(AGE_LOCKED, false);
            }
            if kind == MobKind::TamableAnimal {
                b.define(DATA_FLAGS_ID, 0);
                b.define(DATA_OWNERUUID_ID, None);
            }
            define_synched_data(b);
        });
        let mut data = Self {
            living,
            kind,
            enemy: kind == MobKind::Monster,
            ambient_sound_time: 0,
            xp_reward: if kind == MobKind::Monster { 5 } else { 0 },
            look_control: Some(Box::<StandardLookControl>::default()),
            move_control: Some(Box::<StandardMoveControl>::default()),
            jump_control: Some(Box::<StandardJumpControl>::default()),
            body_rotation_control: Some(Box::<StandardBodyRotationControl>::default()),
            navigation: None,
            goal_selector: GoalSelector::new(),
            target_selector: GoalSelector::new(),
            target: None,
            sensing: Sensing::default(),
            drop_chances: DropChances::DEFAULT,
            can_pick_up_loot: false,
            persistence_required: false,
            pathfinding_malus: PathfindingMalus::default(),
            loot_table: None,
            loot_table_seed: 0,
            leash_data: None,
            home_position: BlockPos::ZERO,
            home_radius: -1,
            first_passenger_is_mob: false,
            ageable: kind.is_ageable().then(AgeableData::default),
            animal: kind.is_animal().then(AnimalData::default),
            tamable: (kind == MobKind::TamableAnimal).then(TamableData::default),
            neutral: None,
        };
        if kind.is_animal() {
            // `Animal.<init>`
            data.set_pathfinding_malus(PathType::FireInNeighbor, 16.0);
            data.set_pathfinding_malus(PathType::Fire, -1.0);
        }
        data
    }

    pub fn entity(&self) -> &EntityData {
        &self.living.entity
    }

    pub fn entity_mut(&mut self) -> &mut EntityData {
        &mut self.living.entity
    }

    /// `createNavigation(level)` result for this mob.
    pub fn set_navigation(&mut self, navigation: Box<dyn PathNavigation>) {
        self.navigation = Some(navigation);
    }

    pub fn look_control_mut(&mut self) -> &mut dyn LookControl {
        self.look_control.as_deref_mut().expect("look control is only taken while it ticks")
    }

    pub fn move_control_mut(&mut self) -> &mut dyn MoveControl {
        self.move_control.as_deref_mut().expect("move control is only taken while it ticks")
    }

    pub fn jump_control_mut(&mut self) -> &mut dyn JumpControl {
        self.jump_control.as_deref_mut().expect("jump control is only taken while it ticks")
    }

    /// `setPathfindingMalus(type, cost)`.
    pub fn set_pathfinding_malus(&mut self, ty: PathType, cost: f32) {
        self.pathfinding_malus.set(ty, cost);
    }

    /// `hasHome()`.
    pub fn has_home(&self) -> bool {
        self.home_radius != -1
    }

    /// `isWithinHome(pos)`.
    pub fn is_within_home_pos(&self, pos: BlockPos) -> bool {
        self.home_radius == -1 || self.home_position.dist_sqr(&pos) < f64::from(self.home_radius.wrapping_mul(self.home_radius))
    }

    /// `isWithinHome(Vec3)`.
    pub fn is_within_home_vec(&self, pos: mc_core::phys::Vec3) -> bool {
        self.home_radius == -1 || self.home_position.dist_to_center_sqr(&pos) < f64::from(self.home_radius.wrapping_mul(self.home_radius))
    }

    /// `setHomeTo(newCenter, radius)`.
    pub fn set_home_to(&mut self, new_center: BlockPos, radius: i32) {
        self.home_position = new_center;
        self.home_radius = radius;
    }

    /// `clearHome()`.
    pub fn clear_home(&mut self) {
        self.home_radius = -1;
    }

    pub fn get_mob_flag(&self, flag: i32) -> bool {
        (self.living.entity.entity_data.get(DATA_MOB_FLAGS_ID) as i32 & flag) != 0
    }

    pub fn set_mob_flag(&mut self, flag: i32, value: bool) {
        let cur = self.living.entity.entity_data.get(DATA_MOB_FLAGS_ID) as i32;
        let new = if value { cur | flag } else { cur & !flag };
        self.living.entity.entity_data.set(DATA_MOB_FLAGS_ID, new as i8);
    }

    /// `isNoAi()`.
    pub fn is_no_ai(&self) -> bool {
        self.get_mob_flag(MOB_FLAG_NO_AI)
    }

    pub fn set_no_ai(&mut self, flag: bool) {
        self.set_mob_flag(MOB_FLAG_NO_AI, flag);
    }

    pub fn is_left_handed(&self) -> bool {
        self.get_mob_flag(MOB_FLAG_LEFTHANDED)
    }

    pub fn set_left_handed(&mut self, flag: bool) {
        self.set_mob_flag(MOB_FLAG_LEFTHANDED, flag);
    }

    pub fn is_aggressive(&self) -> bool {
        self.get_mob_flag(MOB_FLAG_AGGRESSIVE)
    }

    pub fn set_aggressive(&mut self, flag: bool) {
        self.set_mob_flag(MOB_FLAG_AGGRESSIVE, flag);
    }
}

/// `getAttributeValue(attribute)` of a living mob.
pub fn attribute_value(mob: &dyn Entity, attribute: Attribute) -> f64 {
    mob.living().expect("living mob").get_attribute_value(attribute)
}

/// `setZza(zza)`.
pub fn set_zza(mob: &mut dyn Entity, zza: f32) {
    if let Some(l) = mob.living_mut() {
        l.zza = zza;
    }
}

/// `setYya(yya)`.
pub fn set_yya(mob: &mut dyn Entity, yya: f32) {
    if let Some(l) = mob.living_mut() {
        l.yya = yya;
    }
}

/// `setXxa(xxa)`.
pub fn set_xxa(mob: &mut dyn Entity, xxa: f32) {
    if let Some(l) = mob.living_mut() {
        l.xxa = xxa;
    }
}

/// `getLookControl().setLookAt(x, y, z)` (the mob's `getHeadRotSpeed()` / `getMaxHeadXRot()`).
pub fn look_at_xyz(mob: &mut dyn Entity, x: f64, y: f64, z: f64) {
    let (y_speed, x_max) = (mob.get_head_rot_speed() as f32, mob.get_max_head_x_rot() as f32);
    if let Some(m) = mob.mob_mut() {
        m.look_control_mut().set_look_at_full(x, y, z, y_speed, x_max);
    }
}

/// `getLookControl().setLookAt(target)` (`target.getEyeY()`).
pub fn look_at_entity(mob: &mut dyn Entity, target: &dyn Entity) {
    look_at_xyz(mob, target.base().x(), target.base().get_eye_y(), target.base().z());
}

/// `getLookControl().setLookAt(target, yMaxRotSpeed, xMaxRotAngle)`.
pub fn look_at_entity_full(mob: &mut dyn Entity, target: &dyn Entity, y_max_rot_speed: f32, x_max_rot_angle: f32) {
    if let Some(m) = mob.mob_mut() {
        m.look_control_mut().set_look_at_entity_full(target, y_max_rot_speed, x_max_rot_angle);
    }
}

/// `getJumpControl().jump()`.
pub fn jump_control_jump(mob: &mut dyn Entity) {
    if let Some(m) = mob.mob_mut() {
        if let Some(j) = m.jump_control.as_deref_mut() {
            j.jump();
        }
    }
}

/// `getPathfindingMalus(pathType)`: the controlled vehicle's malus map when it is a `Mob` that
/// `shouldPassengersInheritMalus()`, else this mob's, falling back to the type's own malus.
pub fn get_pathfinding_malus(mob: &dyn Entity, level: &ServerLevel, ty: PathType) -> f32 {
    let mut inherit_from = mob;
    if let Some(vehicle) = navigation::controlled_vehicle(mob, level).and_then(|id| level.get_entity(id)) {
        if vehicle.mob().is_some() && vehicle.should_passengers_inherit_malus() {
            inherit_from = vehicle;
        }
    }
    inherit_from.mob().and_then(|m| m.pathfinding_malus.get(ty)).unwrap_or_else(|| ty.get_malus())
}

/// `setPathfindingMalus(type, cost)`.
pub fn set_pathfinding_malus(mob: &mut dyn Entity, ty: PathType, cost: f32) {
    if let Some(m) = mob.mob_mut() {
        m.set_pathfinding_malus(ty, cost);
    }
}

/// `getMoveControl().setWantedPosition(x, y, z, speedModifier)` (own control; use [`navigation`] helpers for the vehicle
/// redirect of `getMoveControl()`).
pub fn set_wanted_position(mob: &mut dyn Entity, x: f64, y: f64, z: f64, speed_modifier: f64) {
    if let Some(m) = mob.mob_mut() {
        if let Some(c) = m.move_control.as_deref_mut() {
            c.set_wanted_position(x, y, z, speed_modifier);
        }
    }
}

/// `getMoveControl().getSpeedModifier()`.
pub fn move_control_speed_modifier(mob: &dyn Entity) -> f64 {
    mob.mob().and_then(|m| m.move_control.as_deref()).map_or(0.0, |c| c.get_speed_modifier())
}

/// Implements the structural `Entity` accessors for a concrete mob type whose `MobData` is the field `$mob`.
#[macro_export]
macro_rules! mob_entity_accessors {
    ($mob:ident) => {
        fn base(&self) -> &$crate::world::entity::EntityData {
            &self.$mob.living.entity
        }
        fn base_mut(&mut self) -> &mut $crate::world::entity::EntityData {
            &mut self.$mob.living.entity
        }
        fn as_any(&self) -> &dyn ::std::any::Any {
            self
        }
        fn as_any_mut(&mut self) -> &mut dyn ::std::any::Any {
            self
        }
        fn as_dyn(&self) -> &dyn $crate::world::entity::Entity {
            self
        }
        fn as_dyn_mut(&mut self) -> &mut dyn $crate::world::entity::Entity {
            self
        }
        fn living(&self) -> Option<&$crate::world::entity::living::LivingEntityData> {
            Some(&self.$mob.living)
        }
        fn living_mut(&mut self) -> Option<&mut $crate::world::entity::living::LivingEntityData> {
            Some(&mut self.$mob.living)
        }
        fn mob(&self) -> Option<&$crate::world::entity::mob::MobData> {
            Some(&self.$mob)
        }
        fn mob_mut(&mut self) -> Option<&mut $crate::world::entity::mob::MobData> {
            Some(&mut self.$mob)
        }
    };
}
