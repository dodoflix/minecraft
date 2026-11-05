//! `net.minecraft.world.entity.animal.fish.AbstractFish`: the shared fish behavior (bucket flag, flopping on land,
//! `FishMoveControl`, `FishSwimGoal`, panic/avoid-player goals, water-bound navigation).
//!
//! A concrete fish embeds `MobData`, implements [`AbstractFish`] and writes `abstract_fish_entity!()` inside its
//! `impl Entity` block (which expands to every override `AbstractFish` makes), adding its own overrides after it.

use crate::define_accessor;
use crate::server::level::ServerLevel;
use crate::world::entity::ai::control::move_control::{MoveControl, MoveControlBase, Operation};
use crate::world::entity::ai::goal::{AvoidEntityGoal, Flag, Goal, GoalFlags, PanicGoal, RandomSwimmingGoal};
use crate::world::entity::ai::navigation::StandardPathNavigation;
use crate::world::entity::mob::{self, MobData};
use crate::world::entity::movement;
use crate::world::entity::{Entity, MoverType};
use crate::world::item::ItemStack;
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_data::{Attribute, SoundEvent};
use mc_nbt::CompoundTag;
use std::sync::Arc;

define_accessor!(pub FROM_BUCKET: Boolean = AbstractFish);

/// The members of `AbstractFish` a subclass supplies (abstract / overridable methods).
pub trait AbstractFish: Entity {
    /// `canRandomSwim()`.
    fn can_random_swim(&self, _level: &ServerLevel) -> bool {
        true
    }
    /// `getFlopSound()`.
    fn get_flop_sound(&self) -> SoundEvent;
    /// `getBucketItemStack()`.
    fn get_bucket_item_stack(&self) -> ItemStack;
    /// `saveToBucketTag(bucket)` (subclasses add their variant components).
    fn save_to_bucket_tag(&self, bucket: &mut ItemStack) {
        crate::world::entity::bucketable::save_default_data_to_bucket_tag(self.as_dyn(), bucket);
    }
}

/// `AbstractFish.createAttributes()` is `Mob.createMobAttributes().add(MAX_HEALTH, 3.0)`: read from `mc_data`.
pub fn define_synched_data(b: &mut crate::world::entity::synched_data::SynchedEntityDataBuilder) {
    b.define(FROM_BUCKET, false);
}

/// `fromBucket()`.
pub fn from_bucket(e: &dyn Entity) -> bool {
    e.base().entity_data.get(FROM_BUCKET)
}

/// `setFromBucket(fromBucket)`.
pub fn set_from_bucket(e: &mut dyn Entity, from_bucket: bool) {
    e.base_mut().entity_data.set(FROM_BUCKET, from_bucket);
}

/// `AbstractFish.<init>` (after `WaterAnimal.<init>`): the water-bound navigation and `FishMoveControl`; `WaterAnimal`
/// sets the water malus. Called by the concrete constructor in `Mob.<init>` order (navigation before `registerGoals`).
pub fn init_controls(mob: &mut MobData, navigation: StandardPathNavigation) {
    mob.set_pathfinding_malus(crate::world::level::pathfinder::PathType::Water, 0.0);
    mob.move_control = Some(Box::<FishMoveControl>::default());
    mob.set_navigation(Box::new(navigation));
}

/// `registerGoals()` of `AbstractFish` (`super.registerGoals()` of `WaterAnimal`/`PathfinderMob` adds nothing).
pub fn register_goals(e: &mut dyn Entity) {
    // `EntitySelector.NO_SPECTATORS`
    let avoid = AvoidEntityGoal::with_predicate_on_avoid_entity(Arc::new(|target| target.is_player()), 8.0, 1.6, 1.4, Arc::new(|target| !target.is_spectator()));
    let goals = &mut e.mob_mut().expect("mob").goal_selector;
    goals.add_goal(0, Box::new(PanicGoal::new(1.25)));
    goals.add_goal(2, Box::new(avoid));
    goals.add_goal(4, Box::new(FishSwimGoal::new()));
}

/// `requiresCustomPersistence()`: `super.requiresCustomPersistence() || fromBucket()`.
pub fn requires_custom_persistence(e: &dyn Entity) -> bool {
    e.base().is_passenger() || mob::leashable::is_leashed(e) || from_bucket(e)
}

/// `removeWhenFarAway(distSqr)`.
pub fn remove_when_far_away(e: &dyn Entity) -> bool {
    !from_bucket(e) && !e.base().has_custom_name()
}

