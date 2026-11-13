//! `AbstractHorse`: taming (temper), feeding, rearing, riding (`PlayerRideableJumping`), the inventory
//! (`HasCustomInventoryScreen`), breeding attribute inheritance and the owner reference.
//!
//! Rust shape of the class hierarchy: [`AbstractHorseData`] holds the fields; the [`AbstractHorse`] trait declares the
//! virtual members subclasses override (with `AbstractHorse`'s own behavior as default); the free functions are the
//! non-virtual bodies (a subclass override calls them as its `super`). Every concrete class implements [`AbstractHorse`]
//! and forwards its `Entity` members to these functions through [`crate::abstract_horse_entity!`].

use crate::server::level::ServerLevel;
use crate::world::container::{Container, SimpleContainer};
use crate::world::damagesource::DamageSource;
use crate::world::entity::ai::goal::{
    BreedGoal, FloatGoal, FollowParentGoal, LookAtPlayerGoal, PanicGoal, RandomLookAroundGoal,
    RandomStandGoal, RunAroundLikeCrazyGoal, TemptGoal, WaterAvoidingRandomStrollGoal,
};
use crate::world::entity::ai::targeting::TargetingConditions;
use crate::world::entity::equipment::EquipmentSlot;
use crate::world::entity::living_entity::{
    self,
    util::{l, lm},
};
use crate::world::entity::mob::{self, ext, navigation};
use crate::world::entity::{Entity, EntityId, ServerPlayer};
use crate::world::item::{InteractionHand, InteractionResult, ItemStack, ItemStackExt as _};
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::{Block, SoundType};
use mc_data::{Attribute, GameEvent, Item, SoundEvent};
use mc_nbt::CompoundTag;
use std::sync::Arc;
use uuid::Uuid;

crate::define_accessor!(pub DATA_ID_FLAGS: Byte = AbstractHorse);

pub const CHEST_SLOT_OFFSET: i32 = 499;
pub const INVENTORY_SLOT_OFFSET: i32 = 500;
pub const BREEDING_CROSS_FACTOR: f64 = 0.15;
const BACKWARDS_MOVE_SPEED_FACTOR: f32 = 0.25;
const SIDEWAYS_MOVE_SPEED_FACTOR: f32 = 0.5;
pub const FLAG_TAME: i32 = 2;
pub const FLAG_BRED: i32 = 8;
pub const FLAG_EATING: i32 = 16;
pub const FLAG_STANDING: i32 = 32;
pub const FLAG_OPEN_MOUTH: i32 = 64;
pub const BABY_SCALE: f32 = 0.7;
pub const INVENTORY_ROWS: i32 = 3;

/// `generateMaxHealth(integerByBoundProvider)`.
pub fn generate_max_health(mut integer_by_bound: impl FnMut(i32) -> i32) -> f32 {
    let a = integer_by_bound(8);
    let b = integer_by_bound(9);
    15.0 + a as f32 + b as f32
}

/// `generateJumpStrength(probabilityProvider)`.
pub fn generate_jump_strength(mut probability: impl FnMut() -> f64) -> f64 {
    let a = probability();
    let b = probability();
    let c = probability();
    f64::from(0.4f32) + a * 0.2 + b * 0.2 + c * 0.2
}

/// `generateSpeed(probabilityProvider)`.
pub fn generate_speed(mut probability: impl FnMut() -> f64) -> f64 {
    let a = probability();
    let b = probability();
    let c = probability();
    (f64::from(0.45f32) + a * 0.3 + b * 0.3 + c * 0.3) * 0.25
}

pub fn min_movement_speed() -> f32 {
    generate_speed(|| 0.0) as f32
}
pub fn max_movement_speed() -> f32 {
    generate_speed(|| 1.0) as f32
}
pub fn min_jump_strength() -> f32 {
    generate_jump_strength(|| 0.0) as f32
}
pub fn max_jump_strength() -> f32 {
    generate_jump_strength(|| 1.0) as f32
}
pub fn min_health() -> f32 {
    generate_max_health(|_| 0)
}
pub fn max_health() -> f32 {
    generate_max_health(|i| i - 1)
}

/// The fields of `AbstractHorse`.
pub struct AbstractHorseData {
    pub eating_counter: i32,
    pub mouth_counter: i32,
    pub stand_counter: i32,
    pub tail_counter: i32,
    pub sprint_counter: i32,
    pub inventory: SimpleContainer,
    pub temper: i32,
    pub player_jump_pending_scale: f32,
    pub allow_stand_sliding: bool,
    pub eat_anim: f32,
    pub eat_anim_o: f32,
    pub stand_anim: f32,
    pub stand_anim_o: f32,
    pub mouth_anim: f32,
    pub mouth_anim_o: f32,
    pub can_gallop: bool,
    pub gallop_sound_counter: i32,
    /// `owner` (`EntityReference<LivingEntity>`).
    pub owner: Option<Uuid>,
    /// `getFirstPassenger() instanceof Player` / `instanceof Mob` (kept by `add_passenger` / `on_passenger_removed`).
    pub first_passenger_is_player: bool,
    pub first_passenger_is_mob: bool,
    /// Bumped by `createInventory()` (`MountInventory::inventory_epoch`).
    pub inventory_epoch: u32,
}

impl AbstractHorseData {
    /// The field initializers of `AbstractHorse` (`createInventory()` runs in the constructor, see [`AbstractHorse::create_inventory`]).
    pub fn new() -> Self {
        Self {
            eating_counter: 0,
            mouth_counter: 0,
            stand_counter: 0,
            tail_counter: 0,
            sprint_counter: 0,
            inventory: SimpleContainer::new(0),
            temper: 0,
            player_jump_pending_scale: 0.0,
            allow_stand_sliding: false,
            eat_anim: 0.0,
            eat_anim_o: 0.0,
            stand_anim: 0.0,
            stand_anim_o: 0.0,
            mouth_anim: 0.0,
            mouth_anim_o: 0.0,
            can_gallop: true,
            gallop_sound_counter: 0,
            owner: None,
            first_passenger_is_player: false,
            first_passenger_is_mob: false,
            inventory_epoch: 0,
        }
    }
}

impl Default for AbstractHorseData {
    fn default() -> Self {
        Self::new()
    }
}

/// `AbstractHorse` (the virtual members; see the module docs).
pub trait AbstractHorse: Entity {
    /// `getAmbientStandInterval()`.
    fn get_ambient_stand_interval(&self) -> i32 {
        self.get_ambient_sound_interval()
    }

    fn horse(&self) -> &AbstractHorseData;
    fn horse_mut(&mut self) -> &mut AbstractHorseData;

    /// `getEatingSound()`.
    fn get_eating_sound(&self) -> Option<SoundEvent> {
        None
    }
    /// `getAngrySound()`.
    fn get_angry_sound(&self) -> Option<SoundEvent> {
        None
    }
    /// `canPerformRearing()`.
    fn can_perform_rearing(&self) -> bool {
        true
    }
    /// `getInventoryColumns()`.
    fn get_inventory_columns(&self) -> i32 {
        0
    }
    /// `canEatGrass()`.
    fn can_eat_grass(&self) -> bool {
        true
    }
    /// `isMobControlled()`.
    fn is_mob_controlled(&self) -> bool {
        false
    }
    /// `getMaxTemper()`.
    fn get_max_temper(&self) -> i32 {
        100
    }
    /// `randomizeAttributes(random)`.
    fn randomize_attributes(&mut self, _random: &mut dyn RandomSource) {}
    /// `playJumpSound()`.
    fn play_jump_sound(&mut self, level: &mut ServerLevel) {
        self.play_sound(level, SoundEvent::ENTITY_HORSE_JUMP, 0.4, 1.0);
    }
    /// `playGallopSound(soundType)`.
    fn play_gallop_sound(&mut self, level: &mut ServerLevel, sound_type: &SoundType) {
        self.play_sound(
            level,
            SoundEvent::ENTITY_HORSE_GALLOP,
            sound_type.volume * 0.15,
            sound_type.pitch,
        );
    }
    /// `getAmbientStandSound()`.
    fn get_ambient_stand_sound(&self) -> Option<SoundEvent> {
        self.get_ambient_sound()
    }
    /// `handleEating(player, itemStack)`.
    fn handle_eating(
        &mut self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        stack: &ItemStack,
    ) -> bool {
        handle_eating(self, level, player, stack)
    }
    /// `doPlayerRide(player)`.
    fn do_player_ride(&mut self, level: &mut ServerLevel, player: &mut ServerPlayer) {
        do_player_ride(self, level, player);
    }
    /// `followMommy(level)`.
    fn follow_mommy(&mut self, level: &mut ServerLevel) {
        follow_mommy(self, level);
    }
    /// `createInventory()` (the chested horses and llamas resize it with their columns).
    fn create_inventory(&mut self) {
        create_inventory(self);
    }
    /// `SkeletonHorse.setTrap(trap)`: only the skeleton horse keeps the flag.
    fn set_trap(&mut self, _trap: bool) {}
}

