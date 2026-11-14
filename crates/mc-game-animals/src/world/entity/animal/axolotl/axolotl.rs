//! `net.minecraft.world.entity.animal.axolotl.Axolotl`: a bucketable amphibious animal with a brain; plays dead when hurt,
//! hunts aquatic mobs and gives regeneration to the player that it helped to kill something.

use super::axolotl_ai;
use linkme::distributed_slice;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::enums::EntitySpawnReason;
use mc_data::{EntityType, Item, MobEffect, SoundEvent};
use mc_game_core::define_accessor;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::damagesource::DamageSource;
use mc_game_core::world::difficulty_instance::DifficultyInstance;
use mc_game_core::world::entity::ai::brain::{Brain, Provider};
use mc_game_core::world::entity::ai::control::look_control::{LookControl, LookControlBase};
use mc_game_core::world::entity::ai::control::move_control::{MoveControl, MoveControlBase};
use mc_game_core::world::entity::ai::control::smooth_swimming_look_control::SmoothSwimmingLookControl;
use mc_game_core::world::entity::ai::control::smooth_swimming_move_control::SmoothSwimmingMoveControl;
use mc_game_core::world::entity::ai::memory::BrainPacked;
use mc_game_core::world::entity::ai::memory::memory_module_type::{
    ATTACK_TARGET, HAS_HUNTING_COOLDOWN, PLAY_DEAD_TICKS,
};
use mc_game_core::world::entity::ai::navigation::StandardPathNavigation;
use mc_game_core::world::entity::ai::sensing::sensor_type::{
    AXOLOTL_ATTACKABLES, FOOD_TEMPTATIONS, HURT_BY, NEAREST_ADULT, NEAREST_LIVING_ENTITIES,
};
use mc_game_core::world::entity::bucketable::{self, Bucketable};
use mc_game_core::world::entity::dimensions::{EntityAttachments, EntityDimensions};
use mc_game_core::world::entity::effect::MobEffectInstance;
use mc_game_core::world::entity::factory::{
    self, ENTITY_FACTORIES, EntityRegistration, EntitySpawnRequest,
};
use mc_game_core::world::entity::mob::spawn::AgeableMobGroupData;
use mc_game_core::world::entity::mob::{self, MobData, MobKind, ageable};
use mc_game_core::world::entity::spawn_group_data::SpawnGroupData;
use mc_game_core::world::entity::{Entity, EntityId, MoverType, ServerPlayer};
use mc_game_core::world::item::{
    InteractionHand, InteractionResult, ItemStack, create_filled_result,
};
use mc_game_core::world::level::LevelReader;
use mc_game_core::world::level::block::state::BlockStateExt;
use mc_game_core::world::level::pathfinder::PathType;
use mc_game_core::world::level::server_level_accessor::ServerLevelAccessor;
use mc_nbt::CompoundTag;
use mc_protocol::components::{AXOLOTL_VARIANT, AxolotlVariant, BUCKET_ENTITY_DATA, CustomData};

define_accessor!(pub DATA_VARIANT: Int = Axolotl);
define_accessor!(pub DATA_PLAYING_DEAD: Boolean = Axolotl);
define_accessor!(pub FROM_BUCKET: Boolean = Axolotl);

/// `TOTAL_PLAYDEAD_TIME`.
pub const TOTAL_PLAYDEAD_TIME: i32 = 200;
/// `PLAYER_REGEN_DETECTION_RANGE`.
pub const PLAYER_REGEN_DETECTION_RANGE: f64 = 20.0;
/// `RARE_VARIANT_CHANCE`.
pub const RARE_VARIANT_CHANCE: i32 = 1200;
/// `AXOLOTL_TOTAL_AIR_SUPPLY`.
const AXOLOTL_TOTAL_AIR_SUPPLY: i32 = 6000;
/// `REHYDRATE_AIR_SUPPLY`.
const REHYDRATE_AIR_SUPPLY: i32 = 1800;
/// `REGEN_BUFF_MAX_DURATION`.
const REGEN_BUFF_MAX_DURATION: i32 = 2400;
/// `REGEN_BUFF_BASE_DURATION`.
const REGEN_BUFF_BASE_DURATION: i32 = 100;

