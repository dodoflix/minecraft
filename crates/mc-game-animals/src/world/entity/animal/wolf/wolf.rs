//! `Wolf`: tameable, neutral pack mob with collar, armor, wet shaking and variants.

use super::wolf_sound_variant::{WolfSoundSet, WolfSoundVariant};
use super::wolf_variant::WolfVariant;
use crate::world::entity::ai::goal::BegGoal;
use crate::world::entity::animal::turtle;
use linkme::distributed_slice;
use mc_core::ident::Identifier;
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::enums::{EntitySpawnReason, Pose};
use mc_data::{EntityType, GameEvent, Item, SoundEvent};
use mc_game_ai::world::entity::ai::goal::target::{
    NonTameRandomTargetGoal, OwnerHurtByTargetGoal, OwnerHurtTargetGoal,
};
use mc_game_ai::world::entity::ai::goal::{AvoidEntityGoal, LeapAtTargetGoal};
use mc_game_core::define_accessor;
use mc_game_core::registry::RegistryAccess;
use mc_game_core::registry::keys;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::damagesource::{DamageSource, DamageTypeTags};
use mc_game_core::world::difficulty_instance::DifficultyInstance;
use mc_game_core::world::entity::ai::goal::ResetUniversalAngerTargetGoal;
use mc_game_core::world::entity::ai::goal::target::{
    HurtByTargetGoal, NearestAttackableTargetGoal, TargetType, class_of_types,
};
use mc_game_core::world::entity::ai::goal::{
    BreedGoal, FloatGoal, FollowOwnerGoal, Goal, GoalFlags, LookAtPlayerGoal, MeleeAttackGoal,
    RandomLookAroundGoal, SitWhenOrderedToGoal, TamableAnimalPanicGoal,
    WaterAvoidingRandomStrollGoal,
};
use mc_game_core::world::entity::ai::navigation::StandardPathNavigation;
use mc_game_core::world::entity::crackiness::Crackiness;
use mc_game_core::world::entity::dimensions::{EntityAttachments, EntityDimensions};
use mc_game_core::world::entity::equipment::EquipmentSlot;
use mc_game_core::world::entity::factory::{
    ENTITY_FACTORIES, EntityRegistration, EntitySpawnRequest,
};
use mc_game_core::world::entity::living_entity::util::broadcast_entity_event;
use mc_game_core::world::entity::mob::neutral::{self, NeutralData};
use mc_game_core::world::entity::mob::spawn::AgeableMobGroupData;
use mc_game_core::world::entity::mob::{self, MobData, MobKind, interact, tamable};
use mc_game_core::world::entity::scenario_dump::{SCENARIO_DUMP_EXTRA, ScenarioDumpExtra};
use mc_game_core::world::entity::spawn_group_data::SpawnGroupData;
use mc_game_core::world::entity::variant::{self, SpawnContext};
use mc_game_core::world::entity::{Entity, EntityId, ServerPlayer};
use mc_game_core::world::item::dye_color;
use mc_game_core::world::item::{InteractionHand, InteractionResult, ItemStack, ItemStackExt};
use mc_game_core::world::level::pathfinder::PathType;
use mc_game_core::world::level::server_level_accessor::ServerLevelAccessor;
use mc_nbt::CompoundTag;
use mc_protocol::components::{
    DYE, DataComponentMap, DyeColor, Holder, WOLF_COLLAR, WOLF_SOUND_VARIANT, WOLF_VARIANT,
};
use std::sync::Arc;

define_accessor!(pub DATA_INTERESTED_ID: Boolean = Wolf);
define_accessor!(pub DATA_COLLAR_COLOR: Int = Wolf);
define_accessor!(pub DATA_ANGER_END_TIME: Long = Wolf);
define_accessor!(pub DATA_VARIANT_ID: WolfVariant = Wolf);
define_accessor!(pub DATA_SOUND_VARIANT_ID: WolfSoundVariant = Wolf);

pub const START_HEALTH: f32 = 8.0;
pub const TAME_HEALTH: f32 = 40.0;
pub const ARMOR_REPAIR_UNIT: f32 = 0.125;
pub const DEFAULT_TAIL_ANGLE: f32 = (std::f64::consts::PI / 5.0) as f32;
const DEFAULT_COLLAR_COLOR: DyeColor = DyeColor::Red;

/// `PREY_SELECTOR`: sheep, rabbits and foxes.
pub fn prey_selector(target: &dyn Entity, _level: &ServerLevel) -> bool {
    matches!(
        target.get_type(),
        EntityType::SHEEP | EntityType::RABBIT | EntityType::FOX
    )
}

/// `BABY_DIMENSIONS`.
fn baby_dimensions() -> EntityDimensions {
    let dims = EntityDimensions::scalable(0.3, 0.425).with_eye_height(0.34375);
    let attachments =
        EntityAttachments::create_default(0.3, 0.425).with_passenger(Vec3::new(0.0, 0.4375, 0.0));
    dims.with_attachments(attachments)
}