impl<T: AbstractHorse + ?Sized> crate::world::inventory::MountInventory for T {
    fn get_inventory_columns(&self) -> i32 {
        AbstractHorse::get_inventory_columns(self)
    }
    fn inventory_epoch(&self) -> u32 {
        self.horse().inventory_epoch
    }
    fn can_use_slot(&self, slot: EquipmentSlot) -> bool {
        Entity::can_use_slot(self, slot)
    }
    fn is_equippable_in_slot(&self, stack: &ItemStack, slot: EquipmentSlot) -> bool {
        Entity::is_equippable_in_slot(self, stack, slot)
    }
    fn get_equipment_slot_item(&self, slot: EquipmentSlot) -> ItemStack {
        self.get_item_by_slot(slot)
    }
    fn set_equipment_slot_item(
        &mut self,
        level: &mut ServerLevel,
        slot: EquipmentSlot,
        stack: ItemStack,
    ) {
        let empty = stack.is_empty();
        mob::equipment::set_item_slot(self.as_dyn_mut(), level, slot, stack);
        if !empty {
            mob::equipment::set_guaranteed_drop(self.as_dyn_mut(), slot);
            if let Some(m) = self.mob_mut() {
                m.persistence_required = true;
            }
        }
    }
    fn is_llama(&self) -> bool {
        self.get_type() == mc_data::EntityType::LLAMA
            || self.get_type() == mc_data::EntityType::TRADER_LLAMA
    }
}

impl<T: AbstractHorse + ?Sized> crate::world::entity::ai::goal::AbstractHorseAccess for T {
    fn is_mob_controlled(&self, _level: &ServerLevel) -> bool {
        AbstractHorse::is_mob_controlled(self)
    }
    fn is_tamed(&self) -> bool {
        is_tamed(self)
    }
    fn stand_if_possible(&mut self, _level: &mut ServerLevel) {
        stand_if_possible(self);
    }
    fn get_ambient_stand_sound(&self) -> Option<SoundEvent> {
        AbstractHorse::get_ambient_stand_sound(self)
    }
    fn get_ambient_stand_interval(&self) -> i32 {
        AbstractHorse::get_ambient_stand_interval(self)
    }
    fn get_temper(&self) -> i32 {
        self.horse().temper
    }
    fn get_max_temper(&self) -> i32 {
        AbstractHorse::get_max_temper(self)
    }
    fn modify_temper(&mut self, amount: i32) -> i32 {
        modify_temper(self, amount)
    }
    fn tame_with_name(&mut self, level: &mut ServerLevel, player: EntityId) {
        if let Some((uuid, id)) = level.get_entity(player).map(|p| (p.uuid(), p.id())) {
            tame_with_name(self, level, uuid, id);
        }
    }
    fn make_mad(&mut self, level: &mut ServerLevel) {
        make_mad(self, level);
    }
    fn is_bred(&self) -> bool {
        is_bred(self)
    }
    fn can_parent(&self) -> bool {
        can_parent(self)
    }
    fn set_tamed(&mut self, flag: bool) {
        set_tamed(self, flag);
    }
    fn get_inventory_columns(&self) -> i32 {
        AbstractHorse::get_inventory_columns(self)
    }
    fn set_trap(&mut self, trap: bool) {
        AbstractHorse::set_trap(self, trap);
    }
}

fn flags(e: &(impl AbstractHorse + ?Sized)) -> i32 {
    i32::from(e.base().entity_data.get(DATA_ID_FLAGS))
}

/// `getFlag(flag)`.
pub fn get_flag(e: &(impl AbstractHorse + ?Sized), flag: i32) -> bool {
    (flags(e) & flag) != 0
}

/// `setFlag(flag, value)`.
pub fn set_flag(e: &mut (impl AbstractHorse + ?Sized), flag: i32, value: bool) {
    let current = flags(e);
    let next = if value {
        current | flag
    } else {
        current & !flag
    };
    e.base_mut().entity_data.set(DATA_ID_FLAGS, next as i8);
}

/// `isTamed()`.
pub fn is_tamed(e: &(impl AbstractHorse + ?Sized)) -> bool {
    get_flag(e, FLAG_TAME)
}

/// `setTamed(flag)`.
pub fn set_tamed(e: &mut (impl AbstractHorse + ?Sized), flag: bool) {
    set_flag(e, FLAG_TAME, flag);
}

/// `isEating()`.
pub fn is_eating(e: &(impl AbstractHorse + ?Sized)) -> bool {
    get_flag(e, FLAG_EATING)
}

/// `setEating(flag)`.
pub fn set_eating(e: &mut (impl AbstractHorse + ?Sized), flag: bool) {
    set_flag(e, FLAG_EATING, flag);
}

/// `isStanding()`.
pub fn is_standing(e: &(impl AbstractHorse + ?Sized)) -> bool {
    get_flag(e, FLAG_STANDING)
}

/// `isBred()`.
pub fn is_bred(e: &(impl AbstractHorse + ?Sized)) -> bool {
    get_flag(e, FLAG_BRED)
}

/// `setBred(flag)`.
pub fn set_bred(e: &mut (impl AbstractHorse + ?Sized), flag: bool) {
    set_flag(e, FLAG_BRED, flag);
}

/// `isSaddled()`.
pub fn is_saddled(e: &(impl AbstractHorse + ?Sized)) -> bool {
    !e.get_item_by_slot(EquipmentSlot::Saddle).is_empty()
}

/// `isWearingBodyArmor()`.
pub fn is_wearing_body_armor(e: &(impl AbstractHorse + ?Sized)) -> bool {
    !e.get_item_by_slot(EquipmentSlot::Body).is_empty()
}

/// `setOwner(owner)`.
pub fn set_owner(e: &mut (impl AbstractHorse + ?Sized), owner: Option<Uuid>) {
    e.horse_mut().owner = owner;
}

/// `getOwner()` (`OwnableEntity`): the living owner while it is in the level.
pub fn get_owner(e: &(impl AbstractHorse + ?Sized), level: &ServerLevel) -> Option<EntityId> {
    let uuid = e.horse().owner?;
    level
        .get_entity_by_uuid(uuid)
        .filter(|o| o.living().is_some())
        .map(|o| o.id())
}

/// `getTemper()` / `setTemper(temper)` / `modifyTemper(amount)`.
pub fn modify_temper(e: &mut (impl AbstractHorse + ?Sized), amount: i32) -> i32 {
    let temper = mth::clamp_i32(e.horse().temper + amount, 0, e.get_max_temper());
    e.horse_mut().temper = temper;
    temper
}

/// `getInventorySize()` (`AbstractMountInventoryMenu.getInventorySize(columns)`).
pub fn get_inventory_size(e: &(impl AbstractHorse + ?Sized)) -> i32 {
    e.get_inventory_columns() * 3
}

/// `createInventory()`: a new container of the current size that keeps the old content.
pub fn create_inventory(e: &mut (impl AbstractHorse + ?Sized)) {
    let size = get_inventory_size(e) as usize;
    e.horse_mut().inventory_epoch += 1;
    let old = std::mem::replace(&mut e.horse_mut().inventory, SimpleContainer::new(size));
    let max = old.get_container_size().min(size);
    for slot in 0..max {
        let stack = old.get_item(slot);
        if !stack.is_empty() {
            e.horse_mut().inventory.set_item(slot, stack.copy());
        }
    }
}