/// `BRAIN_PROVIDER`.
static BRAIN_PROVIDER: Provider = Provider::with_activities(
    &[
        &NEAREST_LIVING_ENTITIES,
        &NEAREST_ADULT,
        &HURT_BY,
        &AXOLOTL_ATTACKABLES,
        &FOOD_TEMPTATIONS,
    ],
    axolotl_ai::get_activities,
);

/// `Axolotl.Variant.DEFAULT`.
pub const DEFAULT_VARIANT: AxolotlVariant = AxolotlVariant::Lucy;

/// `Axolotl.Variant.common` (only `blue` is rare).
fn is_common(variant: AxolotlVariant) -> bool {
    variant != AxolotlVariant::Blue
}

/// `Axolotl.Variant.byId(id)` (`ByIdMap.OutOfBoundsStrategy.ZERO`).
pub fn variant_by_id(id: i32) -> AxolotlVariant {
    AxolotlVariant::from_id(id).unwrap_or(DEFAULT_VARIANT)
}

/// `Axolotl.Variant.getSpawnVariant(random, common)`: `Util.getRandom(validVariants, random)`.
fn get_spawn_variant(random: &mut dyn RandomSource, common: bool) -> AxolotlVariant {
    let valid: Vec<AxolotlVariant> = AxolotlVariant::ALL
        .iter()
        .copied()
        .filter(|&v| is_common(v) == common)
        .collect();
    valid[random.next_int_bounded(valid.len() as i32) as usize]
}

/// `Axolotl.Variant.getCommonSpawnVariant(random)`.
pub fn get_common_spawn_variant(random: &mut dyn RandomSource) -> AxolotlVariant {
    get_spawn_variant(random, true)
}

/// `Axolotl.Variant.getRareSpawnVariant(random)`.
pub fn get_rare_spawn_variant(random: &mut dyn RandomSource) -> AxolotlVariant {
    get_spawn_variant(random, false)
}

/// `useRareVariant(random)`.
fn use_rare_variant(random: &mut dyn RandomSource) -> bool {
    random.next_int_bounded(RARE_VARIANT_CHANCE) == 0
}

/// `Axolotl.AxolotlGroupData`.
pub struct AxolotlGroupData {
    base: AgeableMobGroupData,
    pub types: Vec<AxolotlVariant>,
}

impl AxolotlGroupData {
    /// `new AxolotlGroupData(types...)`.
    pub fn new(types: Vec<AxolotlVariant>) -> Self {
        Self {
            base: AgeableMobGroupData::with_baby(false),
            types,
        }
    }

    /// `getVariant(random)`.
    pub fn get_variant(&self, random: &mut dyn RandomSource) -> AxolotlVariant {
        self.types[random.next_int_bounded(self.types.len() as i32) as usize]
    }
}

impl SpawnGroupData for AxolotlGroupData {
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

/// `BABY_DIMENSIONS`.
fn baby_dimensions() -> EntityDimensions {
    let dims = EntityDimensions::scalable(0.375, 0.21).with_eye_height(0.09375);
    dims.with_attachments(
        EntityAttachments::create_default(0.375, 0.21).with_passenger(Vec3::new(0.0, 0.1875, 0.0)),
    )
}

/// `isPlayingDead()`.
fn is_playing_dead_of(e: &dyn Entity) -> bool {
    e.base().entity_data.get(DATA_PLAYING_DEAD)
}

/// `Axolotl`.
pub struct Axolotl {
    pub mob: MobData,
}

impl Axolotl {
    /// `new Axolotl(type, level)`.
    pub fn new(ty: EntityType, id: EntityId) -> Self {
        let mob =
            MobData::new_with_air_supply(ty, id, MobKind::Animal, AXOLOTL_TOTAL_AIR_SUPPLY, |b| {
                b.define(DATA_VARIANT, 0);
                b.define(DATA_PLAYING_DEAD, false);
                b.define(FROM_BUCKET, false);
            });
        let mut axolotl = Axolotl { mob };
        // `LivingEntity.<init>`: `this.brain = this.makeBrain(Brain.Packed.EMPTY)`
        let brain = axolotl.make_brain(&BrainPacked::EMPTY);
        axolotl.mob.living.brain = brain;
        let navigation = StandardPathNavigation::amphibious(&axolotl);
        axolotl.mob.set_navigation(Box::new(navigation));
        axolotl.mob.set_pathfinding_malus(PathType::Water, 0.0);
        axolotl.mob.move_control = Some(Box::new(AxolotlMoveControl::new()));
        axolotl.mob.look_control = Some(Box::new(AxolotlLookControl::new(20)));
        axolotl
    }

