//! `net.minecraft.world.entity.monster.skeleton`: `AbstractSkeleton` and its subclasses `Skeleton`, `Stray`,
//! `WitherSkeleton`, `Bogged`, `Parched`.
//!
//! One Rust type ([`AbstractSkeleton`]) with a [`SkeletonKind`], like the zombie family.

mod goals;

use linkme::distributed_slice;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::Block;
use mc_data::enums::EntitySpawnReason;
use mc_data::{EntityType, Item, MobEffect, SoundEvent};
use mc_game_core::define_accessor;
use mc_game_core::register_spawn_placement;
use mc_game_core::registry::RegistryAccess;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::damagesource::DamageSource;
use mc_game_core::world::difficulty_instance::DifficultyInstance;
use mc_game_core::world::entity::ai::goal::Goal;
use mc_game_core::world::entity::ai::navigation::StandardPathNavigation;
use mc_game_core::world::entity::effect::MobEffectInstance;
use mc_game_core::world::entity::equipment::EquipmentSlot;
use mc_game_core::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use mc_game_core::world::entity::living_entity::util::lm;
use mc_game_core::world::entity::mob::conversion_tracker::{
    ConversionTracker, ConversionTrackerConfig,
};
use mc_game_core::world::entity::mob::{self, MobData, MobKind};
use mc_game_core::world::entity::spawn_group_data::SpawnGroupData;
use mc_game_core::world::entity::spawn_predicates as sp;
use mc_game_core::world::entity::{Entity, EntityId, ServerPlayer};
use mc_game_core::world::item::{InteractionHand, InteractionResult, ItemStack};
use mc_game_core::world::level::block::state::BlockStateExt;
use mc_game_core::world::level::pathfinder::PathType;
use mc_game_core::world::level::server_level_accessor::ServerLevelAccessor;
use mc_nbt::CompoundTag;
use mc_world::level_storage::Difficulty;

define_accessor!(pub DATA_STRAY_CONVERSION_ID: Boolean = Skeleton);
define_accessor!(pub DATA_SHEARED: Boolean = Bogged);

pub const HARD_ATTACK_INTERVAL: i32 = 20;
pub const NORMAL_ATTACK_INTERVAL: i32 = 40;
pub const INCREASED_HARD_ATTACK_INTERVAL: i32 = 50;
pub const INCREASED_NORMAL_ATTACK_INTERVAL: i32 = 70;

/// `Skeleton.freezingTracker`'s constants.
const FREEZING_TRACKER: ConversionTrackerConfig = ConversionTrackerConfig {
    conversion_id: DATA_STRAY_CONVERSION_ID,
    affliction_time_tag: "FreezingTime",
    total_affliction_time: 140,
    conversion_time_tag: "StrayConversionTime",
    total_conversion_time: 300,
};

/// The class of an [`AbstractSkeleton`].
pub enum SkeletonKind {
    /// `Skeleton` with its `freezingTracker`.
    Skeleton(ConversionTracker),
    Stray,
    WitherSkeleton,
    Bogged,
    Parched,
}

/// `AbstractSkeleton` (and, through [`SkeletonKind`], `Skeleton`, `Stray`, `WitherSkeleton`, `Bogged`, `Parched`).
pub struct AbstractSkeleton {
    pub mob: MobData,
    pub kind: SkeletonKind,
    /// `bowGoal` / `meleeGoal` while they are not registered with the goal selector (the objects are removed and re-added
    /// by `reassessWeaponGoal`, their state survives).
    parked_bow_goal: Option<Box<dyn Goal>>,
    parked_melee_goal: Option<Box<dyn Goal>>,
    /// `reassessWeaponGoal()` was requested outside a tick (NBT load): runs at the start of the next tick.
    weapon_goal_dirty: bool,
}