/// `registerGoals()` of `AbstractHorse` with the class's `addBehaviourGoals()`.
pub fn register_goals<E: AbstractHorse + Sized>(
    e: &mut E,
    add_behaviour_goals: impl FnOnce(&mut E),
) {
    let breed = BreedGoal::with_class_test(
        1.0,
        Arc::new(|other: &dyn Entity| other.as_abstract_horse().is_some()),
    );
    let follow_parent = FollowParentGoal::new(e, 1.0);
    let look_at = LookAtPlayerGoal::new_player(e, 6.0);
    let can_rear = e.can_perform_rearing();
    let random_stand = RandomStandGoal::new(e.as_dyn());
    {
        let goals = &mut e.mob_mut().expect("mob").goal_selector;
        goals.add_goal(1, Box::new(RunAroundLikeCrazyGoal::new(1.2)));
        goals.add_goal(2, Box::new(breed));
        goals.add_goal(4, Box::new(follow_parent));
        goals.add_goal(6, Box::new(WaterAvoidingRandomStrollGoal::new(0.7)));
        goals.add_goal(7, Box::new(look_at));
        goals.add_goal(8, Box::new(RandomLookAroundGoal::new()));
        if can_rear {
            goals.add_goal(9, Box::new(random_stand));
        }
    }
    add_behaviour_goals(e);
}

/// `addBehaviourGoals()` of `AbstractHorse`.
pub fn add_behaviour_goals<E: AbstractHorse + Sized>(e: &mut E) {
    let float = FloatGoal::new(e);
    let goals = &mut e.mob_mut().expect("mob").goal_selector;
    goals.add_goal(0, Box::new(float));
    goals.add_goal(1, Box::new(MountPanicGoal::new(1.2)));
    goals.add_goal(
        3,
        Box::new(TemptGoal::new(
            1.25,
            Arc::new(|stack: &ItemStack| super::item_in_tag(stack, "horse_tempt_items")),
            false,
        )),
    );
}

/// `AbstractHorse.MountPanicGoal`: no panic while a mob rides.
pub struct MountPanicGoal {
    inner: PanicGoal,
}

impl MountPanicGoal {
    pub fn new(speed_modifier: f64) -> Self {
        Self {
            inner: PanicGoal::new(speed_modifier),
        }
    }
}

impl crate::world::entity::ai::goal::Goal for MountPanicGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        if mob
            .as_abstract_horse()
            .is_some_and(|h| h.is_mob_controlled(level))
        {
            return false;
        }
        self.inner.can_use_impl(mob, level)
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

    fn flags(&self) -> crate::world::entity::ai::goal::GoalFlags {
        self.inner.flags()
    }

    fn as_panic_goal(&self) -> Option<&PanicGoal> {
        Some(&self.inner)
    }
}

/// `canUseSlot(slot)` of `AbstractHorse`.
pub fn can_use_slot(e: &(impl AbstractHorse + ?Sized), slot: EquipmentSlot) -> bool {
    if slot != EquipmentSlot::Saddle {
        true
    } else {
        e.is_alive() && !e.is_baby() && is_tamed(e)
    }
}

/// `equipBodyArmor(player, itemStack)`.
pub fn equip_body_armor(
    e: &mut (impl AbstractHorse + ?Sized),
    level: &mut ServerLevel,
    player: &mut ServerPlayer,
    hand: InteractionHand,
) {
    let stack = player.get_item_in_hand(hand);
    if e.is_equippable_in_slot(&stack, EquipmentSlot::Body) {
        let one = mob::interact::with_hand_stack(player, hand, |s, p| {
            s.consume_and_return(1, p.has_infinite_materials())
        });
        mob::equipment::set_item_slot_and_drop_when_killed(
            e.as_dyn_mut(),
            level,
            EquipmentSlot::Body,
            one,
        );
    }
}

/// `eating()`.
fn eating(e: &mut (impl AbstractHorse + ?Sized), level: &mut ServerLevel) {
    open_mouth(e);
    if !e.base().is_silent()
        && let Some(sound) = e.get_eating_sound()
    {
        let pitch =
            1.0 + (e.base_mut().random.next_float() - e.base_mut().random.next_float()) * 0.2;
        let p = e.position();
        let source = e.get_sound_source();
        level.play_sound_at(None, p.x, p.y, p.z, sound, source, 1.0, pitch);
    }
}

/// `openMouth()`.
fn open_mouth(e: &mut (impl AbstractHorse + ?Sized)) {
    e.horse_mut().mouth_counter = 1;
    set_flag(e, FLAG_OPEN_MOUTH, true);
}

/// `causeFallDamage(fallDistance, damageModifier, damageSource)` of `AbstractHorse`.
pub fn cause_fall_damage(
    e: &mut (impl AbstractHorse + ?Sized),
    level: &mut ServerLevel,
    fall_distance: f64,
    damage_modifier: f32,
    source: &DamageSource,
) -> bool {
    if fall_distance > 1.0 {
        let sound = if e.is_baby() {
            SoundEvent::ENTITY_BABY_HORSE_LAND
        } else {
            SoundEvent::ENTITY_HORSE_LAND
        };
        e.play_sound(level, sound, 0.4, 1.0);
    }
    let dmg =
        living_entity::damage::calculate_fall_damage(e.as_dyn(), fall_distance, damage_modifier);
    if dmg <= 0 {
        false
    } else {
        e.hurt_server(level, source, dmg as f32);
        crate::world::entity::entity::propagate_fall_to_passengers(
            e.as_dyn_mut(),
            level,
            fall_distance,
            damage_modifier,
            source,
        );
        living_entity::damage::play_block_fall_sound(e.as_dyn_mut(), level);
        true
    }
}

/// `hurtServer(level, source, damage)` of `AbstractHorse`.
pub fn hurt_server(
    e: &mut (impl AbstractHorse + ?Sized),
    level: &mut ServerLevel,
    source: &DamageSource,
    damage: f32,
) -> bool {
    let was_hurt = living_entity::damage::hurt_server(e.as_dyn_mut(), level, source, damage);
    if was_hurt && e.base_mut().random.next_int_bounded(3) == 0 {
        stand_if_possible(e);
    }
    was_hurt
}

fn is_wood_sound_type(sound_type: &SoundType) -> bool {
    [
        SoundEvent::BLOCK_WOOD_STEP,
        SoundEvent::BLOCK_NETHER_WOOD_STEP,
        SoundEvent::BLOCK_STEM_STEP,
        SoundEvent::BLOCK_CHERRY_WOOD_STEP,
        SoundEvent::BLOCK_BAMBOO_WOOD_STEP,
    ]
    .contains(&sound_type.step_sound)
}

/// `playStepSound(pos, blockState)` of `AbstractHorse`.
pub fn play_step_sound(
    e: &mut (impl AbstractHorse + ?Sized),
    level: &mut ServerLevel,
    pos: BlockPos,
    block_state: mc_data::block::BlockState,
) {
    use crate::world::level::level_access::BlockGetter as _;
    if !block_state.liquid() {
        let above_state = level.get_block_state(pos.above());
        let mut sound_type = block_state.get_sound_type();
        if above_state.block() == Block::SNOW {
            sound_type = above_state.get_sound_type();
        }
        if e.base().is_vehicle() && e.horse().can_gallop {
            e.horse_mut().gallop_sound_counter += 1;
            let counter = e.horse().gallop_sound_counter;
            if counter > 5 && counter % 3 == 0 {
                e.play_gallop_sound(level, sound_type);
            } else if counter <= 5 {
                e.play_sound(
                    level,
                    SoundEvent::ENTITY_HORSE_STEP_WOOD,
                    sound_type.volume * 0.15,
                    sound_type.pitch,
                );
            }
        } else if is_wood_sound_type(sound_type) {
            e.play_sound(
                level,
                SoundEvent::ENTITY_HORSE_STEP_WOOD,
                sound_type.volume * 0.15,
                sound_type.pitch,
            );
        } else {
            let step = if e.is_baby() {
                SoundEvent::ENTITY_BABY_HORSE_STEP
            } else {
                SoundEvent::ENTITY_HORSE_STEP
            };
            e.play_sound(level, step, sound_type.volume * 0.15, sound_type.pitch);
        }
    }
}