    /// `getVariant()`.
    pub fn get_variant(&self) -> AxolotlVariant {
        variant_by_id(self.mob.living.entity.entity_data.get(DATA_VARIANT))
    }

    /// `setVariant(variant)`.
    pub fn set_variant(&mut self, variant: AxolotlVariant) {
        self.mob
            .living
            .entity
            .entity_data
            .set(DATA_VARIANT, variant.id());
    }

    /// `setPlayingDead(playingDead)`.
    pub fn set_playing_dead(&mut self, playing_dead: bool) {
        self.mob
            .living
            .entity
            .entity_data
            .set(DATA_PLAYING_DEAD, playing_dead);
    }

    /// `isPlayingDead()`.
    pub fn is_playing_dead(&self) -> bool {
        is_playing_dead_of(self)
    }

    /// `handleAirSupply(level, preTickAirSupply)`.
    fn handle_air_supply(&mut self, level: &mut ServerLevel, pre_tick_air_supply: i32) {
        if self.is_alive() && !self.is_in_water_or_rain(level) {
            self.base_mut().set_air_supply(pre_tick_air_supply - 1);
            if self.should_take_drowning_damage() {
                self.base_mut().set_air_supply(0);
                let source = level.damage_sources().dry_out();
                self.hurt_server(level, &source, 2.0);
            }
        } else {
            let max = self.get_max_air_supply();
            self.base_mut().set_air_supply(max);
        }
    }