impl AbstractSkeleton {
    /// `new Skeleton(type, level)` & co.
    pub fn new(ty: EntityType, id: EntityId, _registries: &RegistryAccess) -> Self {
        let kind = match ty {
            EntityType::STRAY => SkeletonKind::Stray,
            EntityType::WITHER_SKELETON => SkeletonKind::WitherSkeleton,
            EntityType::BOGGED => SkeletonKind::Bogged,
            EntityType::PARCHED => SkeletonKind::Parched,
            _ => SkeletonKind::Skeleton(ConversionTracker::new(FREEZING_TRACKER)),
        };
        let mob = MobData::new(ty, id, MobKind::Monster, |b| match ty {
            EntityType::SKELETON => {
                b.define(DATA_STRAY_CONVERSION_ID, false);
            }
            EntityType::BOGGED => {
                b.define(DATA_SHEARED, false);
            }
            _ => {}
        });
        let mut skeleton = AbstractSkeleton {
            mob,
            kind,
            parked_bow_goal: None,
            parked_melee_goal: None,
            weapon_goal_dirty: false,
        };
        let navigation = StandardPathNavigation::ground(&skeleton);
        skeleton.mob.set_navigation(Box::new(navigation));
        goals::register_goals(&mut skeleton);
        goals::create_weapon_goals(&mut skeleton);
        // the `AbstractSkeleton` constructor body: `reassessWeaponGoal()` (no weapon yet: the melee goal is added)
        goals::reassess_weapon_goal(&mut skeleton, Difficulty::Normal);
        if let SkeletonKind::WitherSkeleton = skeleton.kind {
            skeleton.mob.set_pathfinding_malus(PathType::Lava, 8.0);
        }
        skeleton
    }

    /// `reassessWeaponGoal()`.
    pub fn reassess_weapon_goal(&mut self, difficulty: Difficulty) {
        goals::reassess_weapon_goal(self, difficulty);
    }

    /// `getHardAttackInterval()`.
    pub fn get_hard_attack_interval(&self) -> i32 {
        match self.kind {
            SkeletonKind::Bogged | SkeletonKind::Parched => INCREASED_HARD_ATTACK_INTERVAL,
            _ => HARD_ATTACK_INTERVAL,
        }
    }

    /// `getAttackInterval()`.
    pub fn get_attack_interval(&self) -> i32 {
        match self.kind {
            SkeletonKind::Bogged | SkeletonKind::Parched => INCREASED_NORMAL_ATTACK_INTERVAL,
            _ => NORMAL_ATTACK_INTERVAL,
        }
    }

    /// `isSheared()` (`Bogged`).
    pub fn is_sheared(&self) -> bool {
        self.mob.living.entity.entity_data.get(DATA_SHEARED)
    }

    /// `setSheared(sheared)` (`Bogged`).
    pub fn set_sheared(&mut self, sheared: bool) {
        self.mob
            .living
            .entity
            .entity_data
            .set(DATA_SHEARED, sheared);
    }

    /// `isShaking()`: `Skeleton` while its freezing conversion runs, else fully frozen.
    pub fn is_shaking(&self) -> bool {
        match &self.kind {
            SkeletonKind::Skeleton(tracker) => {
                tracker.is_converting(&self.mob.living.entity.entity_data)
            }
            _ => self.base().get_ticks_frozen() >= self.get_ticks_required_to_freeze(),
        }
    }

    /// `startFreezeConversion(time)` (`@VisibleForTesting`).
    pub fn start_freeze_conversion(&mut self, time: i32) {
        if let SkeletonKind::Skeleton(tracker) = &mut self.kind {
            tracker.start_conversion(&mut self.mob.living.entity.entity_data, time);
        }
    }

    fn get_step_sound(&self) -> SoundEvent {
        match self.kind {
            SkeletonKind::Skeleton(_) => SoundEvent::ENTITY_SKELETON_STEP,
            SkeletonKind::Stray => SoundEvent::ENTITY_STRAY_STEP,
            SkeletonKind::WitherSkeleton => SoundEvent::ENTITY_WITHER_SKELETON_STEP,
            SkeletonKind::Bogged => SoundEvent::ENTITY_BOGGED_STEP,
            SkeletonKind::Parched => SoundEvent::ENTITY_PARCHED_STEP,
        }
    }
}

impl Entity for AbstractSkeleton {
    mc_game_core::mob_entity_accessors!(mob);

