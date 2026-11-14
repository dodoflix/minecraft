//! `Fox`: sleeping, stalking and pouncing, berry eating, mouth item carrying, trusted players, red/snow variants.

use super::fox_goals::{
    DefendTrustedTargetGoal, FaceplantGoal, FoxBreedGoal, FoxEatBerriesGoal, FoxFloatGoal,
    FoxFollowParentGoal, FoxLookAtPlayerGoal, FoxMeleeAttackGoal, FoxPanicGoal, FoxPounceGoal,
    FoxSearchForItemsGoal, FoxStrollThroughVillageGoal, PerchAndSearchGoal, SeekShelterGoal,
    SleepGoal, StalkPreyGoal,
};
use crate::world::entity::animal::turtle::turtle::Turtle;
use crate::world::entity::animal::wolf::wolf::Wolf;
use linkme::distributed_slice;
use mc_core::ident::Identifier;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::enums::{EntitySpawnReason, Pose};
use mc_data::{EntityType, Item, SoundEvent};
use mc_game_ai::world::entity::ai::goal::{
    AvoidEntityGoal, ClimbOnTopOfPowderSnowGoal, LeapAtTargetGoal,
};
use mc_game_core::define_accessor;
use mc_game_core::registry::RegistryAccess;
use mc_game_core::registry::keys;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::damagesource::DamageSource;
use mc_game_core::world::difficulty_instance::DifficultyInstance;
use mc_game_core::world::entity::ai::control::look_control::{self, LookControl, LookControlBase};
use mc_game_core::world::entity::ai::control::move_control::{self, MoveControl, MoveControlBase};
use mc_game_core::world::entity::ai::goal::WaterAvoidingRandomStrollGoal;
use mc_game_core::world::entity::ai::goal::target::{
    NearestAttackableTargetGoal, TargetType, class_of,
};
use mc_game_core::world::entity::ai::navigation::StandardPathNavigation;
use mc_game_core::world::entity::dimensions::{EntityAttachments, EntityDimensions};
use mc_game_core::world::entity::equipment::EquipmentSlot;
use mc_game_core::world::entity::factory::{
    ENTITY_FACTORIES, EntityRegistration, EntitySpawnRequest,
};
use mc_game_core::world::entity::item::ItemEntity;
use mc_game_core::world::entity::mob::spawn::AgeableMobGroupData;
use mc_game_core::world::entity::mob::{self, MobData, MobKind, ageable, ext, tamable};
use mc_game_core::world::entity::spawn_group_data::SpawnGroupData;
use mc_game_core::world::entity::spawn_predicates;
use mc_game_core::world::entity::{Entity, EntityId};
use mc_game_core::world::item::ItemStack;
use mc_game_core::world::level::block::BlockStateExt as _;
use mc_game_core::world::level::level_access::{BlockGetter, LevelAccessor as _};
use mc_game_core::world::level::pathfinder::PathType;
use mc_game_core::world::level::server_level_accessor::ServerLevelAccessor;
use mc_nbt::{CompoundTag, ListTag, Tag};
use mc_protocol::components::{CONSUMABLE, DataComponentMap, FOOD, FOX_VARIANT, FoxVariant};
use mc_protocol::packets::play::SoundSource;
use std::sync::Arc;
use uuid::Uuid;

define_accessor!(pub DATA_TYPE_ID: Int = Fox);
define_accessor!(pub DATA_FLAGS_ID: Byte = Fox);
define_accessor!(pub DATA_TRUSTED_ID_0: OptionalLivingEntityReference = Fox);
define_accessor!(pub DATA_TRUSTED_ID_1: OptionalLivingEntityReference = Fox);

pub const FLAG_SITTING: i32 = 1;
pub const FLAG_CROUCHING: i32 = 4;
pub const FLAG_INTERESTED: i32 = 8;
pub const FLAG_POUNCING: i32 = 16;
pub const FLAG_SLEEPING: i32 = 32;
pub const FLAG_FACEPLANTED: i32 = 64;
pub const FLAG_DEFENDING: i32 = 128;
const MAX_CROUCH_AMOUNT: f32 = 5.0;
const MIN_TICKS_BEFORE_EAT: i32 = 600;

/// `Fox.Variant.DEFAULT`.
pub const DEFAULT_VARIANT: FoxVariant = FoxVariant::Red;

