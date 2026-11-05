//! `net.minecraft.world.entity.monster.Vex` (the evoker's summoned vexes).

use crate::define_accessor;
use crate::registry::RegistryAccess;
use crate::server::level::ServerLevel;
use crate::world::damagesource::DamageSource;
use crate::world::difficulty_instance::DifficultyInstance;
use crate::world::entity::ai::control::move_control::{MoveControl, MoveControlBase, Operation};
use crate::world::entity::ai::goal::look_at_player::LookAtType;
use crate::world::entity::ai::goal::target::{HurtByTargetGoal, NearestAttackableTargetGoal, TargetGoal, TargetType};
use crate::world::entity::ai::goal::{FloatGoal, Flag, Goal, GoalFlags, LookAtPlayerGoal, reduced_tick_delay};
use crate::world::entity::ai::navigation::StandardPathNavigation;
use crate::world::entity::ai::targeting::TargetingConditions;
use crate::world::entity::equipment::EquipmentSlot;
use crate::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use crate::world::entity::mob::{self, MobData, MobKind};
use crate::world::entity::{Entity, EntityId};
use crate::world::item::ItemStack;
use crate::world::level::server_level_accessor::ServerLevelAccessor;
use linkme::distributed_slice;
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::enums::EntitySpawnReason;
use mc_data::{EntityType, Item, SoundEvent};
use mc_nbt::CompoundTag;
use std::sync::Arc;
use uuid::Uuid;

define_accessor!(pub DATA_FLAGS_ID: Byte = Vex);

pub const FLAP_DEGREES_PER_TICK: f32 = 45.836624;
/// `Mth.ceil((float)(Math.PI * 5.0 / 4.0))`.
pub const TICKS_PER_FLAP: i32 = 4;
const FLAG_IS_CHARGING: i32 = 1;

/// `Vex`.
pub struct Vex {
    pub mob: MobData,
    /// `owner` (`EntityReference<LivingEntity>`).
    owner: Option<Uuid>,
    bound_origin: Option<BlockPos>,
    has_limited_life: bool,
    limited_life_ticks: i32,
}

impl Vex {
    /// `new Vex(type, level)`.
    pub fn new(ty: EntityType, id: EntityId, _registries: &RegistryAccess) -> Self {
        let mob = MobData::new(ty, id, MobKind::Monster, |b| {
            b.define(DATA_FLAGS_ID, 0);
        });
        let mut vex = Vex { mob, owner: None, bound_origin: None, has_limited_life: false, limited_life_ticks: 0 };
        let navigation = StandardPathNavigation::ground(&vex);
        vex.mob.set_navigation(Box::new(navigation));
        vex.mob.move_control = Some(Box::new(VexMoveControl { base: MoveControlBase::default() }));
        vex.mob.xp_reward = 3;
        vex.register_goals();
        vex
    }

    /// `registerGoals()` (`super.registerGoals()` is `Mob`'s empty one).
    fn register_goals(&mut self) {
        let float = FloatGoal::new(self);
        let look_player = LookAtPlayerGoal::with_probability(self, LookAtType::Player, 3.0, 1.0);
        let look_mob = LookAtPlayerGoal::new(self, LookAtType::Class(Arc::new(|e: &dyn Entity| e.mob().is_some())), 8.0);
        let goals = &mut self.mob.goal_selector;
        goals.add_goal(0, Box::new(float));
        goals.add_goal(4, Box::new(VexChargeAttackGoal));
        goals.add_goal(8, Box::new(VexRandomMoveGoal));
        goals.add_goal(9, Box::new(look_player));
        goals.add_goal(10, Box::new(look_mob));
        let raider: crate::world::entity::ai::goal::target::EntityClass = Arc::new(|e: &dyn Entity| e.raider_state().is_some());
        let targets = &mut self.mob.target_selector;
        targets.add_goal(1, Box::new(HurtByTargetGoal::new(vec![raider]).set_alert_others(Vec::new())));
        targets.add_goal(2, Box::new(VexCopyOwnerTargetGoal::new()));
        targets.add_goal(3, Box::new(NearestAttackableTargetGoal::new(TargetType::Player, true)));
    }