    /// `applySupportingEffects(player)`: regeneration for the player the axolotl helped (and no mining fatigue).
    pub fn apply_supporting_effects(&self, level: &mut ServerLevel, player: EntityId) {
        let source = self.id();
        level.with_entity(player, |level, p| {
            let regen = p
                .get_effect(MobEffect::REGENERATION)
                .map(|e| (e.duration, e.ends_within(REGEN_BUFF_MAX_DURATION - 1)));
            if regen.is_none_or(|(_, ends_within)| ends_within) {
                let previous_duration = regen.map_or(0, |(d, _)| d);
                let regen_duration =
                    REGEN_BUFF_MAX_DURATION.min(REGEN_BUFF_BASE_DURATION + previous_duration);
                p.add_effect(
                    level,
                    MobEffectInstance::of(MobEffect::REGENERATION, regen_duration, 0),
                    Some(source),
                );
            }
            p.remove_effect(level, MobEffect::MINING_FATIGUE);
        });
    }
}

/// `Axolotl.onStopAttacking(level, body, target)`: the `StopAttackingIfTargetInvalid` callback.
pub fn on_stop_attacking(level: &mut ServerLevel, body: &mut dyn Entity, target: EntityId) {
    let Some(target) = level.get_entity_or_recently_removed(target) else {
        return;
    };
    if !target.is_dead_or_dying() {
        return;
    }
    let Some(last_damage_source) =
        mc_game_core::world::entity::ai::sensing::hurt_by_sensor::get_last_damage_source(
            level, target,
        )
    else {
        return;
    };
    let Some(player) = last_damage_source
        .get_entity()
        .filter(|&id| level.get_entity(id).is_some_and(|e| e.is_player()))
    else {
        return;
    };
    let range = body
        .base()
        .bounding_box()
        .inflate_scalar(PLAYER_REGEN_DETECTION_RANGE);
    let players_in_range = level.get_entities_of_class(&range, |e| e.is_player(), |_| true);
    if players_in_range.contains(&player)
        && let Some(axolotl) = body.as_any().downcast_ref::<Axolotl>()
    {
        axolotl.apply_supporting_effects(level, player);
    }
}

/// `Axolotl.checkAxolotlSpawnRules`.
pub fn check_axolotl_spawn_rules(
    _ty: EntityType,
    level: &dyn ServerLevelAccessor,
    _reason: EntitySpawnReason,
    pos: BlockPos,
    _random: &mut dyn RandomSource,
) -> bool {
    level
        .get_block_state(pos.below())
        .is_tag("axolotls_spawnable_on")
}

mc_game_core::register_spawn_placement!(EntityType::AXOLOTL, check_axolotl_spawn_rules);

/// `Axolotl.AxolotlMoveControl`: a `SmoothSwimmingMoveControl` that does nothing while playing dead.
struct AxolotlMoveControl {
    inner: SmoothSwimmingMoveControl,
}

impl AxolotlMoveControl {
    fn new() -> Self {
        Self {
            inner: SmoothSwimmingMoveControl::new(85, 10, 0.1, 0.5, false),
        }
    }
}

impl MoveControl for AxolotlMoveControl {
    fn base(&self) -> &MoveControlBase {
        self.inner.base()
    }
    fn base_mut(&mut self) -> &mut MoveControlBase {
        self.inner.base_mut()
    }
    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        if !is_playing_dead_of(mob) {
            self.inner.tick(mob, level);
        }
    }
}

/// `Axolotl.AxolotlLookControl`: a `SmoothSwimmingLookControl` that does nothing while playing dead.
struct AxolotlLookControl {
    inner: SmoothSwimmingLookControl,
}

impl AxolotlLookControl {
    fn new(max_y_rot_from_center: i32) -> Self {
        Self {
            inner: SmoothSwimmingLookControl::new(max_y_rot_from_center),
        }
    }
}

impl LookControl for AxolotlLookControl {
    fn base(&self) -> &LookControlBase {
        self.inner.base()
    }
    fn base_mut(&mut self) -> &mut LookControlBase {
        self.inner.base_mut()
    }
    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        if !is_playing_dead_of(mob) {
            self.inner.tick(mob, level);
        }
    }
}

/// `itemStack.is(ItemTags.AXOLOTL_FOOD)`.
fn is_axolotl_food(stack: &ItemStack) -> bool {
    mc_data::tags::get::<Item>("axolotl_food").is_some_and(|t| t.contains(stack.item()))
}

impl Bucketable for Axolotl {
    fn from_bucket(&self) -> bool {
        self.mob.living.entity.entity_data.get(FROM_BUCKET)
    }
    fn set_from_bucket(&mut self, from_bucket: bool) {
        self.mob
            .living
            .entity
            .entity_data
            .set(FROM_BUCKET, from_bucket);
    }
    fn save_to_bucket_tag(&self, bucket: &mut ItemStack) {
        bucketable::save_default_data_to_bucket_tag(self, bucket);
        bucket.set(AXOLOTL_VARIANT, self.get_variant());
        let mut tag = bucket
            .get(&BUCKET_ENTITY_DATA)
            .map(|d| d.copy_tag())
            .unwrap_or_default();
        tag.put_int("Age", ageable::get_age(self));
        tag.put_boolean("AgeLocked", ageable::is_age_locked(self));
        let brain = self.brain();
        if brain.has_memory_value(HAS_HUNTING_COOLDOWN.id()) {
            tag.put_long(
                "HuntingCooldown",
                brain.get_time_until_expiry(&HAS_HUNTING_COOLDOWN),
            );
        }
        bucket.set(BUCKET_ENTITY_DATA, CustomData::of(&tag));
    }
    fn load_from_bucket_tag(&mut self, tag: &CompoundTag) {
        bucketable::load_default_data_from_bucket_tag(self, tag);
        ageable::set_age(self, tag.get_int_or("Age", 0));
        ageable::set_age_locked(self, tag.get_boolean_or("AgeLocked", false));
        match tag.get_long("HuntingCooldown") {
            Some(_) => self.brain_mut().set_memory_with_expiry(
                &HAS_HUNTING_COOLDOWN,
                true,
                tag.get_long_or("HuntingCooldown", 0),
            ),
            None => self.brain_mut().erase_memory(&HAS_HUNTING_COOLDOWN),
        }
    }
    fn get_bucket_item_stack(&self) -> ItemStack {
        ItemStack::new(Item::AXOLOTL_BUCKET)
    }
    fn get_pickup_sound(&self) -> SoundEvent {
        SoundEvent::ITEM_BUCKET_FILL_AXOLOTL
    }
}