/// `openCustomInventoryScreen(player)`. The screen reads the horse, which is out of its level while it runs
/// (inside `interact`, the `OPEN_INVENTORY` hook), so the horse is lent for the open.
pub fn open_custom_inventory_screen(
    e: &mut (impl AbstractHorse + ?Sized),
    level: &mut ServerLevel,
    player: &mut ServerPlayer,
) {
    if (!e.base().is_vehicle() || e.base().passengers.contains(&player.id())) && is_tamed(e) {
        let id = e.id();
        level.lend(e.as_dyn_mut(), |level| {
            player.open_horse_inventory(level, id)
        });
    }
}

/// `fedFood(player, itemStack)`.
pub fn fed_food(
    e: &mut (impl AbstractHorse + ?Sized),
    level: &mut ServerLevel,
    player: &mut ServerPlayer,
    hand: InteractionHand,
) -> InteractionResult {
    let stack = player.get_item_in_hand(hand);
    let ate_food = e.handle_eating(level, player, &stack);
    if ate_food {
        mob::interact::with_hand_stack(player, hand, |s, p| {
            s.consume(1, p.has_infinite_materials())
        });
    }
    if !ate_food {
        InteractionResult::PASS
    } else {
        InteractionResult::SUCCESS_SERVER
    }
}

/// `handleEating(player, itemStack)` of `AbstractHorse`.
pub fn handle_eating(
    e: &mut (impl AbstractHorse + ?Sized),
    level: &mut ServerLevel,
    player: &mut ServerPlayer,
    stack: &ItemStack,
) -> bool {
    let mut item_used = false;
    let mut heal = 0.0f32;
    let mut age_up = 0;
    let mut temper = 0;
    if stack.is(Item::WHEAT) {
        heal = 2.0;
        age_up = 20;
        temper = 3;
    } else if stack.is(Item::SUGAR) {
        heal = 1.0;
        age_up = 30;
        temper = 3;
    } else if stack.is(Item::HAY_BLOCK) {
        heal = 20.0;
        age_up = 180;
    } else if stack.is(Item::APPLE) {
        heal = 3.0;
        age_up = 60;
        temper = 3;
    } else if stack.is(Item::RED_MUSHROOM) {
        heal = 3.0;
        age_up = 0;
        temper = 3;
    } else if stack.is(Item::CARROT) {
        heal = 3.0;
        age_up = 60;
        temper = 3;
    } else if stack.is(Item::GOLDEN_CARROT) {
        heal = 4.0;
        age_up = 60;
        temper = 5;
        if is_tamed(e)
            && mob::ageable::get_age(e.as_dyn()) == 0
            && !mob::animal::is_in_love(e.as_dyn())
        {
            item_used = true;
            mob::animal::set_in_love(e.as_dyn_mut(), level, Some(&*player));
        }
    } else if stack.is(Item::GOLDEN_APPLE) || stack.is(Item::ENCHANTED_GOLDEN_APPLE) {
        heal = 10.0;
        age_up = 240;
        temper = 10;
        if is_tamed(e)
            && mob::ageable::get_age(e.as_dyn()) == 0
            && !mob::animal::is_in_love(e.as_dyn())
        {
            item_used = true;
            mob::animal::set_in_love(e.as_dyn_mut(), level, Some(&*player));
        }
    }
    if e.get_health() < l(e.as_dyn()).get_max_health() && heal > 0.0 {
        e.heal(level, heal);
        item_used = true;
    }
    if e.is_baby() && age_up > 0 && !mob::ageable::is_age_locked(e.as_dyn()) {
        mob::ageable::age_up(e.as_dyn_mut(), level, age_up, false);
        item_used = true;
    }
    if temper > 0 && (item_used || !is_tamed(e)) && e.horse().temper < e.get_max_temper() {
        modify_temper(e, temper);
        item_used = true;
    }
    if item_used {
        eating(e, level);
        e.game_event(level, GameEvent::EAT);
    }
    item_used
}

/// `doPlayerRide(player)`.
pub fn do_player_ride(
    e: &mut (impl AbstractHorse + ?Sized),
    level: &mut ServerLevel,
    player: &mut ServerPlayer,
) {
    set_eating(e, false);
    clear_standing(e);
    player.set_y_rot(e.base().y_rot());
    player.set_x_rot(e.base().x_rot());
    level.start_riding_held(player, e.as_dyn_mut(), false, true);
}

/// `isImmobile()` of `AbstractHorse`.
pub fn is_immobile(e: &(impl AbstractHorse + ?Sized)) -> bool {
    (e.is_dead_or_dying() && e.base().is_vehicle() && is_saddled(e))
        || is_eating(e)
        || is_standing(e)
}

/// `dropEquipment(level)` of `AbstractHorse`.
pub fn drop_equipment(e: &mut (impl AbstractHorse + ?Sized), level: &mut ServerLevel) {
    use crate::world::item::enchantment::enchantment_effect_components::EffectComponent;
    let stacks: Vec<ItemStack> = (0..e.horse().inventory.get_container_size())
        .map(|i| e.horse().inventory.get_item(i).clone())
        .collect();
    for stack in stacks {
        if !stack.is_empty()
            && !crate::world::item::enchantment_helper::has(
                &level.registries,
                &stack,
                EffectComponent::prevent_equipment_drop,
            )
        {
            crate::world::entity::item::spawn_at_location(level, e.as_dyn(), stack, Vec3::ZERO);
        }
    }
}

/// `aiStep()` of `AbstractHorse`.
pub fn ai_step(e: &mut (impl AbstractHorse + ?Sized), level: &mut ServerLevel) {
    if e.base_mut().random.next_int_bounded(200) == 0 {
        e.horse_mut().tail_counter = 1;
    }
    mob::tick::ai_step(e.as_dyn_mut(), level);
    if e.is_alive() {
        if e.base_mut().random.next_int_bounded(900) == 0 && l(e.as_dyn()).death_time == 0 {
            e.heal(level, 1.0);
        }
        if e.can_eat_grass() {
            use crate::world::level::level_access::BlockGetter as _;
            if !is_eating(e)
                && !e.base().is_vehicle()
                && e.base_mut().random.next_int_bounded(300) == 0
                && level.get_block_state(e.block_position().below()).block() == Block::GRASS_BLOCK
            {
                set_eating(e, true);
            }
            if is_eating(e) {
                e.horse_mut().eating_counter += 1;
                if e.horse().eating_counter > 50 {
                    e.horse_mut().eating_counter = 0;
                    set_eating(e, false);
                }
            }
        }
        e.follow_mommy(level);
    }
}

/// `followMommy(level)`.
pub fn follow_mommy(e: &mut (impl AbstractHorse + ?Sized), level: &mut ServerLevel) {
    if is_bred(e) && e.is_baby() && !is_eating(e) {
        let conditions = TargetingConditions::for_non_combat()
            .range(16.0)
            .ignore_line_of_sight()
            .selector(Some(Arc::new(
                |target: &dyn Entity, _level: &ServerLevel| {
                    target.as_abstract_horse().is_some_and(|h| h.is_bred())
                },
            )));
        let p = e.position();
        let bb = e.base().bounding_box().inflate_scalar(16.0);
        let candidates = ext::get_nearby_entities(
            level,
            |t| t.as_abstract_horse().is_some(),
            &conditions,
            e.as_dyn(),
            &bb,
        );
        if let Some(mommy) =
            ext::get_nearest_entity(level, &candidates, &conditions, e.as_dyn(), p.x, p.y, p.z)
        {
            let (mommy_pos, dist) = match level.get_entity(mommy) {
                Some(m) => (
                    m.block_position(),
                    e.base().distance_to_sqr_vec(m.position()),
                ),
                None => return,
            };
            if dist > 4.0 {
                navigation::create_path_entity(e.as_dyn_mut(), level, mommy_pos, 0);
            }
        }
    }
}