/// `Wolf.WolfPackData`.
pub struct WolfPackData {
    base: AgeableMobGroupData,
    pub variant: Identifier,
}

impl WolfPackData {
    pub fn new(variant: Identifier) -> Self {
        Self {
            base: AgeableMobGroupData::with_baby(false),
            variant,
        }
    }
}

impl SpawnGroupData for WolfPackData {
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

/// `Wolf.WolfAvoidEntityGoal<Llama>`.
pub struct WolfAvoidEntityGoal {
    avoid: AvoidEntityGoal,
}

impl Default for WolfAvoidEntityGoal {
    fn default() -> Self {
        Self::new()
    }
}

impl WolfAvoidEntityGoal {
    pub fn new() -> Self {
        Self {
            avoid: AvoidEntityGoal::new(
                class_of_types(&[EntityType::LLAMA, EntityType::TRADER_LLAMA]),
                24.0,
                1.5,
                1.5,
            ),
        }
    }
}

impl Goal for WolfAvoidEntityGoal {
    fn name(&self) -> &'static str {
        "Wolf$WolfAvoidEntityGoal"
    }
    fn can_use(&mut self, wolf: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        if !(self.avoid.can_use_impl(wolf, level)
            && self
                .avoid
                .to_avoid
                .and_then(|a| level.get_entity(a))
                .is_some_and(|a| {
                    a.as_any()
                        .is::<crate::world::entity::animal::equine::Llama>()
                }))
        {
            return false;
        }
        let strength = self
            .avoid
            .to_avoid
            .and_then(|a| level.get_entity(a))
            .and_then(|a| {
                a.as_any()
                    .downcast_ref::<crate::world::entity::animal::equine::Llama>()
            })
            .map_or(0, |l| l.get_strength());
        !tamable::is_tame(wolf) && strength >= wolf.base_mut().random.next_int_bounded(5)
    }
    fn can_continue_to_use(&mut self, wolf: &mut dyn Entity, _level: &mut ServerLevel) -> bool {
        self.avoid.can_continue_to_use_impl(wolf)
    }
    fn start(&mut self, wolf: &mut dyn Entity, level: &mut ServerLevel) {
        wolf.set_target(level, None);
        self.avoid.start_impl(wolf, level);
    }
    fn stop(&mut self, _wolf: &mut dyn Entity, _level: &mut ServerLevel) {
        self.avoid.stop_impl();
    }
    fn tick(&mut self, wolf: &mut dyn Entity, level: &mut ServerLevel) {
        wolf.set_target(level, None);
        self.avoid.tick_impl(wolf, level);
    }
    fn flags(&self) -> GoalFlags {
        self.avoid.flags()
    }
}

/// `Wolf`.
pub struct Wolf {
    pub mob: MobData,
    interested_angle: f32,
    interested_angle_o: f32,
    is_wet: bool,
    is_shaking: bool,
    shake_anim: f32,
    shake_anim_o: f32,
    /// Identifier of the variant / sound variant entry (what `VariantUtils.writeVariant` saves) and the sound set cached
    /// when the sound variant was set (no registry access while sounds play).
    variant_key: Identifier,
    sound_variant_key: Identifier,
    sound_variant: WolfSoundVariant,
    /// `variant` / `sound_variant` read from NBT, resolved to registry ids by [`Entity::resolve_registry_refs`].
    pending_variant: Option<Identifier>,
    pending_sound_variant: Option<Identifier>,
    /// Registries the variant components of a spawn egg resolve against.
    registries: RegistryAccess,
}

impl Wolf {
    /// `new Wolf(type, level)` (`registerGoals` and `createNavigation` run as in `Mob.<init>`).
    pub fn new(ty: EntityType, id: EntityId, registries: &RegistryAccess) -> Self {
        let default_variant = variant::get_default_or_any(
            registries,
            keys::WOLF_VARIANT,
            &Identifier::parse("minecraft:pale").expect("valid id"),
        );
        let sound_registry = registries.registry_or_throw(keys::WOLF_SOUND_VARIANT);
        let default_sound = sound_registry
            .id_of(&Identifier::parse("minecraft:classic").expect("valid id"))
            .unwrap_or(0);
        let mob = MobData::new(ty, id, MobKind::TamableAnimal, |b| {
            b.define(DATA_VARIANT_ID, default_variant as i32);
            b.define(DATA_SOUND_VARIANT_ID, default_sound as i32);
            b.define(DATA_INTERESTED_ID, false);
            b.define(DATA_COLLAR_COLOR, DEFAULT_COLLAR_COLOR.id());
            b.define(DATA_ANGER_END_TIME, -1i64);
        });
        let variant_key = registries
            .registry_or_throw(keys::WOLF_VARIANT)
            .name_of(default_variant)
            .expect("variant entry");
        let sound_variant_key = sound_registry
            .name_of(default_sound)
            .expect("sound variant entry");
        let sound_variant = *registries
            .get::<WolfSoundVariant>(keys::WOLF_SOUND_VARIANT, &sound_variant_key)
            .expect("typed wolf sound variant");
        let mut wolf = Wolf {
            mob,
            interested_angle: 0.0,
            interested_angle_o: 0.0,
            is_wet: false,
            is_shaking: false,
            shake_anim: 0.0,
            shake_anim_o: 0.0,
            variant_key,
            sound_variant_key,
            registries: registries.clone(),
            sound_variant,
            pending_variant: None,
            pending_sound_variant: None,
        };
        wolf.mob.neutral = Some(NeutralData {
            end_time_accessor: Some(DATA_ANGER_END_TIME),
            ..NeutralData::default()
        });
        let navigation = StandardPathNavigation::ground(&wolf);
        wolf.mob.set_navigation(Box::new(navigation));
        wolf.register_goals();
        tamable::set_tame(&mut wolf, false, false);
        wolf.mob.set_pathfinding_malus(PathType::PowderSnow, -1.0);
        wolf.mob
            .set_pathfinding_malus(PathType::OnTopOfPowderSnow, -1.0);
        wolf
    }