/// `Fox.Variant.byId(id)` (`ByIdMap.continuous(.., ZERO)`).
pub fn variant_by_id(id: i32) -> FoxVariant {
    FoxVariant::from_id(id).unwrap_or(DEFAULT_VARIANT)
}

/// `BABY_DIMENSIONS`.
fn baby_dimensions() -> EntityDimensions {
    let base = EntityDimensions::of_type(EntityType::FOX).scale(0.6);
    base.with_eye_height(0.34375).with_attachments(
        EntityAttachments::create_default(base.width, base.height)
            .with_passenger(Vec3::new(0.0, 0.375, 0.0)),
    )
}

/// `Fox.FoxGroupData`.
pub struct FoxGroupData {
    base: AgeableMobGroupData,
    pub variant: FoxVariant,
}

impl FoxGroupData {
    fn new(variant: FoxVariant) -> Self {
        Self {
            base: AgeableMobGroupData::with_baby(false),
            variant,
        }
    }
}

impl SpawnGroupData for FoxGroupData {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
    fn as_ageable_mut(&mut self) -> Option<&mut AgeableMobGroupData> {
        Some(&mut self.base)
    }
}

/// `Fox.FoxLookControl`.
pub struct FoxLookControl {
    base: LookControlBase,
    reset_x_rot: bool,
}

impl Default for FoxLookControl {
    fn default() -> Self {
        Self {
            base: LookControlBase::default(),
            reset_x_rot: true,
        }
    }
}

impl LookControl for FoxLookControl {
    fn base(&self) -> &LookControlBase {
        &self.base
    }
    fn base_mut(&mut self) -> &mut LookControlBase {
        &mut self.base
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        if !mob.is_sleeping() {
            let fox = mob.as_any().downcast_ref::<Fox>().expect("fox");
            self.reset_x_rot = !fox.is_pouncing()
                && !mob.is_crouching()
                && !fox.is_interested()
                && !fox.is_faceplanted();
            look_control::default_tick(self, mob, level);
        }
    }

    fn reset_x_rot_on_tick(&self) -> bool {
        self.reset_x_rot
    }
}

/// `Fox.FoxMoveControl`.
#[derive(Default)]
pub struct FoxMoveControl {
    base: MoveControlBase,
}

impl MoveControl for FoxMoveControl {
    fn base(&self) -> &MoveControlBase {
        &self.base
    }
    fn base_mut(&mut self) -> &mut MoveControlBase {
        &mut self.base
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        if mob
            .as_any()
            .downcast_ref::<Fox>()
            .is_some_and(Fox::can_move)
        {
            move_control::base_tick(&mut self.base, mob, level);
        }
    }
}

/// `Fox`.
pub struct Fox {
    pub mob: MobData,
    interested_angle: f32,
    interested_angle_o: f32,
    pub crouch_amount: f32,
    pub crouch_amount_o: f32,
    pub ticks_since_eaten: i32,
}

/// `ItemTags.FOX_FOOD`.
fn is_fox_food(stack: &ItemStack) -> bool {
    mc_data::tags::get::<Item>("fox_food").is_some_and(|t| t.contains(stack.item()))
}

/// `isConsumableFood(itemStack)`.
fn is_consumable_food(stack: &ItemStack) -> bool {
    stack.get(&FOOD).is_some() && stack.get(&CONSUMABLE).is_some()
}

impl Fox {
    /// `new Fox(type, level)`.
    pub fn new(ty: EntityType, id: EntityId, _registries: &RegistryAccess) -> Self {
        let mob = MobData::new(ty, id, MobKind::Animal, |b| {
            b.define(DATA_TRUSTED_ID_0, None);
            b.define(DATA_TRUSTED_ID_1, None);
            b.define(DATA_TYPE_ID, DEFAULT_VARIANT.id());
            b.define(DATA_FLAGS_ID, 0i8);
        });
        let mut fox = Fox {
            mob,
            interested_angle: 0.0,
            interested_angle_o: 0.0,
            crouch_amount: 0.0,
            crouch_amount_o: 0.0,
            ticks_since_eaten: 0,
        };
        let navigation = StandardPathNavigation::ground(&fox);
        fox.mob.set_navigation(Box::new(navigation));
        fox.register_goals();
        fox.mob.look_control = Some(Box::<FoxLookControl>::default());
        fox.mob.move_control = Some(Box::<FoxMoveControl>::default());
        fox.mob
            .set_pathfinding_malus(PathType::DamagingInNeighbor, 0.0);
        fox.mob.set_pathfinding_malus(PathType::Damaging, 0.0);
        fox.mob.can_pick_up_loot = true;
        if let Some(mut n) = fox.mob.navigation.take() {
            n.set_required_path_length(&fox, 32.0);
            fox.mob.navigation = Some(n);
        }
        fox
    }