/// `tick()` of `AbstractHorse`.
pub fn tick(e: &mut (impl AbstractHorse + ?Sized), level: &mut ServerLevel) {
    mob::tick::tick(e.as_dyn_mut(), level);
    if e.horse().mouth_counter > 0 {
        e.horse_mut().mouth_counter += 1;
        if e.horse().mouth_counter > 30 {
            e.horse_mut().mouth_counter = 0;
            set_flag(e, FLAG_OPEN_MOUTH, false);
        }
    }
    if e.horse().stand_counter > 0 {
        e.horse_mut().stand_counter -= 1;
        if e.horse().stand_counter <= 0 {
            clear_standing(e);
        }
    }
    if e.horse().tail_counter > 0 {
        e.horse_mut().tail_counter += 1;
        if e.horse().tail_counter > 8 {
            e.horse_mut().tail_counter = 0;
        }
    }
    if e.horse().sprint_counter > 0 {
        e.horse_mut().sprint_counter += 1;
        if e.horse().sprint_counter > 300 {
            e.horse_mut().sprint_counter = 0;
        }
    }
    let eating = is_eating(e);
    let standing = is_standing(e);
    let mouth_open = get_flag(e, FLAG_OPEN_MOUTH);
    let h = e.horse_mut();
    h.eat_anim_o = h.eat_anim;
    if eating {
        h.eat_anim += (1.0 - h.eat_anim) * 0.4 + 0.05;
        if h.eat_anim > 1.0 {
            h.eat_anim = 1.0;
        }
    } else {
        h.eat_anim += (0.0 - h.eat_anim) * 0.4 - 0.05;
        if h.eat_anim < 0.0 {
            h.eat_anim = 0.0;
        }
    }
    h.stand_anim_o = h.stand_anim;
    if standing {
        h.eat_anim = 0.0;
        h.eat_anim_o = h.eat_anim;
        h.stand_anim += (1.0 - h.stand_anim) * 0.4 + 0.05;
        if h.stand_anim > 1.0 {
            h.stand_anim = 1.0;
        }
    } else {
        h.allow_stand_sliding = false;
        h.stand_anim +=
            (0.8 * h.stand_anim * h.stand_anim * h.stand_anim - h.stand_anim) * 0.6 - 0.05;
        if h.stand_anim < 0.0 {
            h.stand_anim = 0.0;
        }
    }
    h.mouth_anim_o = h.mouth_anim;
    if mouth_open {
        h.mouth_anim += (1.0 - h.mouth_anim) * 0.7 + 0.05;
        if h.mouth_anim > 1.0 {
            h.mouth_anim = 1.0;
        }
    } else {
        h.mouth_anim += (0.0 - h.mouth_anim) * 0.7 - 0.05;
        if h.mouth_anim < 0.0 {
            h.mouth_anim = 0.0;
        }
    }
}

/// `mobInteract(player, hand)` of `AbstractHorse`.
pub fn mob_interact(
    e: &mut (impl AbstractHorse + ?Sized),
    level: &mut ServerLevel,
    player: &mut ServerPlayer,
    hand: InteractionHand,
) -> InteractionResult {
    if e.base().is_vehicle() || e.is_baby() {
        return mob::interact::mob_interact(e.as_dyn_mut(), level, player, hand);
    }
    if is_tamed(e) && player.is_shift_key_down() {
        open_custom_inventory_screen(e, level, player);
        return InteractionResult::SUCCESS;
    }
    let item_stack = player.get_item_in_hand(hand);
    if !item_stack.is_empty() {
        let result = mob::interact::with_hand_stack(player, hand, |s, p| {
            crate::world::item::interact_living_entity(level, s, p, e.as_dyn_mut(), hand)
        });
        if result.consumes_action() {
            return result;
        }
        if e.is_equippable_in_slot(&item_stack, EquipmentSlot::Body) && !is_wearing_body_armor(e) {
            equip_body_armor(e, level, player, hand);
            return InteractionResult::SUCCESS;
        }
    }
    e.do_player_ride(level, player);
    InteractionResult::SUCCESS
}

/// `setStanding(ticks)`.
pub fn set_standing(e: &mut (impl AbstractHorse + ?Sized), ticks: i32) {
    set_eating(e, false);
    set_flag(e, FLAG_STANDING, true);
    e.horse_mut().stand_counter = ticks;
}

/// `clearStanding()`.
pub fn clear_standing(e: &mut (impl AbstractHorse + ?Sized)) {
    set_flag(e, FLAG_STANDING, false);
    e.horse_mut().stand_counter = 0;
}

/// `standIfPossible()` (`isEffectiveAi() || !level.isClientSide()`: always true on the server).
pub fn stand_if_possible(e: &mut (impl AbstractHorse + ?Sized)) {
    if e.can_perform_rearing() {
        set_standing(e, 20);
    }
}

/// `makeMad()`.
pub fn make_mad(e: &mut (impl AbstractHorse + ?Sized), level: &mut ServerLevel) {
    if !is_standing(e) {
        stand_if_possible(e);
        if let Some(sound) = e.get_angry_sound() {
            crate::world::entity::living_entity::damage::make_sound(e.as_dyn_mut(), level, sound);
        }
    }
}

/// `tameWithName(player)`.
pub fn tame_with_name(
    e: &mut (impl AbstractHorse + ?Sized),
    level: &mut ServerLevel,
    player_uuid: Uuid,
    player: EntityId,
) -> bool {
    set_owner(e, Some(player_uuid));
    set_tamed(e, true);
    ext::trigger_tame_animal_by_id(level, player, e.as_dyn());
    level.broadcast_entity_event(e.id(), 7);
    true
}

/// `tickRidden(controller, riddenInput)`.
pub fn tick_ridden(
    e: &mut (impl AbstractHorse + ?Sized),
    level: &mut ServerLevel,
    controller: EntityId,
    ridden_input: Vec3,
) {
    let Some((x_rot, y_rot)) = level
        .get_entity(controller)
        .map(|c| (c.base().x_rot(), c.base().y_rot()))
    else {
        return;
    };
    // `getRiddenRotation(controller)`: `Vec2(controller.xRot * 0.5F, controller.yRot)`
    e.set_rot(y_rot, x_rot * 0.5);
    let y = e.base().y_rot();
    e.base_mut().y_rot_o = y;
    lm(e.as_dyn_mut()).y_body_rot = y;
    e.set_y_head_rot(y);
    if e.is_local_instance_authoritative() {
        if ridden_input.z <= 0.0 {
            e.horse_mut().gallop_sound_counter = 0;
        }
        if e.base().on_ground() {
            if e.horse().player_jump_pending_scale > 0.0 && !l(e.as_dyn()).jumping {
                let scale = e.horse().player_jump_pending_scale;
                execute_riders_jump(e, level, scale, ridden_input);
            }
            e.horse_mut().player_jump_pending_scale = 0.0;
        }
    }
}

/// `getRiddenInput(controller, selfInput)`.
pub fn get_ridden_input(
    e: &(impl AbstractHorse + ?Sized),
    level: &ServerLevel,
    controller: EntityId,
    _self_input: Vec3,
) -> Vec3 {
    if e.base().on_ground()
        && e.horse().player_jump_pending_scale == 0.0
        && is_standing(e)
        && !e.horse().allow_stand_sliding
    {
        return Vec3::ZERO;
    }
    let Some(c) = level.get_entity(controller).and_then(|c| c.living()) else {
        return Vec3::ZERO;
    };
    let sideways = c.xxa * SIDEWAYS_MOVE_SPEED_FACTOR;
    let mut forward = c.zza;
    if forward <= 0.0 {
        forward *= BACKWARDS_MOVE_SPEED_FACTOR;
    }
    Vec3::new(f64::from(sideways), 0.0, f64::from(forward))
}

/// `executeRidersJump(amount, input)`.
pub fn execute_riders_jump(
    e: &mut (impl AbstractHorse + ?Sized),
    level: &ServerLevel,
    amount: f32,
    input: Vec3,
) {
    let impulse = f64::from(living_entity::jump::get_jump_power_with(
        e.as_dyn(),
        level,
        amount,
    ));
    let movement = e.base().delta_movement();
    e.base_mut()
        .set_delta_movement(Vec3::new(movement.x, impulse, movement.z));
    e.base_mut().needs_sync = true;
    if input.z > 0.0 {
        let y_rot = e.base().y_rot() * (std::f64::consts::PI / 180.0) as f32;
        let sin = mth::sin(f64::from(y_rot));
        let cos = mth::cos(f64::from(y_rot));
        let dm = e.base().delta_movement();
        e.base_mut().set_delta_movement(dm.add(Vec3::new(
            f64::from(-0.4f32 * sin * amount),
            0.0,
            f64::from(0.4f32 * cos * amount),
        )));
    }
}