    /// `registerGoals()`.
    fn register_goals(&mut self) {
        let float = FloatGoal::new(self);
        let breed = BreedGoal::new(self, 1.0);
        let look_at = LookAtPlayerGoal::new_player(self, 8.0);
        let goals = &mut self.mob.goal_selector;
        goals.add_goal(1, Box::new(float));
        goals.add_goal(
            1,
            Box::new(TamableAnimalPanicGoal::with_tag(
                1.5,
                DamageTypeTags::PANIC_ENVIRONMENTAL_CAUSES,
            )),
        );
        goals.add_goal(2, Box::new(SitWhenOrderedToGoal::new()));
        goals.add_goal(3, Box::new(WolfAvoidEntityGoal::new()));
        goals.add_goal(4, Box::new(LeapAtTargetGoal::new(0.4)));
        goals.add_goal(5, Box::new(MeleeAttackGoal::new(1.0, true)));
        goals.add_goal(6, Box::new(FollowOwnerGoal::new(1.0, 10.0, 2.0)));
        goals.add_goal(7, Box::new(breed));
        goals.add_goal(8, Box::new(WaterAvoidingRandomStrollGoal::new(1.0)));
        goals.add_goal(9, Box::new(BegGoal::new(8.0)));
        goals.add_goal(10, Box::new(look_at));
        goals.add_goal(10, Box::new(RandomLookAroundGoal::new()));
        let targets = &mut self.mob.target_selector;
        targets.add_goal(1, Box::new(OwnerHurtByTargetGoal::new()));
        targets.add_goal(2, Box::new(OwnerHurtTargetGoal::new()));
        targets.add_goal(
            3,
            Box::new(HurtByTargetGoal::new(vec![]).set_alert_others(vec![])),
        );
        targets.add_goal(
            4,
            Box::new(
                NearestAttackableTargetGoal::build(TargetType::Player, 10, true, false, None)
                    .with_source_selector(Arc::new(|mob, target, level| {
                        neutral::is_angry_at(mob, level, target)
                    })),
            ),
        );
        targets.add_goal(
            5,
            Box::new(NonTameRandomTargetGoal::new(
                TargetType::Class(Arc::new(|e| e.mob().is_some_and(|m| m.animal.is_some()))),
                false,
                Some(Arc::new(prey_selector)),
            )),
        );
        targets.add_goal(
            6,
            Box::new(NonTameRandomTargetGoal::new(
                TargetType::of_type(EntityType::TURTLE),
                false,
                Some(Arc::new(turtle::baby_on_land_selector)),
            )),
        );
        targets.add_goal(
            7,
            Box::new(NearestAttackableTargetGoal::new(
                TargetType::Class(class_of_types(&[
                    EntityType::SKELETON,
                    EntityType::STRAY,
                    EntityType::WITHER_SKELETON,
                    EntityType::BOGGED,
                    EntityType::PARCHED,
                ])),
                false,
            )),
        );
        targets.add_goal(8, Box::new(ResetUniversalAngerTargetGoal::new(true)));
    }

    fn get_sound_set(&self) -> &WolfSoundSet {
        if self.is_baby() {
            &self.sound_variant.baby_sounds
        } else {
            &self.sound_variant.adult_sounds
        }
    }

    /// `setVariant(variant)`.
    pub fn set_variant(&mut self, registries: &RegistryAccess, id: u32) {
        self.mob
            .living
            .entity
            .entity_data
            .set(DATA_VARIANT_ID, id as i32);
        if let Some(name) = registries
            .registry(keys::WOLF_VARIANT)
            .and_then(|r| r.name_of(id))
        {
            self.variant_key = name;
        }
    }

    /// `getVariant()` (registry id).
    pub fn get_variant(&self) -> i32 {
        self.mob.living.entity.entity_data.get(DATA_VARIANT_ID)
    }