/// `addAdditionalSaveData`: `FromBucket`.
pub fn add_additional_save_data(e: &dyn Entity, out: &mut CompoundTag) {
    mob::nbt::add_additional_save_data(e, out);
    out.put_boolean("FromBucket", from_bucket(e));
}

/// `readAdditionalSaveData`: `FromBucket`.
pub fn read_additional_save_data(e: &mut dyn Entity, input: &CompoundTag) {
    mob::nbt::read_additional_save_data(e, input);
    set_from_bucket(e, input.get_boolean_or("FromBucket", false));
}

/// `travelInWater(input, baseGravity, isFalling, oldY)`.
pub fn travel_in_water(e: &mut dyn Entity, level: &mut ServerLevel, input: Vec3) {
    e.move_relative(0.01, input);
    let delta = e.base().delta_movement();
    e.r#move(level, MoverType::SelfMover, delta);
    let scaled = e.base().delta_movement().scale(0.9);
    e.base_mut().set_delta_movement(scaled);
    if e.get_target().is_none() {
        let dm = e.base().delta_movement().add(Vec3::new(0.0, -0.005, 0.0));
        e.base_mut().set_delta_movement(dm);
    }
}

/// `aiStep()`: flops on land, then `Mob.aiStep`.
pub fn ai_step(e: &mut dyn Entity, level: &mut ServerLevel, flop_sound: SoundEvent) {
    if !e.is_in_water() && e.base().on_ground() && e.base().vertical_collision {
        let dx = (e.base_mut().random.next_float() * 2.0 - 1.0) * 0.05;
        let dz = (e.base_mut().random.next_float() * 2.0 - 1.0) * 0.05;
        let dm = e.base().delta_movement().add(Vec3::new(f64::from(dx), f64::from(0.4f32), f64::from(dz)));
        e.base_mut().set_delta_movement(dm);
        movement::set_on_ground(e, &*level, false);
        e.base_mut().needs_sync = true;
        crate::world::entity::living_entity::damage::make_sound(e, level, flop_sound);
    }
    mob::tick::ai_step(e, level);
}

/// `AbstractFish.FishMoveControl`.
#[derive(Default)]
pub struct FishMoveControl {
    base: MoveControlBase,
}

impl MoveControl for FishMoveControl {
    fn base(&self) -> &MoveControlBase {
        &self.base
    }
    fn base_mut(&mut self) -> &mut MoveControlBase {
        &mut self.base
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        if mob.is_eye_in_fluid("water") {
            let dm = mob.base().delta_movement().add(Vec3::new(0.0, 0.005, 0.0));
            mob.base_mut().set_delta_movement(dm);
        }
        let b = &mut self.base;
        if b.operation == Operation::MoveTo && !mob::navigation::is_done(mob, level) {
            let target_speed = (b.speed_modifier * mob::attribute_value(mob, Attribute::MOVEMENT_SPEED)) as f32;
            let speed = mth::lerp_f32(0.125, mob.get_speed(), target_speed);
            mob.set_speed(speed);
            let xd = b.wanted_x - mob.base().x();
            let yd = b.wanted_y - mob.base().y();
            let zd = b.wanted_z - mob.base().z();
            if yd != 0.0 {
                let dd = (xd * xd + yd * yd + zd * zd).sqrt();
                let dm = mob.base().delta_movement().add(Vec3::new(0.0, f64::from(mob.get_speed()) * (yd / dd) * 0.1, 0.0));
                mob.base_mut().set_delta_movement(dm);
            }
            if xd != 0.0 || zd != 0.0 {
                let y_rot_d = (mth::atan2(zd, xd) * f64::from(180.0f32) / f64::from(std::f32::consts::PI)) as f32 - 90.0;
                let new_y_rot = b.rotlerp(mob.base().y_rot(), y_rot_d, 90.0);
                mob.set_y_rot(new_y_rot);
                l_mut(mob).y_body_rot = new_y_rot;
            }
        } else {
            mob.set_speed(0.0);
        }
    }
}

fn l_mut(e: &mut dyn Entity) -> &mut crate::world::entity::living::LivingEntityData {
    e.living_mut().expect("living mob")
}

/// `AbstractFish.FishSwimGoal`: `RandomSwimmingGoal(fish, 1.0, 40)` gated by `canRandomSwim()`.
pub struct FishSwimGoal {
    inner: RandomSwimmingGoal,
}

impl FishSwimGoal {
    pub fn new() -> Self {
        Self { inner: RandomSwimmingGoal::new(1.0, 40) }
    }
}

impl Default for FishSwimGoal {
    fn default() -> Self {
        Self::new()
    }
}