    /// `registerGoals()`.
    fn register_goals(&mut self) {
        let float = FoxFloatGoal::new(self);
        let climb = ClimbOnTopOfPowderSnowGoal::new();
        let breed = FoxBreedGoal::new(self, 1.0);
        let follow_parent = FoxFollowParentGoal::new(self, 1.25);
        let look_at = FoxLookAtPlayerGoal::new(self, 24.0);
        let sleep = SleepGoal::new(self);
        let goals = &mut self.mob.goal_selector;
        goals.add_goal(0, Box::new(float));
        goals.add_goal(0, Box::new(climb));
        goals.add_goal(1, Box::new(FaceplantGoal::new()));
        goals.add_goal(2, Box::new(FoxPanicGoal::new(2.2)));
        goals.add_goal(3, Box::new(breed));
        // `entity -> AVOID_PLAYERS.test(entity) && !this.trusts(entity) && !this.isDefending()`
        goals.add_goal(
            4,
            Box::new(
                AvoidEntityGoal::with_predicate_on_avoid_entity(
                    Arc::new(|e: &dyn Entity| e.is_player()),
                    16.0,
                    1.6,
                    1.4,
                    Arc::new(|e: &dyn Entity| !e.is_discrete() && ext::no_creative_or_spectator(e)),
                )
                .with_source_selector(Arc::new(
                    |fox: &dyn Entity, target: &dyn Entity, _level: &ServerLevel| {
                        fox.as_any()
                            .downcast_ref::<Fox>()
                            .is_some_and(|f| !f.trusts(target) && !f.is_defending())
                    },
                )),
            ),
        );
        goals.add_goal(
            4,
            Box::new(
                AvoidEntityGoal::with_predicate_on_avoid_entity(
                    Arc::new(|e: &dyn Entity| e.as_any().is::<Wolf>()),
                    8.0,
                    1.6,
                    1.4,
                    Arc::new(|e: &dyn Entity| !tamable::is_tame(e)),
                )
                .with_source_selector(Arc::new(
                    |fox: &dyn Entity, _target: &dyn Entity, _level: &ServerLevel| {
                        fox.as_any()
                            .downcast_ref::<Fox>()
                            .is_some_and(|f| !f.is_defending())
                    },
                )),
            ),
        );
        goals.add_goal(
            4,
            Box::new(
                AvoidEntityGoal::with_predicate_on_avoid_entity(
                    Arc::new(|e: &dyn Entity| e.get_type() == EntityType::POLAR_BEAR),
                    8.0,
                    1.6,
                    1.4,
                    Arc::new(|_: &dyn Entity| true),
                )
                .with_source_selector(Arc::new(
                    |fox: &dyn Entity, _target: &dyn Entity, _level: &ServerLevel| {
                        fox.as_any()
                            .downcast_ref::<Fox>()
                            .is_some_and(|f| !f.is_defending())
                    },
                )),
            ),
        );
        goals.add_goal(5, Box::new(StalkPreyGoal));
        goals.add_goal(6, Box::new(FoxPounceGoal));
        goals.add_goal(6, Box::new(SeekShelterGoal::new(1.25)));
        goals.add_goal(
            7,
            Box::new(FoxMeleeAttackGoal::new(f64::from(1.2f32), true)),
        );
        goals.add_goal(7, Box::new(sleep));
        goals.add_goal(8, Box::new(follow_parent));
        goals.add_goal(9, Box::new(FoxStrollThroughVillageGoal::new(32, 200)));
        goals.add_goal(10, Box::new(FoxEatBerriesGoal::new(1.2, 12, 1)));
        goals.add_goal(10, Box::new(LeapAtTargetGoal::new(0.4)));
        goals.add_goal(11, Box::new(WaterAvoidingRandomStrollGoal::new(1.0)));
        goals.add_goal(11, Box::new(FoxSearchForItemsGoal));
        goals.add_goal(12, Box::new(look_at));
        goals.add_goal(13, Box::new(PerchAndSearchGoal::new()));
        self.mob
            .target_selector
            .add_goal(3, Box::new(DefendTrustedTargetGoal::new()));
    }