    fn get_vex_flag(&self, flag: i32) -> bool {
        (i32::from(self.mob.living.entity.entity_data.get(DATA_FLAGS_ID)) & flag) != 0
    }

    fn set_vex_flag(&mut self, flag: i32, value: bool) {
        let mut flags = i32::from(self.mob.living.entity.entity_data.get(DATA_FLAGS_ID));
        if value {
            flags |= flag;
        } else {
            flags &= !flag;
        }
        self.mob.living.entity.entity_data.set(DATA_FLAGS_ID, (flags & 0xFF) as i8);
    }

    /// `isCharging()`.
    pub fn is_charging(&self) -> bool {
        self.get_vex_flag(FLAG_IS_CHARGING)
    }

    /// `setIsCharging(value)`.
    pub fn set_is_charging(&mut self, value: bool) {
        self.set_vex_flag(FLAG_IS_CHARGING, value);
    }

    /// `setOwner(owner)`.
    pub fn set_owner(&mut self, owner: &dyn Entity) {
        self.owner = Some(owner.uuid());
    }

    /// `getOwner()` (`OwnableEntity`): the living owner while it is in the level.
    pub fn get_owner<'l>(&self, level: &'l ServerLevel) -> Option<&'l dyn Entity> {
        level.get_entity_by_uuid(self.owner?).filter(|e| e.living().is_some())
    }

    /// `getBoundOrigin()`.
    pub fn get_bound_origin(&self) -> Option<BlockPos> {
        self.bound_origin
    }

    /// `setBoundOrigin(boundOrigin)`.
    pub fn set_bound_origin(&mut self, bound_origin: Option<BlockPos>) {
        self.bound_origin = bound_origin;
    }

    /// `setLimitedLife(lifeTicks)`.
    pub fn set_limited_life(&mut self, life_ticks: i32) {
        self.has_limited_life = true;
        self.limited_life_ticks = life_ticks;
    }
}

impl Entity for Vex {
    crate::mob_entity_accessors!(mob);

    fn is_flapping(&self) -> bool {
        self.base().tick_count % TICKS_PER_FLAP == 0
    }

    fn is_affected_by_blocks(&self) -> bool {
        !self.is_removed()
    }

    fn tick(&mut self, level: &mut ServerLevel) {
        self.base_mut().no_physics = true;
        mob::tick::tick(self, level);
        self.base_mut().no_physics = false;
        self.base_mut().set_no_gravity(true);
        if self.has_limited_life {
            self.limited_life_ticks -= 1;
            if self.limited_life_ticks <= 0 {
                self.limited_life_ticks = 20;
                let starve = level.damage_sources().starve();
                self.hurt_server(level, &starve, 1.0);
            }
        }
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        mob::nbt::read_additional_save_data(self, input);
        self.bound_origin = input.get("bound_pos").and_then(|t| t.as_int_array()).filter(|a| a.len() == 3).map(|a| BlockPos::new(a[0], a[1], a[2]));
        match input.get("life_ticks").and_then(|t| t.as_number()) {
            Some(n) => self.set_limited_life(n.int_value()),
            None => self.has_limited_life = false,
        }
        self.owner = crate::world::entity::entity::read_uuid(input.get("owner"));
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        mob::nbt::add_additional_save_data(self, out);
        if let Some(pos) = self.bound_origin {
            out.put_int_array("bound_pos", vec![pos.x(), pos.y(), pos.z()]);
        }
        if self.has_limited_life {
            out.put_int("life_ticks", self.limited_life_ticks);
        }
        if let Some(owner) = self.owner {
            out.put_int_array("owner", mc_core::uuid_util::uuid_to_int_array(&owner).to_vec());
        }
    }