    /// `setSoundVariant(soundVariant)`.
    pub fn set_sound_variant(&mut self, registries: &RegistryAccess, id: u32) {
        self.mob
            .living
            .entity
            .entity_data
            .set(DATA_SOUND_VARIANT_ID, id as i32);
        if let Some(name) = registries
            .registry(keys::WOLF_SOUND_VARIANT)
            .and_then(|r| r.name_of(id))
        {
            if let Some(v) = registries.get::<WolfSoundVariant>(keys::WOLF_SOUND_VARIANT, &name) {
                self.sound_variant = *v;
            }
            self.sound_variant_key = name;
        }
    }

    /// `getCollarColor()`.
    pub fn get_collar_color(&self) -> DyeColor {
        dye_color::by_id(self.mob.living.entity.entity_data.get(DATA_COLLAR_COLOR))
    }

    /// `setCollarColor(color)`.
    pub fn set_collar_color(&mut self, color: DyeColor) {
        self.mob
            .living
            .entity
            .entity_data
            .set(DATA_COLLAR_COLOR, color.id());
    }

    /// `setIsInterested(value)`.
    pub fn set_is_interested(&mut self, value: bool) {
        self.mob
            .living
            .entity
            .entity_data
            .set(DATA_INTERESTED_ID, value);
    }

    /// `isInterested()`.
    pub fn is_interested(&self) -> bool {
        self.mob.living.entity.entity_data.get(DATA_INTERESTED_ID)
    }

    pub fn is_wet(&self) -> bool {
        self.is_wet
    }

    pub fn is_shaking(&self) -> bool {
        self.is_shaking
    }

    /// `getTailAngle()`.
    pub fn get_tail_angle(&self, level: &ServerLevel) -> f32 {
        if neutral::is_angry(self, level) {
            1.5393804
        } else if tamable::is_tame(self) {
            let max_health = self.mob.living.get_max_health();
            let damage_ratio = (max_health - self.mob.living.get_health()) / max_health;
            (0.55 - damage_ratio * 0.4) * std::f64::consts::PI as f32
        } else {
            DEFAULT_TAIL_ANGLE
        }
    }

    fn cancel_shake(&mut self) {
        self.is_shaking = false;
        self.shake_anim = 0.0;
        self.shake_anim_o = 0.0;
    }

    fn body_armor(&self) -> ItemStack {
        self.get_item_by_slot(EquipmentSlot::Body)
    }

    /// `canArmorAbsorb(source)`.
    fn can_armor_absorb(&self, level: &ServerLevel, source: &DamageSource) -> bool {
        self.body_armor().is(Item::WOLF_ARMOR)
            && !source.is_tag(level, DamageTypeTags::BYPASSES_WOLF_ARMOR)
    }

    /// `tryToTame(player)`.
    fn try_to_tame(&mut self, level: &mut ServerLevel, player: &mut ServerPlayer) {
        if self.base_mut().random.next_int_bounded(3) == 0 {
            tamable::tame(self, level, player);
            mc_game_core::world::entity::mob::navigation::stop(self, level);
            mob::tick::set_target(self, level, None);
            tamable::set_ordered_to_sit(self, true);
            broadcast_entity_event(level, self.id(), 7);
        } else {
            broadcast_entity_event(level, self.id(), 6);
        }
    }

    /// `isOwnedBy(player)` for the interacting player (in the level, so the owner reference identifies it).
    fn is_owned_by_player(&self, player: &ServerPlayer) -> bool {
        tamable::get_owner_reference(self) == Some(player.uuid())
    }
}

fn is_wolf_food(stack: &ItemStack) -> bool {
    mc_data::tags::get::<Item>("wolf_food").is_some_and(|t| t.contains(stack.item()))
}

impl Entity for Wolf {
    mc_game_core::mob_entity_accessors!(mob);