    /// `setTargetGoals()` (the goal instances `landTargetGoal` / `turtleEggTargetGoal` / `fishTargetGoal` of `registerGoals`).
    pub fn set_target_goals(&mut self) {
        let land = || {
            NearestAttackableTargetGoal::build(
                TargetType::Class(Arc::new(|e: &dyn Entity| {
                    e.mob().is_some_and(|m| m.animal.is_some())
                })),
                10,
                false,
                false,
                Some(Arc::new(|target: &dyn Entity, _level: &ServerLevel| {
                    matches!(target.get_type(), EntityType::CHICKEN | EntityType::RABBIT)
                })),
            )
        };
        let turtle = || {
            NearestAttackableTargetGoal::build(
                TargetType::Class(class_of::<Turtle>()),
                10,
                false,
                false,
                // `Turtle.BABY_ON_LAND_SELECTOR`
                Some(Arc::new(|target: &dyn Entity, _level: &ServerLevel| {
                    target.is_baby() && !target.is_in_water()
                })),
            )
        };
        let fish = || {
            NearestAttackableTargetGoal::build(
                TargetType::Class(Arc::new(|e: &dyn Entity| {
                    crate::world::entity::animal::fish::as_abstract_fish(e).is_some()
                })),
                20,
                false,
                false,
                Some(Arc::new(|target: &dyn Entity, _level: &ServerLevel| {
                    crate::world::entity::animal::fish::as_schooling_fish(target).is_some()
                })),
            )
        };
        let targets = &mut self.mob.target_selector;
        if self.mob.living.entity.entity_data.get(DATA_TYPE_ID) == FoxVariant::Red.id() {
            targets.add_goal(4, Box::new(land()));
            targets.add_goal(4, Box::new(turtle()));
            targets.add_goal(6, Box::new(fish()));
        } else {
            targets.add_goal(4, Box::new(fish()));
            targets.add_goal(6, Box::new(land()));
            targets.add_goal(6, Box::new(turtle()));
        }
    }

    // ---- flags ----
    fn get_flag(&self, flag: i32) -> bool {
        (i32::from(self.base().entity_data.get(DATA_FLAGS_ID)) & flag) != 0
    }

    fn set_flag(&mut self, flag: i32, value: bool) {
        let current = i32::from(self.base().entity_data.get(DATA_FLAGS_ID));
        let new = if value {
            current | flag
        } else {
            current & !flag
        };
        self.base_mut().entity_data.set(DATA_FLAGS_ID, new as i8);
    }

    pub fn is_sitting(&self) -> bool {
        self.get_flag(FLAG_SITTING)
    }

    pub fn set_sitting(&mut self, value: bool) {
        self.set_flag(FLAG_SITTING, value);
    }

    pub fn is_faceplanted(&self) -> bool {
        self.get_flag(FLAG_FACEPLANTED)
    }

    pub fn set_faceplanted(&mut self, value: bool) {
        self.set_flag(FLAG_FACEPLANTED, value);
    }

    pub fn is_defending(&self) -> bool {
        self.get_flag(FLAG_DEFENDING)
    }

    pub fn set_defending(&mut self, value: bool) {
        self.set_flag(FLAG_DEFENDING, value);
    }

    pub fn set_sleeping(&mut self, value: bool) {
        self.set_flag(FLAG_SLEEPING, value);
    }

    pub fn is_pouncing(&self) -> bool {
        self.get_flag(FLAG_POUNCING)
    }

    pub fn set_is_pouncing(&mut self, value: bool) {
        self.set_flag(FLAG_POUNCING, value);
    }

    pub fn is_fully_crouched(&self) -> bool {
        self.crouch_amount == MAX_CROUCH_AMOUNT
    }

    pub fn set_is_crouching(&mut self, value: bool) {
        self.set_flag(FLAG_CROUCHING, value);
    }

    pub fn set_is_interested(&mut self, value: bool) {
        self.set_flag(FLAG_INTERESTED, value);
    }

    pub fn is_interested(&self) -> bool {
        self.get_flag(FLAG_INTERESTED)
    }

    /// `wakeUp()`.
    pub fn wake_up(&mut self) {
        self.set_sleeping(false);
    }

    /// `clearStates()`.
    pub fn clear_states(&mut self) {
        self.set_is_interested(false);
        self.set_is_crouching(false);
        self.set_sitting(false);
        self.set_sleeping(false);
        self.set_defending(false);
        self.set_faceplanted(false);
    }