/// `addAdditionalSaveData(output)` of `AbstractHorse`.
pub fn add_additional_save_data(e: &(impl AbstractHorse + ?Sized), out: &mut CompoundTag) {
    mob::nbt::add_additional_save_data(e.as_dyn(), out);
    out.put_boolean("EatingHaystack", is_eating(e));
    out.put_boolean("Bred", is_bred(e));
    out.put_int("Temper", e.horse().temper);
    out.put_boolean("Tame", is_tamed(e));
    if let Some(owner) = e.horse().owner {
        out.put_int_array(
            "Owner",
            mc_core::uuid_util::uuid_to_int_array(&owner).to_vec(),
        );
    }
}

/// `readAdditionalSaveData(input)` of `AbstractHorse`.
pub fn read_additional_save_data(e: &mut (impl AbstractHorse + ?Sized), input: &CompoundTag) {
    mob::nbt::read_additional_save_data(e.as_dyn_mut(), input);
    set_eating(e, input.get_boolean_or("EatingHaystack", false));
    set_bred(e, input.get_boolean_or("Bred", false));
    e.horse_mut().temper = input.get_int_or("Temper", 0);
    set_tamed(e, input.get_boolean_or("Tame", false));
    e.horse_mut().owner = crate::world::entity::entity::read_uuid(input.get("Owner"));
}

/// `canParent()`.
pub fn can_parent(e: &(impl AbstractHorse + ?Sized)) -> bool {
    !e.base().is_vehicle()
        && !e.base().is_passenger()
        && is_tamed(e)
        && !e.is_baby()
        && e.get_health() >= l(e.as_dyn()).get_max_health()
        && mob::animal::is_in_love(e.as_dyn())
}

/// `setOffspringAttributes(partner, baby)`.
pub fn set_offspring_attributes(
    e: &mut (impl AbstractHorse + ?Sized),
    level: &ServerLevel,
    partner: EntityId,
    baby: &mut dyn Entity,
) {
    set_offspring_attribute(
        e,
        level,
        partner,
        baby,
        Attribute::MAX_HEALTH,
        f64::from(min_health()),
        f64::from(max_health()),
    );
    set_offspring_attribute(
        e,
        level,
        partner,
        baby,
        Attribute::JUMP_STRENGTH,
        f64::from(min_jump_strength()),
        f64::from(max_jump_strength()),
    );
    set_offspring_attribute(
        e,
        level,
        partner,
        baby,
        Attribute::MOVEMENT_SPEED,
        f64::from(min_movement_speed()),
        f64::from(max_movement_speed()),
    );
}

fn set_offspring_attribute(
    e: &mut (impl AbstractHorse + ?Sized),
    level: &ServerLevel,
    partner: EntityId,
    baby: &mut dyn Entity,
    attribute: Attribute,
    min: f64,
    max: f64,
) {
    let own = l(e.as_dyn()).attributes.get_base_value(attribute);
    let other = level
        .get_entity(partner)
        .and_then(|p| p.living())
        .map_or(own, |p| p.attributes.get_base_value(attribute));
    let new_value = create_offspring_attribute(own, other, min, max, &mut e.base_mut().random);
    if let Some(instance) = lm(baby).attributes.get_instance(attribute) {
        instance.set_base_value(new_value);
    }
}

/// `createOffspringAttribute(parentAValue, parentBValue, min, max, random)`.
pub fn create_offspring_attribute(
    parent_a_value: f64,
    parent_b_value: f64,
    attribute_range_min: f64,
    attribute_range_max: f64,
    random: &mut dyn RandomSource,
) -> f64 {
    assert!(
        attribute_range_max > attribute_range_min,
        "Incorrect range for an attribute"
    );
    let parent_a_value = mth::clamp_f64(parent_a_value, attribute_range_min, attribute_range_max);
    let parent_b_value = mth::clamp_f64(parent_b_value, attribute_range_min, attribute_range_max);
    let margin = 0.15 * (attribute_range_max - attribute_range_min);
    let range = (parent_a_value - parent_b_value).abs() + margin * 2.0;
    let average = (parent_a_value + parent_b_value) / 2.0;
    let a = random.next_double();
    let b = random.next_double();
    let c = random.next_double();
    let baby_quality = (a + b + c) / 3.0 - 0.5;
    let new_value = average + range * baby_quality;
    if new_value > attribute_range_max {
        attribute_range_max - (new_value - attribute_range_max)
    } else if new_value < attribute_range_min {
        attribute_range_min + (attribute_range_min - new_value)
    } else {
        new_value
    }
}

/// `onPlayerJump(jumpAmount)`.
pub fn on_player_jump(e: &mut (impl AbstractHorse + ?Sized), mut jump_amount: i32) {
    if is_saddled(e) {
        if jump_amount < 0 {
            jump_amount = 0;
        } else {
            e.horse_mut().allow_stand_sliding = true;
            stand_if_possible(e);
        }
        e.horse_mut().player_jump_pending_scale = get_player_jump_pending_scale(jump_amount);
    }
}

/// `PlayerRideableJumping.getPlayerJumpPendingScale(jumpAmount)`.
pub fn get_player_jump_pending_scale(jump_amount: i32) -> f32 {
    if jump_amount >= 90 {
        1.0
    } else {
        0.4 + 0.4 * jump_amount as f32 / 90.0
    }
}

/// `handleStartJump(jumpScale)`.
pub fn handle_start_jump(e: &mut (impl AbstractHorse + ?Sized), level: &mut ServerLevel) {
    e.horse_mut().allow_stand_sliding = true;
    stand_if_possible(e);
    e.play_jump_sound(level);
}

/// `getControllingPassenger()`.
pub fn get_controlling_passenger(e: &(impl AbstractHorse + ?Sized)) -> Option<EntityId> {
    let first = e.base().passengers.first().copied()?;
    if is_saddled(e) && e.horse().first_passenger_is_player {
        Some(first)
    } else {
        (e.horse().first_passenger_is_mob && !e.mob().is_some_and(|m| m.is_no_ai()))
            .then_some(first)
    }
}

/// `addPassenger(passenger)` bookkeeping (the list update is `Entity.addPassenger`).
pub fn add_passenger(
    e: &mut (impl AbstractHorse + ?Sized),
    passenger: &dyn Entity,
    first_passenger_is_player: bool,
) {
    let id = passenger.id();
    let is_player = passenger.is_player();
    let is_mob = passenger.mob().is_some();
    let passengers = &mut e.base_mut().passengers;
    let becomes_first = passengers.is_empty() || (is_player && !first_passenger_is_player);
    if becomes_first {
        passengers.insert(0, id);
    } else {
        passengers.push(id);
    }
    if becomes_first {
        let h = e.horse_mut();
        h.first_passenger_is_player = is_player;
        h.first_passenger_is_mob = is_mob;
    }
}

/// `onPassengerRemoved`: re-derive the first passenger's class.
pub fn on_passenger_removed(e: &mut (impl AbstractHorse + ?Sized), level: &ServerLevel) {
    let first = e
        .base()
        .passengers
        .first()
        .copied()
        .and_then(|id| level.get_entity(id));
    let (is_player, is_mob) = first.map_or((false, false), |p| (p.is_player(), p.mob().is_some()));
    let h = e.horse_mut();
    h.first_passenger_is_player = is_player;
    h.first_passenger_is_mob = is_mob;
}

/// `addPassenger(passenger)`: `passenger.absSnapRotationTo(getViewYRot(0), getViewXRot(0))`.
pub fn after_add_passenger(e: &(impl AbstractHorse + ?Sized), passenger: &mut dyn Entity) {
    passenger.abs_snap_rotation_to(e.get_view_y_rot(0.0), e.get_view_x_rot(0.0));
}

/// `positionRider(passenger, moveFunction)`.
pub fn position_rider_with(
    e: &(impl AbstractHorse + ?Sized),
    passenger: &mut dyn Entity,
    move_function: crate::world::entity::entity::MoveFunction,
) {
    let position = e.get_passenger_riding_position(passenger.as_dyn());
    let offset = passenger.get_vehicle_attachment_point(e.as_dyn());
    move_function.apply(
        passenger,
        position.x - offset.x,
        position.y - offset.y,
        position.z - offset.z,
    );
    if let Some(living) = passenger.living_mut() {
        living.y_body_rot = l(e.as_dyn()).y_body_rot;
    }
}