    fn get_encode_id(&self) -> Option<&'static str> {
        Some("minecraft:wolf")
    }

    fn get_dimensions(&self, _pose: Pose) -> EntityDimensions {
        if self.is_baby() {
            baby_dimensions()
        } else {
            EntityDimensions::of_type(self.mob.living.entity.entity_type)
        }
    }

    fn play_step_sound(
        &mut self,
        level: &mut ServerLevel,
        _pos: BlockPos,
        _state: mc_data::block::BlockState,
    ) {
        let sound = self.get_sound_set().step_sound;
        self.play_sound(level, sound, 0.15, 1.0);
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        mob::nbt::add_additional_save_data(self, out);
        out.put_byte("CollarColor", self.get_collar_color().id() as i8);
        variant::write_variant(out, &self.variant_key);
        neutral::add_persistent_anger_save_data(self, out);
        out.put_string("sound_variant", self.sound_variant_key.to_string());
    }

    fn read_additional_save_data(&mut self, level: Option<&ServerLevel>, input: &CompoundTag) {
        mob::nbt::read_additional_save_data(self, level, input);
        self.pending_variant = variant::read_variant_key(input);
        let collar = if input.contains("CollarColor") {
            dye_color::by_id(i32::from(input.get_byte_or("CollarColor", 0)))
        } else {
            DEFAULT_COLLAR_COLOR
        };
        self.set_collar_color(collar);
        neutral::read_persistent_anger_save_data(self, level, input);
        self.pending_sound_variant = input
            .get_string("sound_variant")
            .and_then(Identifier::try_parse);
    }

    /// `Wolf.get(type)`: `WOLF_VARIANT`, `WOLF_SOUND_VARIANT` and `WOLF_COLLAR`.
    fn entity_components(&self) -> DataComponentMap {
        let base = mc_game_core::advancements::predicates::entity::support::base_entity_components(
            self.base(),
        );
        DataComponentMap::builder()
            .add_all(&base)
            .set(WOLF_VARIANT, Holder::Reference(self.variant_key.clone()))
            .set(
                WOLF_SOUND_VARIANT,
                Holder::Reference(self.sound_variant_key.clone()),
            )
            .set(WOLF_COLLAR, self.get_collar_color())
            .build()
    }

    fn apply_implicit_components(&mut self, stack: &ItemStack) {
        let registries = self.registries.clone();
        if let Some(Holder::Reference(key)) = stack.get(&WOLF_VARIANT)
            && let Some(id) = registries
                .registry(keys::WOLF_VARIANT)
                .and_then(|r| r.id_of(key))
        {
            self.set_variant(&registries, id);
        }
        if let Some(Holder::Reference(key)) = stack.get(&WOLF_SOUND_VARIANT)
            && let Some(id) = registries
                .registry(keys::WOLF_SOUND_VARIANT)
                .and_then(|r| r.id_of(key))
        {
            self.set_sound_variant(&registries, id);
        }
        if let Some(color) = stack.get(&WOLF_COLLAR).copied() {
            self.set_collar_color(color);
        }
        mc_game_core::world::entity::spawn_config::apply_base_implicit_components(self, stack);
    }

    fn resolve_registry_refs(&mut self, registries: &RegistryAccess) {
        if let Some(key) = self.pending_variant.take()
            && let Some(id) = registries
                .registry(keys::WOLF_VARIANT)
                .and_then(|r| r.id_of(&key))
        {
            self.set_variant(registries, id);
        }
        if let Some(key) = self.pending_sound_variant.take()
            && let Some(id) = registries
                .registry(keys::WOLF_SOUND_VARIANT)
                .and_then(|r| r.id_of(&key))
        {
            self.set_sound_variant(registries, id);
        }
    }

    fn finalize_spawn(
        &mut self,
        level: &mut dyn ServerLevelAccessor,
        difficulty: DifficultyInstance,
        reason: EntitySpawnReason,
        group_data: Option<Box<dyn SpawnGroupData>>,
    ) -> Option<Box<dyn SpawnGroupData>> {
        let registries = level.registry_access().clone();
        let mut group_data = group_data;
        let pack_variant = group_data
            .as_mut()
            .and_then(|g| g.as_any_mut().downcast_mut::<WolfPackData>())
            .map(|p| p.variant.clone());
        if let Some(variant_key) = pack_variant {
            if let Some(id) = registries
                .registry(keys::WOLF_VARIANT)
                .and_then(|r| r.id_of(&variant_key))
            {
                self.set_variant(&registries, id);
            }
        } else {
            let pos = self.block_position();
            let mut selected: Option<Identifier> = None;
            level.with_random_taken(&mut |level, random| {
                let context = SpawnContext::create(level, pos);
                selected = variant::select_variant_to_spawn::<WolfVariant>(
                    &context,
                    keys::WOLF_VARIANT,
                    random,
                );
                true
            });
            if let Some(key) = selected {
                if let Some(id) = registries
                    .registry(keys::WOLF_VARIANT)
                    .and_then(|r| r.id_of(&key))
                {
                    self.set_variant(&registries, id);
                }
                group_data = Some(Box::new(WolfPackData::new(key)));
            }
        }
        if let Some(key) =
            variant::pick_random_entry(&registries, keys::WOLF_SOUND_VARIANT, level.random())
            && let Some(id) = registries
                .registry(keys::WOLF_SOUND_VARIANT)
                .and_then(|r| r.id_of(&key))
        {
            self.set_sound_variant(&registries, id);
        }
        mob::spawn::finalize_spawn(self, level, difficulty, reason, group_data)
    }

    fn get_ambient_sound_in(&mut self, level: &mut ServerLevel) -> Option<SoundEvent> {
        if neutral::is_angry(self, level) {
            Some(self.get_sound_set().growl_sound)
        } else if self.base_mut().random.next_int_bounded(3) == 0 {
            Some(if tamable::is_tame(self) && self.get_health() < 20.0 {
                self.get_sound_set().whine_sound
            } else {
                self.get_sound_set().pant_sound
            })
        } else {
            Some(self.get_sound_set().ambient_sound)
        }
    }

    fn get_hurt_sound_in(&self, level: &ServerLevel, source: &DamageSource) -> Option<SoundEvent> {
        Some(if self.can_armor_absorb(level, source) {
            SoundEvent::ITEM_WOLF_ARMOR_DAMAGE
        } else {
            self.get_sound_set().hurt_sound
        })
    }

    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(self.get_sound_set().death_sound)
    }

    fn get_sound_volume(&self) -> f32 {
        0.4
    }

    fn ai_step(&mut self, level: &mut ServerLevel) {
        mob::tick::ai_step(self, level);
        if self.is_wet
            && !self.is_shaking
            && !mob::pathfinder::is_path_finding(self, level)
            && self.base().on_ground()
        {
            self.is_shaking = true;
            self.shake_anim = 0.0;
            self.shake_anim_o = 0.0;
            broadcast_entity_event(level, self.id(), 8);
        }
        neutral::update_persistent_anger(self, level, true);
    }

    fn tick(&mut self, level: &mut ServerLevel) {
        mob::tick::tick(self, level);
        if self.is_alive() {
            self.interested_angle_o = self.interested_angle;
            if self.is_interested() {
                self.interested_angle += (1.0 - self.interested_angle) * 0.4;
            } else {
                self.interested_angle += (0.0 - self.interested_angle) * 0.4;
            }
            if self.is_in_water_or_rain(level) {
                self.is_wet = true;
                if self.is_shaking {
                    broadcast_entity_event(level, self.id(), 56);
                    self.cancel_shake();
                }
            } else if (self.is_wet || self.is_shaking) && self.is_shaking {
                if self.shake_anim == 0.0 {
                    let pitch = (self.base_mut().random.next_float()
                        - self.base_mut().random.next_float())
                        * 0.2
                        + 1.0;
                    let volume = self.get_sound_volume();
                    self.play_sound(level, SoundEvent::ENTITY_WOLF_SHAKE, volume, pitch);
                    self.game_event(level, GameEvent::ENTITY_ACTION);
                }
                self.shake_anim_o = self.shake_anim;
                self.shake_anim += 0.05;
                if self.shake_anim_o >= 2.0 {
                    self.is_wet = false;
                    self.is_shaking = false;
                    self.shake_anim_o = 0.0;
                    self.shake_anim = 0.0;
                }
                if self.shake_anim > 0.4 {
                    let shake_count = (mth::sin(f64::from(
                        (self.shake_anim - 0.4) * std::f64::consts::PI as f32,
                    )) * 7.0) as i32;
                    // `level().addParticle(SPLASH, ...)` is client-only, but the two random draws per particle happen
                    for _ in 0..shake_count {
                        let _xo = (self.base_mut().random.next_float() * 2.0 - 1.0)
                            * self.base().get_bb_width()
                            * 0.5;
                        let _zo = (self.base_mut().random.next_float() * 2.0 - 1.0)
                            * self.base().get_bb_width()
                            * 0.5;
                    }
                }
            }
        }
    }

    fn die(&mut self, level: &mut ServerLevel, source: &DamageSource) {
        self.is_wet = false;
        self.is_shaking = false;
        self.shake_anim_o = 0.0;
        self.shake_anim = 0.0;
        tamable::die(self, level, source);
    }

    fn get_max_head_x_rot(&self) -> i32 {
        if tamable::is_in_sitting_pose(self) {
            20
        } else {
            40
        }
    }

    fn hurt_server(&mut self, level: &mut ServerLevel, source: &DamageSource, damage: f32) -> bool {
        if self.is_invulnerable_to(level, source) {
            return false;
        }
        tamable::set_ordered_to_sit(self, false);
        mc_game_core::world::entity::living_entity::damage::hurt_server(self, level, source, damage)
    }

    fn actually_hurt(&mut self, level: &mut ServerLevel, source: &DamageSource, damage: f32) {
        if !self.can_armor_absorb(level, source) {
            mc_game_core::world::entity::living_entity::damage::actually_hurt(
                self, level, source, damage,
            );
        } else {
            let armor_before = self.body_armor();
            let damage_before = armor_before.get_damage_value();
            let max_damage = armor_before.get_max_damage();
            self.hurt_and_break_equipment(level, EquipmentSlot::Body, mth::ceil_f32(damage));
            if Crackiness::WOLF_ARMOR.by_damage(damage_before, max_damage)
                != Crackiness::WOLF_ARMOR.by_damage_item(&self.body_armor())
            {
                self.play_sound(level, SoundEvent::ITEM_WOLF_ARMOR_CRACK, 1.0, 1.0);
                let particle = mc_protocol::packets::play::ParticleOptions::Item(
                    mc_protocol::components::ItemStackTemplate::of(Item::ARMADILLO_SCUTE),
                );
                let pos = self.position();
                level.send_particles(particle, pos.x, pos.y + 1.0, pos.z, 20, 0.2, 0.1, 0.2, 0.1);
            }
        }
    }

    fn apply_taming_side_effects(&mut self) {
        if tamable::is_tame(self) {
            if let Some(i) = self
                .mob
                .living
                .attributes
                .get_instance(mc_data::Attribute::MAX_HEALTH)
            {
                i.set_base_value(40.0);
            }
            self.mob.living.set_health(40.0);
        } else if let Some(i) = self
            .mob
            .living
            .attributes
            .get_instance(mc_data::Attribute::MAX_HEALTH)
        {
            i.set_base_value(8.0);
        }
    }

    fn hurt_armor(&mut self, level: &mut ServerLevel, source: &DamageSource, damage: f32) {
        mc_game_core::world::entity::living_entity::damage::do_hurt_equipment(
            self,
            level,
            source,
            damage,
            &[EquipmentSlot::Body],
        );
    }

    fn can_shear_equipment(&self, player: &ServerPlayer) -> bool {
        self.is_owned_by_player(player)
    }

    fn mob_interact(
        &mut self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        hand: InteractionHand,
    ) -> InteractionResult {
        let item_stack = player.get_item_in_hand(hand);
        let infinite = player.player.abilities.instabuild;
        if tamable::is_tame(self) {
            if self.is_food(&item_stack) && self.get_health() < self.mob.living.get_max_health() {
                tamable::feed(self, level, player, hand, &item_stack, 2.0, 2.0);
                return InteractionResult::SUCCESS;
            }
            let is_collar_dye = mc_data::tags::get::<Item>("wolf_collar_dyes")
                .is_some_and(|t| t.contains(item_stack.item()));
            if !is_collar_dye || !self.is_owned_by_player(player) {
                if self.is_equippable_in_slot(&item_stack, EquipmentSlot::Body)
                    && self.body_armor().is_empty()
                    && self.is_owned_by_player(player)
                    && !self.is_baby()
                {
                    mob::equipment::set_item_slot_and_drop_when_killed(
                        self,
                        level,
                        EquipmentSlot::Body,
                        item_stack.copy_with_count(1),
                    );
                    interact::with_hand_stack(player, hand, |stack, _| stack.consume(1, infinite));
                    return InteractionResult::SUCCESS;
                }
                let armor = self.body_armor();
                if tamable::is_in_sitting_pose(self)
                    && !armor.is_empty()
                    && self.is_owned_by_player(player)
                    && armor.is_damaged()
                    && armor
                        .get(&mc_protocol::components::REPAIRABLE)
                        .is_some_and(|r| r.is_valid_repair_item(item_stack.item()))
                {
                    interact::with_hand_stack(player, hand, |stack, _| stack.shrink(1));
                    self.play_sound(level, SoundEvent::ITEM_WOLF_ARMOR_REPAIR, 1.0, 1.0);
                    let repair_unit = (armor.get_max_damage() as f32 * ARMOR_REPAIR_UNIT) as i32;
                    if let Some(armor) = self.item_by_slot_mut(EquipmentSlot::Body) {
                        armor.set_damage_value((armor.get_damage_value() - repair_unit).max(0));
                    }
                    return InteractionResult::SUCCESS;
                }
                let interaction_result = interact::mob_interact(self, level, player, hand);
                if !interaction_result.consumes_action() && self.is_owned_by_player(player) {
                    let ordered = tamable::is_ordered_to_sit(self);
                    tamable::set_ordered_to_sit(self, !ordered);
                    self.mob.living.jumping = false;
                    mob::navigation::stop(self, level);
                    mob::tick::set_target(self, level, None);
                    return match InteractionResult::SUCCESS {
                        InteractionResult::Success(s) => {
                            InteractionResult::Success(s.without_item())
                        }
                        other => other,
                    };
                }
                return interaction_result;
            }
            let color = item_stack.get(&DYE).copied();
            if let Some(color) = color
                && color != self.get_collar_color()
            {
                self.set_collar_color(color);
                interact::with_hand_stack(player, hand, |stack, _| stack.consume(1, infinite));
                return InteractionResult::SUCCESS;
            }
        } else if item_stack.is(Item::BONE) && !neutral::is_angry(self, level) {
            interact::with_hand_stack(player, hand, |stack, _| stack.consume(1, infinite));
            self.try_to_tame(level, player);
            return InteractionResult::SUCCESS_SERVER;
        }
        interact::mob_interact(self, level, player, hand)
    }

    fn is_food(&self, stack: &ItemStack) -> bool {
        is_wolf_food(stack)
    }

    fn get_max_spawn_cluster_size(&self) -> i32 {
        8
    }

    fn start_persistent_anger_timer(&mut self, level: &mut ServerLevel) {
        // `PERSISTENT_ANGER_TIME = TimeUtil.rangeOfSeconds(20, 39)` = UniformInt(400, 780)
        let time = self.base_mut().random.next_int_between_inclusive(400, 780);
        neutral::set_time_to_remain_angry(self, level, i64::from(time));
    }

    fn can_be_leashed_in(&self, level: &ServerLevel) -> bool {
        !neutral::is_angry(self, level)
    }

    fn get_leash_offset(&self) -> Vec3 {
        Vec3::new(
            0.0,
            f64::from(0.6f32 * self.base().eye_height),
            f64::from(self.base().get_bb_width() * 0.4),
        )
    }

    fn tamed_wolf_owner(&self) -> Option<Option<uuid::Uuid>> {
        tamable::is_tame(self).then(|| tamable::get_owner_reference(self))
    }

    fn can_mate(&self, partner: &dyn Entity) -> bool {
        if partner.id() == self.id() || !tamable::is_tame(self) {
            return false;
        }
        let Some(wolf) = partner.as_any().downcast_ref::<Wolf>() else {
            return false;
        };
        if !tamable::is_tame(wolf) || tamable::is_in_sitting_pose(wolf) {
            return false;
        }
        mob::animal::is_in_love(self) && mob::animal::is_in_love(wolf)
    }

    fn wants_to_attack(
        &self,
        level: &ServerLevel,
        target: &dyn Entity,
        owner: &dyn Entity,
    ) -> bool {
        if matches!(
            target.get_type(),
            EntityType::CREEPER | EntityType::GHAST | EntityType::ARMOR_STAND
        ) {
            return false;
        }
        if let Some(wolf_target) = target.as_any().downcast_ref::<Wolf>() {
            return !tamable::is_tame(wolf_target)
                || tamable::get_owner(wolf_target, level) != Some(owner.id());
        }
        if let (Some(player_target), Some(player_owner)) = (
            target.as_any().downcast_ref::<ServerPlayer>(),
            owner.as_any().downcast_ref::<ServerPlayer>(),
        ) && !player_owner.can_harm_player(level, &player_target.get_scoreboard_name())
        {
            return false;
        }
        if target.as_abstract_horse().is_some_and(|h| h.is_tamed()) {
            return false;
        }
        !(target.mob().is_some_and(|m| m.tamable.is_some()) && tamable::is_tame(target))
    }

    fn get_breed_offspring(
        &mut self,
        level: &mut ServerLevel,
        partner: EntityId,
    ) -> Option<Box<dyn Entity>> {
        let mut baby = mc_game_core::world::entity::factory::create(
            EntityType::WOLF,
            level,
            EntitySpawnRequest {
                reason: EntitySpawnReason::Breeding,
                ignore_checks: false,
            },
        )?;
        let partner_info = level
            .get_entity(partner)
            .and_then(|p| p.as_any().downcast_ref::<Wolf>())
            .map(|p| (p.get_variant(), p.get_collar_color()));
        if let (Some((partner_variant, partner_collar)), Some(wolf)) =
            (partner_info, baby.as_any_mut().downcast_mut::<Wolf>())
        {
            let registries = level.registries.clone();
            let own_variant = self.get_variant();
            let pick = if self.base_mut().random.next_boolean() {
                own_variant
            } else {
                partner_variant
            };
            wolf.set_variant(&registries, pick as u32);
            if tamable::is_tame(self) {
                tamable::set_owner_reference(wolf, tamable::get_owner_reference(self));
                wolf.set_tame(None, true, true);
                let mixed =
                    dye_color::get_mixed_color(level, self.get_collar_color(), partner_collar);
                wolf.set_collar_color(mixed);
            }
            if let Some(key) = variant::pick_random_entry(
                &registries,
                keys::WOLF_SOUND_VARIANT,
                &mut self.base_mut().random,
            ) && let Some(id) = registries
                .registry(keys::WOLF_SOUND_VARIANT)
                .and_then(|r| r.id_of(&key))
            {
                wolf.set_sound_variant(&registries, id);
            }
        }
        Some(baby)
    }
}