    /// `canMove()`.
    pub fn can_move(&self) -> bool {
        !self.is_sleeping() && !self.is_sitting() && !self.is_faceplanted()
    }

    // ---- variant ----
    pub fn get_variant(&self) -> FoxVariant {
        variant_by_id(self.base().entity_data.get(DATA_TYPE_ID))
    }

    pub fn set_variant(&mut self, variant: FoxVariant) {
        self.base_mut().entity_data.set(DATA_TYPE_ID, variant.id());
    }

    // ---- trusted ----
    /// `getTrustedEntities()`.
    pub fn get_trusted_entities(&self) -> Vec<Uuid> {
        let data = &self.base().entity_data;
        data.get(DATA_TRUSTED_ID_0)
            .into_iter()
            .chain(data.get(DATA_TRUSTED_ID_1))
            .collect()
    }

    /// `addTrustedEntity(reference)`.
    pub fn add_trusted_entity(&mut self, entity: Uuid) {
        if self.base().entity_data.get(DATA_TRUSTED_ID_0).is_some() {
            self.base_mut()
                .entity_data
                .set(DATA_TRUSTED_ID_1, Some(entity));
        } else {
            self.base_mut()
                .entity_data
                .set(DATA_TRUSTED_ID_0, Some(entity));
        }
    }

    /// `trusts(entity)`.
    pub fn trusts(&self, entity: &dyn Entity) -> bool {
        let uuid = entity.uuid();
        self.get_trusted_entities().contains(&uuid)
    }

    /// `clearTrusted()`.
    fn clear_trusted(&mut self) {
        self.base_mut().entity_data.set(DATA_TRUSTED_ID_0, None);
        self.base_mut().entity_data.set(DATA_TRUSTED_ID_1, None);
    }

    /// `canEat(itemInMouth)`.
    fn can_eat(&self, level: &ServerLevel, item_in_mouth: &ItemStack) -> bool {
        is_consumable_food(item_in_mouth)
            && mob::tick::get_target_checked(self, level).is_none()
            && self.base().on_ground()
            && !self.is_sleeping()
    }

    /// `spitOutItem(itemStack)`.
    fn spit_out_item(&mut self, level: &mut ServerLevel, stack: ItemStack) {
        if !stack.is_empty() {
            let look = self.get_look_angle();
            let p = self.position();
            let mut thrown =
                ItemEntity::with_stack(level, p.x + look.x, p.y + 1.0, p.z + look.z, stack);
            thrown.set_pick_up_delay(40);
            thrown.set_thrower(self.uuid());
            self.play_sound(level, SoundEvent::ENTITY_FOX_SPIT, 1.0, 1.0);
            level.add_fresh_entity(Box::new(thrown));
        }
    }

    /// `dropItemStack(itemStack)`.
    fn drop_item_stack(&self, level: &mut ServerLevel, stack: ItemStack) {
        let p = self.position();
        let entity = ItemEntity::with_stack(level, p.x, p.y, p.z, stack);
        level.add_fresh_entity(Box::new(entity));
    }
}

/// `Fox.checkFoxSpawnRules`.
pub fn check_fox_spawn_rules(
    _ty: EntityType,
    level: &dyn ServerLevelAccessor,
    _reason: EntitySpawnReason,
    pos: BlockPos,
    _random: &mut dyn RandomSource,
) -> bool {
    level
        .get_block_state(pos.below())
        .is_tag("foxes_spawnable_on")
        && spawn_predicates::is_bright_enough_to_spawn(level, pos)
}

mc_game_core::register_spawn_placement!(EntityType::FOX, check_fox_spawn_rules);

impl Entity for Fox {
    mc_game_core::mob_entity_accessors!(mob);