/// `getPassengerAttachmentPoint(passenger, dimensions, scale)` of `AbstractHorse`.
pub fn get_passenger_attachment_point(
    e: &(impl AbstractHorse + ?Sized),
    base: Vec3,
    scale: f32,
) -> Vec3 {
    let stand = f64::from(e.horse().stand_anim_o) * f64::from(scale);
    let y_rot = -e.base().y_rot() * (std::f64::consts::PI / 180.0) as f32;
    base.add(Vec3::new(0.0, 0.15 * stand, -0.7 * stand).y_rot(y_rot))
}

/// `finalizeSpawn(...)` up to `super.finalizeSpawn` (the caller runs `Animal`'s next): the default group data and
/// `randomizeAttributes`.
pub fn finalize_spawn_prepare(
    e: &mut (impl AbstractHorse + ?Sized),
    level: &mut dyn crate::world::level::server_level_accessor::ServerLevelAccessor,
    group_data: Option<Box<dyn crate::world::entity::spawn_group_data::SpawnGroupData>>,
) -> Option<Box<dyn crate::world::entity::spawn_group_data::SpawnGroupData>> {
    let group_data =
        group_data.unwrap_or_else(|| Box::new(mob::spawn::AgeableMobGroupData::with_chance(0.2)));
    e.randomize_attributes(level.random());
    Some(group_data)
}

/// `getSlot(slot)` of `AbstractHorse`: the inventory index of an inventory slot id.
fn inventory_slot_index(e: &(impl AbstractHorse + ?Sized), slot: i32) -> Option<usize> {
    let inventory_slot = slot - INVENTORY_SLOT_OFFSET;
    (inventory_slot >= 0 && (inventory_slot as usize) < e.horse().inventory.get_container_size())
        .then_some(inventory_slot as usize)
}

/// `getSlot(slot).get()` of `AbstractChestedHorse` (the chest, slot 499) and `AbstractHorse` (the inventory).
pub fn get_slot(e: &(impl AbstractHorse + ?Sized), slot: i32) -> Option<ItemStack> {
    if slot == CHEST_SLOT_OFFSET
        && let Some(has_chest) = e.chested_horse_has_chest()
    {
        return Some(if has_chest {
            ItemStack::new(Item::CHEST)
        } else {
            ItemStack::empty()
        });
    }
    match inventory_slot_index(e, slot) {
        Some(i) => Some(e.horse().inventory.get_item(i).clone()),
        None => crate::advancements::predicates::entity::support::get_slot(e.as_dyn(), slot),
    }
}

/// `getSlot(slot).set(stack)`: the chest slot only takes a chest or nothing.
pub fn can_set_slot(e: &(impl AbstractHorse + ?Sized), slot: i32, stack: &ItemStack) -> bool {
    if slot == CHEST_SLOT_OFFSET && e.chested_horse_has_chest().is_some() {
        return stack.is_empty() || stack.is(Item::CHEST);
    }
    crate::advancements::predicates::entity::support::can_set_equipment_slot(
        e.as_dyn(),
        slot,
        stack,
    )
}

/// `getSlot(slot).set(stack)`.
pub fn set_slot(
    e: &mut (impl AbstractHorse + ?Sized),
    level: &mut ServerLevel,
    slot: i32,
    stack: ItemStack,
) {
    if slot == CHEST_SLOT_OFFSET && e.chested_horse_has_chest().is_some() {
        e.chested_horse_set_chest_slot(&stack);
    } else if let Some(i) = inventory_slot_index(e, slot) {
        e.horse_mut().inventory.set_item(i, stack);
    } else {
        crate::advancements::predicates::entity::support::set_slot(
            level,
            e.as_dyn_mut(),
            slot,
            stack,
        );
    }
}

/// `Entity.getMaxSpawnClusterSize()`.
pub const MAX_SPAWN_CLUSTER_SIZE: i32 = 6;

/// `getSoundVolume()` / `getAmbientSoundInterval()`.
pub const SOUND_VOLUME: f32 = 0.8;
pub const AMBIENT_SOUND_INTERVAL: i32 = 400;

/// `player.isHolding(item)`.
pub fn is_holding(player: &ServerPlayer, item: Item) -> bool {
    player.get_item_in_hand(InteractionHand::MainHand).is(item)
        || player.get_item_in_hand(InteractionHand::OffHand).is(item)
}

/// `Entity.getCollisionHorizontalEscapeVector(vehicleWidth, passengerWidth, yRot)`.
fn get_collision_horizontal_escape_vector(
    vehicle_width: f32,
    passenger_width: f32,
    y_rot: f32,
) -> Vec3 {
    let span = (f64::from(vehicle_width) + f64::from(passenger_width) + f64::from(1.0E-5f32)) / 2.0;
    let rad = (std::f64::consts::PI / 180.0) as f32;
    let x_dir = -mth::sin(f64::from(y_rot * rad));
    let z_dir = mth::cos(f64::from(y_rot * rad));
    let max = x_dir.abs().max(z_dir.abs());
    Vec3::new(
        f64::from(x_dir) * span / f64::from(max),
        0.0,
        f64::from(z_dir) * span / f64::from(max),
    )
}

/// `LivingEntity.getMainArm() == HumanoidArm.RIGHT`.
fn main_arm_is_right(e: &dyn Entity) -> bool {
    match e.as_any().downcast_ref::<ServerPlayer>() {
        Some(p) => p.player.get_main_arm() == mc_protocol::packets::common::HumanoidArm::Right,
        None => !e.mob().is_some_and(|m| m.is_left_handed()),
    }
}

/// `LivingEntity.getDismountPoses()`.
fn get_dismount_poses(e: &dyn Entity) -> &'static [mc_data::enums::Pose] {
    use mc_data::enums::Pose;
    if e.is_player() {
        &[Pose::Standing, Pose::Crouching, Pose::Swimming]
    } else {
        &[Pose::Standing]
    }
}

/// `DismountHelper.canDismountTo(level, entity, box)`.
fn can_dismount_to(level: &ServerLevel, bb: &mc_core::phys::AABB) -> bool {
    use crate::world::level::collision::CollisionGetter as _;
    level
        .get_block_collisions(None, bb)
        .iter()
        .all(|shape| shape.is_empty())
        && level.world_border().is_within_bounds_aabb(bb)
}

/// `Entity.getDismountLocationForPassenger` of `AbstractHorse`.
pub fn get_dismount_location_for_passenger(
    e: &(impl AbstractHorse + ?Sized),
    level: &ServerLevel,
    passenger: &dyn Entity,
) -> Vec3 {
    let main_right = main_arm_is_right(passenger);
    let main_hand_direction = get_collision_horizontal_escape_vector(
        e.base().get_bb_width(),
        passenger.base().get_bb_width(),
        e.base().y_rot() + if main_right { 90.0 } else { -90.0 },
    );
    if let Some(location) =
        get_dismount_location_in_direction(e, level, main_hand_direction, passenger)
    {
        return location;
    }
    let off_hand_direction = get_collision_horizontal_escape_vector(
        e.base().get_bb_width(),
        passenger.base().get_bb_width(),
        e.base().y_rot() + if main_right { -90.0 } else { 90.0 },
    );
    get_dismount_location_in_direction(e, level, off_hand_direction, passenger)
        .unwrap_or_else(|| e.position())
}