/// `Wolf.checkWolfSpawnRules`.
fn check_wolf_spawn_rules(
    _ty: EntityType,
    level: &dyn ServerLevelAccessor,
    _reason: EntitySpawnReason,
    pos: BlockPos,
    _random: &mut dyn RandomSource,
) -> bool {
    use mc_game_core::world::level::block::BlockStateExt as _;
    level
        .get_block_state(pos.below())
        .is_tag("wolves_spawnable_on")
        && mc_game_core::world::entity::spawn_predicates::is_bright_enough_to_spawn(level, pos)
}

mc_game_core::register_spawn_placement!(EntityType::WOLF, check_wolf_spawn_rules);

/// The scenario dump fields of a wolf: `variant svariant collar wet shaking shake interest`.
#[distributed_slice(SCENARIO_DUMP_EXTRA)]
static WOLF_DUMP: ScenarioDumpExtra = ScenarioDumpExtra {
    order: 31,
    applies: |e| e.as_any().is::<Wolf>(),
    fields: |e, _level, _ctx| {
        let wolf = e.as_any().downcast_ref::<Wolf>().expect("applies");
        let f = |v: f32| mc_core::math::java::java_float_to_string(v);
        vec![
            ("variant".into(), wolf.variant_key.to_string()),
            ("svariant".into(), wolf.sound_variant_key.to_string()),
            ("collar".into(), wolf.get_collar_color().name().to_string()),
            ("wet".into(), u8::from(wolf.is_wet).to_string()),
            ("shaking".into(), u8::from(wolf.is_shaking).to_string()),
            ("shake".into(), f(wolf.shake_anim)),
            ("interest".into(), f(wolf.interested_angle)),
        ]
    },
};

#[distributed_slice(ENTITY_FACTORIES)]
static WOLF: EntityRegistration = EntityRegistration {
    entity_type: EntityType::WOLF,
    create: |ty, id, registries| Box::new(Wolf::new(ty, id, registries)),
};