    fn get_encode_id(&self) -> Option<&'static str> {
        Some("minecraft:fox")
    }

    fn get_dimensions(&self, _pose: Pose) -> EntityDimensions {
        if self.is_baby() {
            baby_dimensions()
        } else {
            EntityDimensions::of_type(self.mob.living.entity.entity_type)
        }
    }

    fn ai_step(&mut self, level: &mut ServerLevel) {
        if self.is_alive() && self.is_effective_ai() {
            self.ticks_since_eaten += 1;
            let item_in_mouth = self.get_item_by_slot(EquipmentSlot::MainHand);
            if self.can_eat(level, &item_in_mouth) {
                if self.ticks_since_eaten > MIN_TICKS_BEFORE_EAT {
                    let mut stack = item_in_mouth;
                    let remaining =
                        mc_game_core::world::item::finish_using_item(&mut stack, level, self);
                    // vanilla shrinks the stack held in the slot in place; the remainder replaces it when non-empty
                    if remaining.is_empty() {
                        self.mob
                            .living
                            .equipment
                            .set(EquipmentSlot::MainHand, stack);
                    } else {
                        self.set_item_slot(level, EquipmentSlot::MainHand, remaining);
                    }
                    self.ticks_since_eaten = 0;
                } else if self.ticks_since_eaten > 560 && self.base_mut().random.next_float() < 0.1
                {
                    self.play_eating_sound(level);
                    level.broadcast_entity_event(self.id(), 45);
                }
            }
            let target_alive = mob::tick::get_target_checked(self, level)
                .and_then(|t| level.get_entity(t))
                .is_some_and(|t| t.is_alive());
            if !target_alive {
                self.set_is_crouching(false);
                self.set_is_interested(false);
            }
        }
        if self.is_sleeping() || self.is_immobile() {
            let living = &mut self.mob.living;
            living.jumping = false;
            living.xxa = 0.0;
            living.zza = 0.0;
        }
        mob::tick::ai_step(self, level);
        if self.is_defending() && self.base_mut().random.next_float() < 0.05 {
            self.play_sound(level, SoundEvent::ENTITY_FOX_AGGRO, 1.0, 1.0);
        }
    }

    fn tick(&mut self, level: &mut ServerLevel) {
        mob::tick::tick(self, level);
        if self.is_effective_ai() {
            let in_water = self.is_in_water();
            if in_water
                || mob::tick::get_target_checked(self, level).is_some()
                || level.is_thundering()
            {
                self.wake_up();
            }
            if in_water || self.is_sleeping() {
                self.set_sitting(false);
            }
            if self.is_faceplanted() && level.random().next_float() < 0.2 {
                let pos = self.block_position();
                let state = level.get_block_state(pos);
                level.level_event_impl(None, 2001, pos, state.id() as i32);
            }
        }
        self.interested_angle_o = self.interested_angle;
        if self.is_interested() {
            self.interested_angle += (1.0 - self.interested_angle) * 0.4;
        } else {
            self.interested_angle += (0.0 - self.interested_angle) * 0.4;
        }
        self.crouch_amount_o = self.crouch_amount;
        if self.is_crouching() {
            self.crouch_amount += 0.2;
            if self.crouch_amount > MAX_CROUCH_AMOUNT {
                self.crouch_amount = MAX_CROUCH_AMOUNT;
            }
        } else {
            self.crouch_amount = 0.0;
        }
    }

    fn is_sleeping(&self) -> bool {
        self.get_flag(FLAG_SLEEPING)
    }

    fn is_crouching(&self) -> bool {
        self.get_flag(FLAG_CROUCHING)
    }

    fn set_target(&mut self, level: &ServerLevel, target: Option<EntityId>) {
        if self.is_defending() && target.is_none() {
            self.set_defending(false);
        }
        mob::tick::set_target(self, level, target);
    }

    fn populate_default_equipment_slots(
        &mut self,
        random: &mut dyn RandomSource,
        _difficulty: &DifficultyInstance,
    ) {
        if random.next_float() < 0.2 {
            let odds = random.next_float();
            let held_in_mouth = if odds < 0.05 {
                ItemStack::new(Item::EMERALD)
            } else if odds < 0.2 {
                ItemStack::new(Item::EGG)
            } else if odds < 0.4 {
                if random.next_boolean() {
                    ItemStack::new(Item::RABBIT_FOOT)
                } else {
                    ItemStack::new(Item::RABBIT_HIDE)
                }
            } else if odds < 0.6 {
                ItemStack::new(Item::WHEAT)
            } else if odds < 0.8 {
                ItemStack::new(Item::LEATHER)
            } else {
                ItemStack::new(Item::FEATHER)
            };
            mob::equipment::set_item_slot_spawn(self, EquipmentSlot::MainHand, held_in_mouth);
        }
    }

    fn can_dispenser_equip_into_slot(&self, slot: EquipmentSlot) -> bool {
        slot == EquipmentSlot::MainHand && self.can_pick_up_loot()
    }

    fn can_hold_item(&self, stack: &ItemStack) -> bool {
        let held = self.get_item_by_slot(EquipmentSlot::MainHand);
        held.is_empty()
            || self.ticks_since_eaten > 0 && is_consumable_food(stack) && !is_consumable_food(&held)
    }

    fn pick_up_item(&mut self, level: &mut ServerLevel, item_id: EntityId) {
        let Some(mut stack) = level
            .get_entity(item_id)
            .and_then(|e| e.as_any().downcast_ref::<ItemEntity>())
            .map(ItemEntity::get_item)
        else {
            return;
        };
        if self.can_hold_item(&stack) {
            let count = stack.count();
            if count > 1 {
                let extra = stack.split(count - 1);
                self.drop_item_stack(level, extra);
            }
            let in_mouth = self.get_item_by_slot(EquipmentSlot::MainHand);
            self.spit_out_item(level, in_mouth);
            mc_game_core::world::entity::player::pickup::on_item_pickup_by_id(
                level, &*self, item_id,
            );
            let one = stack.split(1);
            self.set_item_slot(level, EquipmentSlot::MainHand, one);
            mob::equipment::set_guaranteed_drop(self, EquipmentSlot::MainHand);
            ext::take(level, &*self, item_id, stack.count());
            level.with_entity(item_id, |level, e| e.discard_in_level(level));
            self.ticks_since_eaten = 0;
        }
    }

    fn drop_all_death_loot(&mut self, level: &mut ServerLevel, source: &DamageSource) {
        let stack = self.get_item_by_slot(EquipmentSlot::MainHand);
        if !stack.is_empty() {
            mc_game_core::world::entity::item::spawn_at_location(level, &*self, stack, Vec3::ZERO);
            self.set_item_slot(level, EquipmentSlot::MainHand, ItemStack::empty());
        }
        mc_game_core::world::entity::living_entity::death::drop_all_death_loot(self, level, source);
    }

    fn is_food(&self, stack: &ItemStack) -> bool {
        is_fox_food(stack)
    }

    fn on_offspring_spawned_from_egg(
        &mut self,
        level: &mut ServerLevel,
        spawner: &mut mc_game_core::world::entity::ServerPlayer,
        offspring: EntityId,
    ) {
        let uuid = spawner.uuid();
        level.with_entity(offspring, |_, e| {
            if let Some(fox) = e.as_any_mut().downcast_mut::<Fox>() {
                fox.add_trusted_entity(uuid);
            }
        });
    }

    fn get_leash_offset(&self) -> Vec3 {
        Vec3::new(
            0.0,
            f64::from(0.55f32 * self.base().eye_height),
            f64::from(self.base().get_bb_width() * 0.4),
        )
    }

    fn get_breed_offspring(
        &mut self,
        level: &mut ServerLevel,
        partner: EntityId,
    ) -> Option<Box<dyn Entity>> {
        let mut baby = mc_game_core::world::entity::factory::create(
            EntityType::FOX,
            level,
            EntitySpawnRequest {
                reason: EntitySpawnReason::Breeding,
                ignore_checks: false,
            },
        )?;
        let partner_variant = level
            .get_entity(partner)
            .and_then(|p| p.as_any().downcast_ref::<Fox>())
            .map(Fox::get_variant);
        let variant = match partner_variant {
            Some(partner_variant) if !self.base_mut().random.next_boolean() => partner_variant,
            _ => self.get_variant(),
        };
        if let Some(fox) = baby.as_any_mut().downcast_mut::<Fox>() {
            fox.set_variant(variant);
        }
        Some(baby)
    }

    fn finalize_spawn(
        &mut self,
        level: &mut dyn ServerLevelAccessor,
        difficulty: DifficultyInstance,
        reason: EntitySpawnReason,
        group_data: Option<Box<dyn SpawnGroupData>>,
    ) -> Option<Box<dyn SpawnGroupData>> {
        let registries = level.registry_access().clone();
        let biome = level.get_biome_id(self.block_position());
        let snow = Identifier::parse("minecraft:spawns_snow_foxes").expect("valid tag id");
        let mut variant = if biome.is_some_and(|b| {
            registries
                .registry(keys::BIOME)
                .is_some_and(|r| r.is_in_tag(b.0, &snow))
        }) {
            FoxVariant::Snow
        } else {
            FoxVariant::Red
        };
        let mut is_baby = false;
        let group_data = match group_data {
            Some(mut data) if data.as_any().is::<FoxGroupData>() => {
                variant = data
                    .as_any()
                    .downcast_ref::<FoxGroupData>()
                    .expect("fox group data")
                    .variant;
                if data
                    .as_ageable_mut()
                    .is_some_and(|a| a.get_group_size() >= 2)
                {
                    is_baby = true;
                }
                data
            }
            _ => Box::new(FoxGroupData::new(variant)),
        };
        self.set_variant(variant);
        if is_baby {
            ageable::set_age(self, -24000);
        }
        self.set_target_goals();
        self.populate_default_equipment_slots(level.random(), &difficulty);
        mob::spawn::finalize_spawn(self, level, difficulty, reason, Some(group_data))
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        mob::nbt::add_additional_save_data(self, out);
        let mut trusted = ListTag::new();
        for uuid in self.get_trusted_entities() {
            trusted.push(Tag::IntArray(
                mc_core::uuid_util::uuid_to_int_array(&uuid).to_vec(),
            ));
        }
        out.put("Trusted", trusted);
        out.put_boolean("Sleeping", self.is_sleeping());
        out.put_string("Type", self.get_variant().name());
        out.put_boolean("Sitting", self.is_sitting());
        out.put_boolean("Crouching", self.is_crouching());
    }

    fn read_additional_save_data(&mut self, level: Option<&ServerLevel>, input: &CompoundTag) {
        mob::nbt::read_additional_save_data(self, level, input);
        self.clear_trusted();
        if let Some(list) = input.get_list("Trusted") {
            for tag in list.iter() {
                if let Some(uuid) = mc_game_core::world::entity::entity::read_uuid(Some(tag)) {
                    self.add_trusted_entity(uuid);
                }
            }
        }
        self.set_sleeping(input.get_boolean_or("Sleeping", false));
        self.set_variant(
            input
                .get_string("Type")
                .and_then(FoxVariant::from_name)
                .unwrap_or(DEFAULT_VARIANT),
        );
        self.set_sitting(input.get_boolean_or("Sitting", false));
        self.set_is_crouching(input.get_boolean_or("Crouching", false));
        self.set_target_goals();
    }

    fn play_eating_sound(&mut self, level: &mut ServerLevel) {
        self.play_sound(level, SoundEvent::ENTITY_FOX_EAT, 1.0, 1.0);
    }

    fn play_ambient_sound(&mut self, level: &mut ServerLevel) {
        match self.get_ambient_sound_in(level) {
            Some(SoundEvent::ENTITY_FOX_SCREECH) => {
                let pitch = self.get_voice_pitch();
                self.play_sound(level, SoundEvent::ENTITY_FOX_SCREECH, 2.0, pitch);
            }
            _ => mob::tick::play_ambient_sound(self, level),
        }
    }

    fn get_ambient_sound_in(&mut self, level: &mut ServerLevel) -> Option<SoundEvent> {
        if self.is_sleeping() {
            Some(SoundEvent::ENTITY_FOX_SLEEP)
        } else {
            if !level.is_bright_outside() && self.base_mut().random.next_float() < 0.1 {
                let bb = self.base().bounding_box().inflate(16.0, 16.0, 16.0);
                if level
                    .get_entities_of_class(&bb, |e| e.is_player(), |e| !e.is_spectator())
                    .is_empty()
                {
                    return Some(SoundEvent::ENTITY_FOX_SCREECH);
                }
            }
            Some(SoundEvent::ENTITY_FOX_AMBIENT)
        }
    }

    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_FOX_HURT)
    }

    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_FOX_DEATH)
    }

    fn get_sound_source(&self) -> SoundSource {
        SoundSource::Neutral
    }

    fn entity_components(&self) -> DataComponentMap {
        let base = mc_game_core::advancements::predicates::entity::support::base_entity_components(
            self.base(),
        );
        DataComponentMap::builder()
            .add_all(&base)
            .set(FOX_VARIANT, self.get_variant())
            .build()
    }

    fn apply_implicit_components(&mut self, stack: &ItemStack) {
        if let Some(variant) = stack.get(&FOX_VARIANT) {
            self.set_variant(*variant);
        }
        mc_game_core::world::entity::spawn_config::apply_base_implicit_components(self, stack);
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static FOX: EntityRegistration = EntityRegistration {
    entity_type: EntityType::FOX,
    create: |ty, id, registries| Box::new(Fox::new(ty, id, registries)),
};