/// `getDismountLocationInDirection(direction, passenger)` (the rider's pose is not changed here).
fn get_dismount_location_in_direction(
    e: &(impl AbstractHorse + ?Sized),
    level: &ServerLevel,
    direction: Vec3,
    passenger: &dyn Entity,
) -> Option<Vec3> {
    use crate::world::entity::dismount_helper;
    use crate::world::level::collision::BlockGetterExt as _;
    let target_x = e.base().x() + direction.x;
    let target_y = e.base().bounding_box().min_y;
    let target_z = e.base().z() + direction.z;
    for &dismount_pose in get_dismount_poses(passenger) {
        let mut target_block_pos = BlockPos::containing(target_x, target_y, target_z);
        let dismount_jump_limit = e.base().bounding_box().max_y + 0.75;
        loop {
            let block_floor_height = level.get_block_floor_height(target_block_pos);
            if f64::from(target_block_pos.y()) + block_floor_height > dismount_jump_limit {
                break;
            }
            if dismount_helper::is_block_floor_valid(block_floor_height) {
                let pose_collision_box = passenger
                    .get_dimensions(dismount_pose)
                    .make_bounding_box_vec(Vec3::ZERO);
                let location = Vec3::new(
                    target_x,
                    f64::from(target_block_pos.y()) + block_floor_height,
                    target_z,
                );
                if can_dismount_to(level, &pose_collision_box.move_vec(location)) {
                    return Some(location);
                }
            }
            target_block_pos = target_block_pos.above();
            if f64::from(target_block_pos.y()) >= dismount_jump_limit {
                break;
            }
        }
    }
    None
}
/// Implements the `Entity` members every `AbstractHorse` subclass shares, forwarding to this module (the members some
/// subclass overrides - `drop_equipment`, `ai_step`, `mob_interact`, `can_use_slot`, `is_food`, `is_immobile`, `cause_fall_damage`, `play_step_sound`,
/// `finalize_spawn`, `get_passenger_attachment_point`, the save-data pair, `get_breed_offspring`, `can_mate` - are written per class).
#[macro_export]
macro_rules! abstract_horse_entity {
    () => {
        fn get_slot(&self, slot: i32) -> Option<$crate::world::item::ItemStack> {
            $crate::world::entity::animal::equine::abstract_horse::get_slot(self, slot)
        }
        fn can_set_slot(&self, slot: i32, stack: &$crate::world::item::ItemStack) -> bool {
            $crate::world::entity::animal::equine::abstract_horse::can_set_slot(self, slot, stack)
        }
        fn set_slot(&mut self, level: &mut $crate::server::level::ServerLevel, slot: i32, stack: $crate::world::item::ItemStack) {
            $crate::world::entity::animal::equine::abstract_horse::set_slot(self, level, slot, stack);
        }
        fn can_dispenser_equip_into_slot(&self, slot: $crate::world::entity::equipment::EquipmentSlot) -> bool {
            use $crate::world::entity::equipment::EquipmentSlot;
            ((slot == EquipmentSlot::Body || slot == EquipmentSlot::Saddle)
                && $crate::world::entity::animal::equine::abstract_horse::is_tamed(self))
                || $crate::world::entity::mob::equipment::can_dispenser_equip_into_slot($crate::world::entity::Entity::as_dyn(self))
        }
        fn as_abstract_horse(&self) -> Option<&dyn $crate::world::entity::ai::goal::AbstractHorseAccess> {
            Some(self)
        }
        fn as_abstract_horse_mut(&mut self) -> Option<&mut dyn $crate::world::entity::ai::goal::AbstractHorseAccess> {
            Some(self)
        }
        fn tick(&mut self, level: &mut $crate::server::level::ServerLevel) {
            $crate::world::entity::animal::equine::abstract_horse::tick(self, level);
        }
        fn hurt_server(&mut self, level: &mut $crate::server::level::ServerLevel, source: &$crate::world::damagesource::DamageSource, damage: f32) -> bool {
            $crate::world::entity::animal::equine::abstract_horse::hurt_server(self, level, source, damage)
        }
        fn is_pushable(&self, _level: &$crate::server::level::ServerLevel) -> bool {
            !self.base().is_vehicle()
        }
        fn get_max_spawn_cluster_size(&self) -> i32 {
            $crate::world::entity::animal::equine::abstract_horse::MAX_SPAWN_CLUSTER_SIZE
        }
        fn get_sound_volume(&self) -> f32 {
            $crate::world::entity::animal::equine::abstract_horse::SOUND_VOLUME
        }
        fn get_ambient_sound_interval(&self) -> i32 {
            $crate::world::entity::animal::equine::abstract_horse::AMBIENT_SOUND_INTERVAL
        }
        fn on_climbable(&self, _level: &$crate::server::level::ServerLevel) -> bool {
            false
        }
        fn tick_ridden(&mut self, level: &mut $crate::server::level::ServerLevel, controller: $crate::world::entity::EntityId, ridden_input: ::mc_core::phys::Vec3) {
            $crate::world::entity::animal::equine::abstract_horse::tick_ridden(self, level, controller, ridden_input);
        }
        fn get_ridden_input(&self, level: &$crate::server::level::ServerLevel, controller: $crate::world::entity::EntityId, self_input: ::mc_core::phys::Vec3) -> ::mc_core::phys::Vec3 {
            $crate::world::entity::animal::equine::abstract_horse::get_ridden_input(self, level, controller, self_input)
        }
        fn get_ridden_speed(&self, _level: &$crate::server::level::ServerLevel, _controller: $crate::world::entity::EntityId) -> f32 {
            $crate::world::entity::living_entity::util::l(self).get_attribute_value(::mc_data::Attribute::MOVEMENT_SPEED) as f32
        }
        fn get_controlling_passenger(&self) -> Option<$crate::world::entity::EntityId> {
            $crate::world::entity::animal::equine::abstract_horse::get_controlling_passenger(self)
        }
        fn add_passenger(&mut self, passenger: &dyn $crate::world::entity::Entity, first_passenger_is_player: bool) {
            $crate::world::entity::animal::equine::abstract_horse::add_passenger(self, passenger, first_passenger_is_player);
        }
        fn after_add_passenger(&self, passenger: &mut dyn $crate::world::entity::Entity) {
            $crate::world::entity::animal::equine::abstract_horse::after_add_passenger(self, passenger);
        }
        fn on_passenger_removed(&mut self, level: &mut $crate::server::level::ServerLevel, _passenger: $crate::world::entity::EntityId) {
            $crate::world::entity::animal::equine::abstract_horse::on_passenger_removed(self, level);
        }
        fn position_rider_with(&self, passenger: &mut dyn $crate::world::entity::Entity, move_function: $crate::world::entity::entity::MoveFunction) {
            $crate::world::entity::animal::equine::abstract_horse::position_rider_with(self, passenger, move_function);
        }
        fn get_dismount_location_for_passenger(&self, level: &$crate::server::level::ServerLevel, passenger: &dyn $crate::world::entity::Entity) -> ::mc_core::phys::Vec3 {
            $crate::world::entity::animal::equine::abstract_horse::get_dismount_location_for_passenger(self, level, passenger)
        }
        fn get_dismount_location_for_passenger_setting_pose(&self, level: &$crate::server::level::ServerLevel, passenger: &mut dyn $crate::world::entity::Entity) -> ::mc_core::phys::Vec3 {
            $crate::world::entity::animal::equine::abstract_horse::get_dismount_location_for_passenger(self, level, &*passenger)
        }
        fn open_custom_inventory_screen(&mut self, level: &mut $crate::server::level::ServerLevel, player: &mut $crate::world::entity::ServerPlayer) {
            $crate::world::entity::animal::equine::abstract_horse::open_custom_inventory_screen(self, level, player);
        }
        fn as_container(&self) -> Option<&dyn $crate::world::container::Container> {
            Some(&self.horse().inventory)
        }
        fn as_container_mut(&mut self) -> Option<&mut dyn $crate::world::container::Container> {
            Some(&mut self.horse_mut().inventory)
        }
        fn as_mount_inventory(&self) -> Option<&dyn $crate::world::inventory::MountInventory> {
            Some(self)
        }
        fn as_mount_inventory_mut(&mut self) -> Option<&mut dyn $crate::world::inventory::MountInventory> {
            Some(self)
        }
        fn has_custom_inventory_screen(&self) -> bool {
            true
        }
        fn is_player_rideable_jumping(&self) -> bool {
            true
        }
        fn on_player_jump(&mut self, jump_amount: i32) {
            $crate::world::entity::animal::equine::abstract_horse::on_player_jump(self, jump_amount);
        }
        fn can_jump(&self) -> bool {
            $crate::world::entity::animal::equine::abstract_horse::is_saddled(self)
        }
        fn handle_start_jump(&mut self, level: &mut $crate::server::level::ServerLevel, _jump_scale: i32) {
            $crate::world::entity::animal::equine::abstract_horse::handle_start_jump(self, level);
        }
        fn handle_stop_jump(&mut self) {}
    };
}