/// `fish.canRandomSwim()` for whichever concrete fish `e` is.
fn can_random_swim(e: &dyn Entity, level: &ServerLevel) -> bool {
    super::as_abstract_fish(e).is_none_or(|f| f.can_random_swim(level))
}

impl Goal for FishSwimGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        can_random_swim(mob, level) && self.inner.can_use(mob, level)
    }
    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.inner.can_continue_to_use(mob, level)
    }
    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.inner.start(mob, level);
    }
    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.inner.stop(mob, level);
    }
    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Move])
    }
}

/// The overrides `AbstractFish` makes of `Mob`/`Entity` for an `impl Entity` block of a concrete fish (which also
/// receives `WaterAnimal`'s through [`water_creature_entity!`]). Requires `AbstractFish` implemented on `Self`.
macro_rules! abstract_fish_entity {
    ($encode_id:literal) => {
        crate::mob_entity_accessors!(mob);
        fn get_encode_id(&self) -> Option<&'static str> {
            Some($encode_id)
        }
        crate::world::entity::animal::fish::water_animal::water_creature_entity!();
        fn requires_custom_persistence(&self) -> bool {
            crate::world::entity::animal::fish::abstract_fish::requires_custom_persistence(self)
        }
        fn remove_when_far_away(&self, _dist_sqr: f64) -> bool {
            crate::world::entity::animal::fish::abstract_fish::remove_when_far_away(self)
        }
        fn travel_in_water(&mut self, level: &mut crate::server::level::ServerLevel, input: mc_core::phys::Vec3, _base_gravity: f64, _is_falling: bool, _old_y: f64) {
            crate::world::entity::animal::fish::abstract_fish::travel_in_water(self, level, input);
        }
        fn ai_step(&mut self, level: &mut crate::server::level::ServerLevel) {
            let flop = crate::world::entity::animal::fish::abstract_fish::AbstractFish::get_flop_sound(self);
            crate::world::entity::animal::fish::abstract_fish::ai_step(self, level, flop);
        }
        fn get_swim_sound(&self) -> mc_data::SoundEvent {
            mc_data::SoundEvent::ENTITY_FISH_SWIM
        }
        fn play_step_sound(&mut self, _level: &mut crate::server::level::ServerLevel, _pos: mc_core::pos::BlockPos, _state: mc_data::block::BlockState) {}
        fn as_bucketable(&self) -> Option<&dyn crate::world::entity::bucketable::Bucketable> {
            Some(self)
        }
        fn as_bucketable_mut(&mut self) -> Option<&mut dyn crate::world::entity::bucketable::Bucketable> {
            Some(self)
        }
        fn mob_interact(
            &mut self,
            level: &mut crate::server::level::ServerLevel,
            player: &mut crate::world::entity::ServerPlayer,
            hand: crate::world::item::InteractionHand,
        ) -> crate::world::item::InteractionResult {
            match crate::world::entity::bucketable::bucket_mob_pickup(level, player, hand, self.id()) {
                Some(result) => result,
                None => crate::world::entity::mob::interact::mob_interact(self, level, player, hand),
            }
        }
    };
}
pub(crate) use abstract_fish_entity;

/// `impl Bucketable for $ty` (`AbstractFish`'s part): flag, default bucket data, pickup sound.
macro_rules! abstract_fish_bucketable {
    ($ty:ty) => {
        impl crate::world::entity::bucketable::Bucketable for $ty {
            fn from_bucket(&self) -> bool {
                crate::world::entity::animal::fish::abstract_fish::from_bucket(self)
            }
            fn set_from_bucket(&mut self, from_bucket: bool) {
                crate::world::entity::animal::fish::abstract_fish::set_from_bucket(self, from_bucket);
            }
            fn save_to_bucket_tag(&self, bucket: &mut crate::world::item::ItemStack) {
                crate::world::entity::animal::fish::abstract_fish::AbstractFish::save_to_bucket_tag(self, bucket);
            }
            fn load_from_bucket_tag(&mut self, tag: &mc_nbt::CompoundTag) {
                crate::world::entity::bucketable::load_default_data_from_bucket_tag(self, tag);
            }
            fn get_bucket_item_stack(&self) -> crate::world::item::ItemStack {
                crate::world::entity::animal::fish::abstract_fish::AbstractFish::get_bucket_item_stack(self)
            }
            fn get_pickup_sound(&self) -> mc_data::SoundEvent {
                mc_data::SoundEvent::ITEM_BUCKET_FILL_FISH
            }
        }
    };
}
pub(crate) use abstract_fish_bucketable;