    fn get_ambient_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_VEX_AMBIENT)
    }

    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_VEX_DEATH)
    }

    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_VEX_HURT)
    }

    fn get_light_level_dependent_magic_value(&self, _level: &ServerLevel) -> f32 {
        1.0
    }

    fn finalize_spawn(
        &mut self,
        level: &mut dyn ServerLevelAccessor,
        difficulty: DifficultyInstance,
        reason: EntitySpawnReason,
        group_data: Option<Box<dyn crate::world::entity::spawn_group_data::SpawnGroupData>>,
    ) -> Option<Box<dyn crate::world::entity::spawn_group_data::SpawnGroupData>> {
        let registries = level.registry_access().clone();
        self.populate_default_equipment_slots(level.random(), &difficulty);
        self.populate_default_equipment_enchantments(&registries, level.random(), &difficulty);
        mob::spawn::finalize_spawn(self, level, difficulty, reason, group_data)
    }

    fn populate_default_equipment_slots(&mut self, _random: &mut dyn RandomSource, _difficulty: &DifficultyInstance) {
        mob::equipment::set_item_slot_spawn(self, EquipmentSlot::MainHand, ItemStack::new(Item::IRON_SWORD));
        self.set_drop_chance(EquipmentSlot::MainHand, 0.0);
    }
}

/// `Vex.VexChargeAttackGoal`.
struct VexChargeAttackGoal;

fn vex_mut(mob: &mut dyn Entity) -> &mut Vex {
    mob.as_any_mut().downcast_mut::<Vex>().expect("Vex")
}

impl Goal for VexChargeAttackGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        let Some(target) = mob::tick::get_target_checked(mob, level).and_then(|t| level.get_entity(t)) else { return false };
        if target.is_alive() && !mob.mob().and_then(|m| m.move_control.as_deref()).expect("move control").has_wanted() && mob.base_mut().random.next_int_bounded(reduced_tick_delay(7)) == 0 {
            mob.base().distance_to_sqr_vec(target.position()) > 4.0
        } else {
            false
        }
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        mob.mob().and_then(|m| m.move_control.as_deref()).expect("move control").has_wanted()
            && mob.as_any().downcast_ref::<Vex>().is_some_and(Vex::is_charging)
            && mob::tick::get_target_checked(mob, level).and_then(|t| level.get_entity(t)).is_some_and(|t| t.is_alive())
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        if let Some(target) = mob::tick::get_target_checked(mob, level).and_then(|t| level.get_entity(t)) {
            let eye = target.get_eye_position_partial(1.0);
            mob.mob_mut().expect("mob").move_control_mut().set_wanted_position(eye.x, eye.y, eye.z, 1.0);
        }
        vex_mut(mob).set_is_charging(true);
        mob.play_sound(level, SoundEvent::ENTITY_VEX_CHARGE, 1.0, 1.0);
    }

    fn stop(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) {
        vex_mut(mob).set_is_charging(false);
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        let Some(target_id) = mob::tick::get_target_checked(mob, level) else { return };
        let Some(target) = level.get_entity(target_id) else { return };
        if mob.base().bounding_box().intersects_aabb(&target.base().bounding_box()) {
            mob::combat::do_hurt_target(mob, level, target_id);
            vex_mut(mob).set_is_charging(false);
        } else {
            let distance = mob.base().distance_to_sqr_vec(target.position());
            if distance < 9.0 {
                let eye = target.get_eye_position_partial(1.0);
                mob.mob_mut().expect("mob").move_control_mut().set_wanted_position(eye.x, eye.y, eye.z, 1.0);
            }
        }
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Move])
    }
}

/// `Vex.VexCopyOwnerTargetGoal`.
struct VexCopyOwnerTargetGoal {
    target_goal: TargetGoal,
    copy_owner_targeting: TargetingConditions,
}

impl VexCopyOwnerTargetGoal {
    fn new() -> Self {
        Self { target_goal: TargetGoal::new(false, false), copy_owner_targeting: TargetingConditions::for_non_combat().ignore_line_of_sight().ignore_invisibility_testing() }
    }

    /// The owner's target (`owner instanceof Targeting && owner.getTarget()`).
    fn owner_target(mob: &dyn Entity, level: &ServerLevel) -> Option<EntityId> {
        let owner = mob.as_any().downcast_ref::<Vex>()?.get_owner(level)?;
        mob::tick::get_target_checked(owner, level)
    }
}

impl Goal for VexCopyOwnerTargetGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        let target = Self::owner_target(mob, level);
        target.is_some() && self.target_goal.can_attack(mob, level, target, &self.copy_owner_targeting)
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.target_goal.can_continue_to_use(mob, level)
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        let target = Self::owner_target(mob, level);
        mob.set_target(level, target);
        self.target_goal.start();
    }

    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.target_goal.stop(mob, level);
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Target])
    }
}

/// `Vex.VexMoveControl`.
struct VexMoveControl {
    base: MoveControlBase,
}

impl MoveControl for VexMoveControl {
    fn base(&self) -> &MoveControlBase {
        &self.base
    }

    fn base_mut(&mut self) -> &mut MoveControlBase {
        &mut self.base
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        if self.base.operation != Operation::MoveTo {
            return;
        }
        let delta = Vec3::new(self.base.wanted_x - mob.base().x(), self.base.wanted_y - mob.base().y(), self.base.wanted_z - mob.base().z());
        let delta_length = delta.length();
        if delta_length < mob.base().bounding_box().get_size() {
            self.base.operation = Operation::Wait;
            let m = mob.base().delta_movement();
            mob.base_mut().set_delta_movement(m.scale(0.5));
        } else {
            let m = mob.base().delta_movement();
            mob.base_mut().set_delta_movement(m.add(delta.scale(self.base.speed_modifier * 0.05 / delta_length)));
            let pi = std::f32::consts::PI;
            let target = mob::tick::get_target_checked(mob, level).and_then(|t| level.get_entity(t)).map(|t| (t.base().x(), t.base().z()));
            let y_rot = match target {
                None => {
                    let movement = mob.base().delta_movement();
                    -(mth::atan2(movement.x, movement.z) as f32) * (180.0 / pi)
                }
                Some((tx, tz)) => {
                    let (tx, tz) = (tx - mob.base().x(), tz - mob.base().z());
                    -(mth::atan2(tx, tz) as f32) * (180.0 / pi)
                }
            };
            mob.base_mut().set_y_rot(y_rot);
            mob.living_mut().expect("living").y_body_rot = y_rot;
        }
    }
}

/// `Vex.VexRandomMoveGoal`.
struct VexRandomMoveGoal;

impl Goal for VexRandomMoveGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) -> bool {
        !mob.mob().and_then(|m| m.move_control.as_deref()).expect("move control").has_wanted() && mob.base_mut().random.next_int_bounded(reduced_tick_delay(7)) == 0
    }

    fn can_continue_to_use(&mut self, _mob: &mut dyn Entity, _level: &mut ServerLevel) -> bool {
        false
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        let bound_origin = mob.as_any().downcast_ref::<Vex>().and_then(Vex::get_bound_origin).unwrap_or_else(|| mob.block_position());
        for _ in 0..3 {
            let random = &mut mob.base_mut().random;
            let (dx, dy, dz) = (random.next_int_bounded(15) - 7, random.next_int_bounded(11) - 5, random.next_int_bounded(15) - 7);
            let test_pos = bound_origin.offset(dx, dy, dz);
            if crate::world::level::LevelReader::is_empty_block(&*level, test_pos) {
                let (x, y, z) = (f64::from(test_pos.x()) + 0.5, f64::from(test_pos.y()) + 0.5, f64::from(test_pos.z()) + 0.5);
                mob.mob_mut().expect("mob").move_control_mut().set_wanted_position(x, y, z, 0.25);
                if mob::tick::get_target_checked(mob, level).is_none() {
                    if let Some(m) = mob.mob_mut() {
                        m.look_control_mut().set_look_at_full(x, y, z, 180.0, 20.0);
                    }
                }
                break;
            }
        }
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Move])
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static VEX: EntityRegistration = EntityRegistration { entity_type: EntityType::VEX, create: |ty, id, registries| Box::new(Vex::new(ty, id, registries)) };