impl Entity for Axolotl {
    mc_game_core::mob_entity_accessors!(mob);

    fn get_encode_id(&self) -> Option<&'static str> {
        Some("minecraft:axolotl")
    }

    fn get_dimensions(&self, _pose: mc_data::enums::Pose) -> EntityDimensions {
        if self.is_baby() {
            baby_dimensions()
        } else {
            EntityDimensions::of_type(self.mob.living.entity.entity_type)
        }
    }

    fn get_walk_target_value(&self, _pos: BlockPos, _level: &dyn LevelReader) -> f32 {
        0.0
    }

    fn make_brain(&mut self, packed: &BrainPacked) -> Brain {
        BRAIN_PROVIDER.make_brain(self, packed)
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        mob::nbt::add_additional_save_data(self, out);
        out.put_int("Variant", self.get_variant().id());
        out.put_boolean("FromBucket", Bucketable::from_bucket(self));
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        mob::nbt::read_additional_save_data(self, input);
        self.set_variant(variant_by_id(
            input.get_int_or("Variant", DEFAULT_VARIANT.id()),
        ));
        Bucketable::set_from_bucket(self, input.get_boolean_or("FromBucket", false));
    }

    fn play_ambient_sound(&mut self, level: &mut ServerLevel) {
        if !self.is_playing_dead() {
            mob::tick::play_ambient_sound(self, level);
        }
    }

    fn finalize_spawn(
        &mut self,
        level: &mut dyn ServerLevelAccessor,
        difficulty: DifficultyInstance,
        reason: EntitySpawnReason,
        group_data: Option<Box<dyn SpawnGroupData>>,
    ) -> Option<Box<dyn SpawnGroupData>> {
        if reason == EntitySpawnReason::Bucket {
            return group_data;
        }
        let mut is_baby = false;
        let group_data = match group_data {
            Some(mut data) if data.as_any().is::<AxolotlGroupData>() => {
                if data
                    .as_ageable_mut()
                    .is_some_and(|d| d.get_group_size() >= 2)
                {
                    is_baby = true;
                }
                data
            }
            _ => {
                let random = level.random();
                let first = get_common_spawn_variant(random);
                let second = get_common_spawn_variant(random);
                Box::new(AxolotlGroupData::new(vec![first, second])) as Box<dyn SpawnGroupData>
            }
        };
        let variant = group_data
            .as_any()
            .downcast_ref::<AxolotlGroupData>()
            .expect("axolotl group data")
            .get_variant(level.random());
        self.set_variant(variant);
        if is_baby {
            ageable::set_age(self, -24000);
        }
        mob::spawn::finalize_spawn(self, level, difficulty, reason, Some(group_data))
    }

    fn base_tick(&mut self, level: &mut ServerLevel) {
        let air_supply = self.base().get_air_supply();
        mob::tick::base_tick(self, level);
        if !self.mob.is_no_ai() {
            self.handle_air_supply(level, air_supply);
        }
    }

    fn rehydrate(&mut self) {
        let new_air_supply = self.base().get_air_supply() + REHYDRATE_AIR_SUPPLY;
        let max = self.get_max_air_supply();
        self.base_mut().set_air_supply(new_air_supply.min(max));
    }

    fn get_max_air_supply(&self) -> i32 {
        AXOLOTL_TOTAL_AIR_SUPPLY
    }

    fn check_spawn_obstruction(&self, level: &dyn ServerLevelAccessor) -> bool {
        level.is_unobstructed_entity(self)
    }

    fn is_pushed_by_fluid(&self) -> bool {
        false
    }

    fn as_bucketable(&self) -> Option<&dyn Bucketable> {
        Some(self)
    }

    fn as_bucketable_mut(&mut self) -> Option<&mut dyn Bucketable> {
        Some(self)
    }

    fn entity_components(&self) -> mc_protocol::components::DataComponentMap {
        let base = mc_game_core::advancements::predicates::entity::support::base_entity_components(
            self.base(),
        );
        mc_protocol::components::DataComponentMap::builder()
            .add_all(&base)
            .set(AXOLOTL_VARIANT, self.get_variant())
            .build()
    }

    fn apply_implicit_components(&mut self, stack: &ItemStack) {
        if let Some(variant) = stack.get(&AXOLOTL_VARIANT) {
            self.set_variant(*variant);
        }
        mc_game_core::world::entity::spawn_config::apply_base_implicit_components(self, stack);
    }

    fn get_breed_offspring(
        &mut self,
        level: &mut ServerLevel,
        partner: EntityId,
    ) -> Option<Box<dyn Entity>> {
        let mut baby = factory::create(
            EntityType::AXOLOTL,
            level,
            EntitySpawnRequest {
                reason: EntitySpawnReason::Breeding,
                ignore_checks: false,
            },
        )?;
        let own_variant = self.get_variant();
        let variant = if use_rare_variant(&mut self.base_mut().random) {
            get_rare_spawn_variant(&mut self.base_mut().random)
        } else if self.base_mut().random.next_boolean() {
            own_variant
        } else {
            level
                .get_entity(partner)
                .and_then(|p| p.as_any().downcast_ref::<Axolotl>())
                .map_or(own_variant, Axolotl::get_variant)
        };
        if let Some(axolotl) = baby.as_any_mut().downcast_mut::<Axolotl>() {
            axolotl.set_variant(variant);
        }
        baby.set_persistence_required();
        Some(baby)
    }

    fn is_food(&self, stack: &ItemStack) -> bool {
        is_axolotl_food(stack)
    }

    fn can_be_leashed(&self) -> bool {
        true
    }

    fn custom_server_ai_step(&mut self, level: &mut ServerLevel) {
        Brain::tick(level, self);
        axolotl_ai::update_activity(self);
        if !self.mob.is_no_ai() {
            let playing_dead = self
                .brain()
                .get_memory(&PLAY_DEAD_TICKS)
                .is_some_and(|&ticks| ticks > 0);
            self.set_playing_dead(playing_dead);
        }
    }

    fn play_attack_sound(&mut self, level: &mut ServerLevel) {
        self.play_sound(level, SoundEvent::ENTITY_AXOLOTL_ATTACK, 1.0, 1.0);
    }

    fn hurt_server(&mut self, level: &mut ServerLevel, source: &DamageSource, damage: f32) -> bool {
        let current_health = self.get_health();
        if !self.mob.is_no_ai()
            && self.base_mut().random.next_int_bounded(3) == 0
            && ((self.base_mut().random.next_int_bounded(3) as f32) < damage
                || current_health / self.mob.living.get_max_health() < 0.5)
            && damage < current_health
            && self.is_in_water()
            && (source.get_entity().is_some() || source.get_direct_entity().is_some())
            && !self.is_playing_dead()
        {
            self.brain_mut()
                .set_memory(&PLAY_DEAD_TICKS, TOTAL_PLAYDEAD_TIME);
        }
        mc_game_core::world::entity::living_entity::damage::hurt_server(self, level, source, damage)
    }

    fn get_max_head_x_rot(&self) -> i32 {
        1
    }

    fn get_max_head_y_rot(&self) -> i32 {
        1
    }

    fn mob_interact(
        &mut self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        hand: InteractionHand,
    ) -> InteractionResult {
        match bucketable::bucket_mob_pickup_in_place(level, player, hand, self) {
            Some(result) => result,
            None => mob::interact::mob_interact(self, level, player, hand),
        }
    }

    fn can_be_seen_as_enemy(&self) -> bool {
        !self.is_playing_dead()
            && !self.base().permanently_invulnerable
            && self.can_be_seen_by_anyone()
    }

    fn requires_custom_persistence(&self) -> bool {
        (self.base().is_passenger() || mob::leashable::is_leashed(self))
            || Bucketable::from_bucket(self)
    }

    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_AXOLOTL_HURT)
    }

    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_AXOLOTL_DEATH)
    }

    fn get_ambient_sound(&self) -> Option<SoundEvent> {
        Some(if self.is_in_water() {
            SoundEvent::ENTITY_AXOLOTL_IDLE_WATER
        } else {
            SoundEvent::ENTITY_AXOLOTL_IDLE_AIR
        })
    }

    fn get_swim_splash_sound(&self) -> SoundEvent {
        SoundEvent::ENTITY_AXOLOTL_SPLASH
    }

    fn get_swim_sound(&self) -> SoundEvent {
        SoundEvent::ENTITY_AXOLOTL_SWIM
    }

    fn travel_in_water(
        &mut self,
        level: &mut ServerLevel,
        input: Vec3,
        _base_gravity: f64,
        _is_falling: bool,
        _old_y: f64,
    ) {
        let speed = self.get_speed();
        self.move_relative(speed, input);
        let delta = self.base().delta_movement();
        self.r#move(level, MoverType::SelfMover, delta);
        let scaled = self.base().delta_movement().scale(0.9);
        self.base_mut().set_delta_movement(scaled);
    }

    fn use_player_item(
        &mut self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        hand: InteractionHand,
    ) {
        let item_stack = player.get_item_in_hand(hand);
        if item_stack.item() == Item::TROPICAL_FISH_BUCKET {
            let filled = create_filled_result(
                item_stack,
                &mut mc_game_core::world::entity::player::menu_ctx::PlayerCtx { player, level },
                ItemStack::new(Item::WATER_BUCKET),
                true,
            );
            player.set_item_in_hand(hand, filled);
        } else {
            mob::interact::use_player_item(level, player, hand);
        }
    }

    fn remove_when_far_away(&self, _dist_sqr: f64) -> bool {
        !Bucketable::from_bucket(self) && !self.base().has_custom_name()
    }

    fn get_target(&self) -> Option<EntityId> {
        self.brain().get_memory(&ATTACK_TARGET).map(|r| r.0)
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static AXOLOTL: EntityRegistration = EntityRegistration {
    entity_type: EntityType::AXOLOTL,
    create: |ty, id, _registries| Box::new(Axolotl::new(ty, id)),
};

/// The scenario dump fields of an axolotl: `variant playdead bucket`.
#[distributed_slice(mc_game_core::world::entity::scenario_dump::SCENARIO_DUMP_EXTRA)]
static AXOLOTL_DUMP: mc_game_core::world::entity::scenario_dump::ScenarioDumpExtra =
    mc_game_core::world::entity::scenario_dump::ScenarioDumpExtra {
        order: 30,
        applies: |e| e.as_any().is::<Axolotl>(),
        fields: |e, _level, _ctx| {
            let a = e.as_any().downcast_ref::<Axolotl>().expect("applies");
            vec![
                ("variant".into(), a.get_variant().name().to_string()),
                ("playdead".into(), u8::from(a.is_playing_dead()).to_string()),
                (
                    "bucket".into(),
                    u8::from(Bucketable::from_bucket(a)).to_string(),
                ),
            ]
        },
    };