    fn tick(&mut self, level: &mut ServerLevel) {
        if self.weapon_goal_dirty {
            self.weapon_goal_dirty = false;
            goals::reassess_weapon_goal_in_level(self, level);
        }
        if let SkeletonKind::Skeleton(_) = self.kind {
            self.tick_freezing_tracker(level);
        }
        mob::tick::tick(self, level);
    }

    fn can_freeze(&self) -> bool {
        !matches!(self.kind, SkeletonKind::Skeleton(_))
    }

    fn get_ambient_sound(&self) -> Option<SoundEvent> {
        Some(match self.kind {
            SkeletonKind::Skeleton(_) => SoundEvent::ENTITY_SKELETON_AMBIENT,
            SkeletonKind::Stray => SoundEvent::ENTITY_STRAY_AMBIENT,
            SkeletonKind::WitherSkeleton => SoundEvent::ENTITY_WITHER_SKELETON_AMBIENT,
            SkeletonKind::Bogged => SoundEvent::ENTITY_BOGGED_AMBIENT,
            SkeletonKind::Parched => SoundEvent::ENTITY_PARCHED_AMBIENT,
        })
    }

    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        Some(match self.kind {
            SkeletonKind::Skeleton(_) => SoundEvent::ENTITY_SKELETON_HURT,
            SkeletonKind::Stray => SoundEvent::ENTITY_STRAY_HURT,
            SkeletonKind::WitherSkeleton => SoundEvent::ENTITY_WITHER_SKELETON_HURT,
            SkeletonKind::Bogged => SoundEvent::ENTITY_BOGGED_HURT,
            SkeletonKind::Parched => SoundEvent::ENTITY_PARCHED_HURT,
        })
    }

    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(match self.kind {
            SkeletonKind::Skeleton(_) => SoundEvent::ENTITY_SKELETON_DEATH,
            SkeletonKind::Stray => SoundEvent::ENTITY_STRAY_DEATH,
            SkeletonKind::WitherSkeleton => SoundEvent::ENTITY_WITHER_SKELETON_DEATH,
            SkeletonKind::Bogged => SoundEvent::ENTITY_BOGGED_DEATH,
            SkeletonKind::Parched => SoundEvent::ENTITY_PARCHED_DEATH,
        })
    }

    fn play_step_sound(
        &mut self,
        level: &mut ServerLevel,
        _pos: BlockPos,
        _state: mc_data::block::BlockState,
    ) {
        let sound = self.get_step_sound();
        self.play_sound(level, sound, 0.15, 1.0);
    }

    fn after_ride_tick(&mut self, level: &ServerLevel) {
        if let Some(vehicle) =
            mob::navigation::controlled_vehicle(self, level).and_then(|v| level.get_entity(v))
            && vehicle.mob().is_some_and(|m| m.kind.is_pathfinder())
        {
            let body = vehicle.living().map(|l| l.y_body_rot);
            if let (Some(body), Some(l)) = (body, self.living_mut()) {
                l.y_body_rot = body;
            }
        }
    }

    fn populate_default_equipment_slots(
        &mut self,
        random: &mut dyn RandomSource,
        difficulty: &DifficultyInstance,
    ) {
        match self.kind {
            SkeletonKind::WitherSkeleton => mob::equipment::set_item_slot_spawn(
                self,
                EquipmentSlot::MainHand,
                ItemStack::new(Item::STONE_SWORD),
            ),
            _ => {
                mob::equipment::populate_default_equipment_slots(self, random, difficulty);
                mob::equipment::set_item_slot_spawn(
                    self,
                    EquipmentSlot::MainHand,
                    ItemStack::new(Item::BOW),
                );
            }
        }
    }

    fn populate_default_equipment_enchantments(
        &mut self,
        registries: &RegistryAccess,
        random: &mut dyn RandomSource,
        difficulty: &DifficultyInstance,
    ) {
        if !matches!(self.kind, SkeletonKind::WitherSkeleton) {
            mob::equipment::populate_default_equipment_enchantments(
                self, registries, random, difficulty,
            );
        }
    }

    fn finalize_spawn(
        &mut self,
        level: &mut dyn ServerLevelAccessor,
        difficulty: DifficultyInstance,
        reason: EntitySpawnReason,
        group_data: Option<Box<dyn SpawnGroupData>>,
    ) -> Option<Box<dyn SpawnGroupData>> {
        let group_data = mob::spawn::finalize_spawn(self, level, difficulty, reason, group_data);
        // `AbstractSkeleton.finalizeSpawn`: `random` is the level's
        self.populate_default_equipment_slots(level.random(), &difficulty);
        let registries = level.registry_access().clone();
        self.populate_default_equipment_enchantments(&registries, level.random(), &difficulty);
        self.reassess_weapon_goal(difficulty.get_difficulty());
        let can_pick_up = level.random().next_float() < 0.55 * difficulty.get_special_multiplier();
        self.set_can_pick_up_loot(can_pick_up);
        if self.get_item_by_slot(EquipmentSlot::Head).is_empty()
            && mc_game_core::util::special_dates::is_halloween()
            && level.random().next_float() < 0.25
        {
            let pumpkin = if level.random().next_float() < 0.1 {
                Item::JACK_O_LANTERN
            } else {
                Item::CARVED_PUMPKIN
            };
            mob::equipment::set_item_slot_spawn(self, EquipmentSlot::Head, ItemStack::new(pumpkin));
            self.set_drop_chance(EquipmentSlot::Head, 0.0);
        }
        if let SkeletonKind::WitherSkeleton = self.kind {
            // `WitherSkeleton.finalizeSpawn`: after `super`
            if let Some(a) = lm(self)
                .attributes
                .get_instance(mc_data::Attribute::ATTACK_DAMAGE)
            {
                a.set_base_value(4.0);
            }
            self.reassess_weapon_goal(difficulty.get_difficulty());
        }
        group_data
    }

    fn can_use_non_melee_weapon(&self, stack: &ItemStack) -> bool {
        stack.item() == Item::BOW
    }

    fn get_preferred_weapon_type(&self) -> Option<&'static str> {
        match self.kind {
            SkeletonKind::WitherSkeleton => None,
            _ => Some("skeleton_preferred_weapons"),
        }
    }

    fn can_hold_item(&self, stack: &ItemStack) -> bool {
        match self.kind {
            SkeletonKind::WitherSkeleton => {
                !goals::item_is_in_tag(stack, "wither_skeleton_disliked_weapons")
            }
            _ => true,
        }
    }

    fn wants_to_pick_up(&self, _level: &ServerLevel, stack: &ItemStack) -> bool {
        if goals::item_is_in_tag(stack, "spears") {
            false
        } else {
            self.can_hold_item(stack)
        }
    }

    fn can_be_affected(&self, level: &ServerLevel, effect: &MobEffectInstance) -> bool {
        match self.kind {
            SkeletonKind::WitherSkeleton if effect.effect == MobEffect::WITHER => false,
            SkeletonKind::Parched if effect.effect == MobEffect::WEAKNESS => false,
            _ => mc_game_core::world::entity::living_entity::effects::can_be_affected(
                self, level, effect,
            ),
        }
    }

    fn do_hurt_target(&mut self, level: &mut ServerLevel, target: &mut dyn Entity) -> bool {
        let result = mc_game_core::world::entity::living_entity::combat::mob_do_hurt_target(
            self, level, target,
        );
        if let SkeletonKind::WitherSkeleton = self.kind {
            if !result {
                return false;
            }
            if target.living().is_some() {
                let source = self.id();
                target.add_effect(
                    level,
                    MobEffectInstance::of(MobEffect::WITHER, 200, 0),
                    Some(source),
                );
            }
            return true;
        }
        result
    }

    fn on_equip_item(
        &mut self,
        level: &mut ServerLevel,
        slot: EquipmentSlot,
        old: &ItemStack,
        new: &ItemStack,
    ) {
        mc_game_core::world::entity::living_entity::equipment::on_equip_item(
            self, level, slot, old, new,
        );
        goals::reassess_weapon_goal_in_level(self, level);
    }

    fn is_shearable(&self) -> bool {
        matches!(self.kind, SkeletonKind::Bogged)
    }

    fn ready_for_shearing(&self) -> bool {
        matches!(self.kind, SkeletonKind::Bogged) && !self.is_sheared()
    }

    fn shear(
        &mut self,
        level: &mut ServerLevel,
        sound_source: mc_protocol::packets::play::SoundSource,
        tool: &ItemStack,
    ) {
        goals::shear(self, level, sound_source, tool);
    }

    fn mob_interact(
        &mut self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        hand: InteractionHand,
    ) -> InteractionResult {
        if let SkeletonKind::Bogged = self.kind {
            let in_hand = player.get_item_in_hand(hand);
            if in_hand.is(Item::SHEARS) && !self.is_sheared() {
                goals::shear_bogged(self, level, player, hand);
                return InteractionResult::SUCCESS;
            }
        }
        mob::interact::mob_interact(self, level, player, hand)
    }

    fn perform_ranged_attack(&mut self, level: &mut ServerLevel, target: EntityId, power: f32) {
        goals::perform_ranged_attack(self, level, target, power);
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        mob::nbt::add_additional_save_data(self, out);
        match &self.kind {
            SkeletonKind::Skeleton(tracker) => {
                let afflicted = self.base().is_in_powder_snow;
                tracker.add_additional_save_data(
                    &self.mob.living.entity.entity_data,
                    afflicted,
                    out,
                );
            }
            SkeletonKind::Bogged => out.put_boolean("sheared", self.is_sheared()),
            _ => {}
        }
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        mob::nbt::read_additional_save_data(self, input);
        self.weapon_goal_dirty = true;
        match &mut self.kind {
            SkeletonKind::Skeleton(tracker) => {
                tracker.read_additional_save_data(&mut self.mob.living.entity.entity_data, input)
            }
            SkeletonKind::Bogged => {
                let sheared = input.get_boolean_or("sheared", false);
                self.set_sheared(sheared);
            }
            _ => {}
        }
    }
}

impl AbstractSkeleton {
    /// `freezingTracker.tick()` (`Skeleton.tick`, before `super.tick()`).
    fn tick_freezing_tracker(&mut self, level: &mut ServerLevel) {
        let ready = self.is_alive() && !self.mob.is_no_ai();
        let afflicted = ready && self.base().is_in_powder_snow;
        let convert = match &mut self.kind {
            SkeletonKind::Skeleton(tracker) => {
                tracker.tick(&mut self.mob.living.entity.entity_data, ready, afflicted)
            }
            _ => false,
        };
        if convert {
            let params = mob::ConversionParams::single(self, level, true, true);
            mob::convert::convert_to(
                self,
                level,
                EntityType::STRAY,
                &params,
                EntitySpawnReason::Conversion,
                |converted, level| {
                    if !converted.base().is_silent() {
                        level.level_event_impl(None, 1048, converted.block_position(), 0);
                    }
                },
            );
        }
    }
}

/// `Stray.checkStraySpawnRules(type, level, spawnReason, pos, random)`.
pub fn check_stray_spawn_rules(
    ty: EntityType,
    level: &dyn ServerLevelAccessor,
    reason: EntitySpawnReason,
    pos: BlockPos,
    random: &mut dyn RandomSource,
) -> bool {
    let mut check_sky_pos = pos;
    loop {
        check_sky_pos = check_sky_pos.above();
        if !level
            .get_block_state(check_sky_pos)
            .is_block(Block::POWDER_SNOW)
        {
            break;
        }
    }
    sp::check_monster_spawn_rules(ty, level, reason, pos, random)
        && (sp::is_spawner(reason) || level.can_see_sky(check_sky_pos.below()))
}

register_spawn_placement!(EntityType::STRAY, check_stray_spawn_rules);

macro_rules! register_skeleton {
    ($name:ident, $ty:ident) => {
        #[distributed_slice(ENTITY_FACTORIES)]
        static $name: EntityRegistration = EntityRegistration {
            entity_type: EntityType::$ty,
            create: |ty, id, registries| Box::new(AbstractSkeleton::new(ty, id, registries)),
        };
    };
}

register_skeleton!(SKELETON, SKELETON);
register_skeleton!(STRAY, STRAY);
register_skeleton!(WITHER_SKELETON, WITHER_SKELETON);
register_skeleton!(BOGGED, BOGGED);
register_skeleton!(PARCHED, PARCHED);
