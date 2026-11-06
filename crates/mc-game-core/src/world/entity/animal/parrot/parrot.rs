//! `Parrot` (+ `ShoulderRidingEntity`): flying tameable that imitates mobs, dances to jukeboxes and lands on its owner.

use crate::define_accessor;
use crate::registry::RegistryAccess;
use crate::server::level::ServerLevel;
use crate::world::damagesource::DamageSource;
use crate::world::difficulty_instance::DifficultyInstance;
use crate::world::entity::ai::control::FlyingMoveControl;
use crate::world::entity::ai::goal::{
    FloatGoal, FollowMobGoal, FollowOwnerGoal, Goal, GoalFlags, LandOnOwnersShoulderGoal,
    LookAtPlayerGoal, ShoulderRidingAccess, SitWhenOrderedToGoal, TamableAnimalPanicGoal,
    WaterAvoidingRandomFlyingGoal,
};
use crate::world::entity::ai::navigation::{PathNavigation, StandardPathNavigation};
use crate::world::entity::ai::util::land_random_pos;
use crate::world::entity::effect::MobEffectInstance;
use crate::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use crate::world::entity::living_entity::util::broadcast_entity_event;
use crate::world::entity::mob::spawn::AgeableMobGroupData;
use crate::world::entity::mob::{self, MobData, MobKind, interact, tamable};
use crate::world::entity::spawn_group_data::SpawnGroupData;
use crate::world::entity::spawn_predicates;
use crate::world::entity::{Entity, EntityId, ServerPlayer};
use crate::world::item::{InteractionHand, InteractionResult, ItemStack};
use crate::world::level::block::BlockStateExt as _;
use crate::world::level::level_access::LevelAccessor as _;
use crate::world::level::pathfinder::PathType;
use crate::world::level::server_level_accessor::ServerLevelAccessor;
use crate::world::level::{BlockGetter as _, LevelReader as _};
use linkme::distributed_slice;
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::Block;
use mc_data::enums::EntitySpawnReason;
use mc_data::{EntityType, Item, MobEffect, SoundEvent};
use mc_nbt::CompoundTag;
use mc_protocol::packets::play::SoundSource;
use mc_world::level_storage::Difficulty;

define_accessor!(pub DATA_VARIANT_ID: Int = Parrot);

pub const RIDE_COOLDOWN: i32 = 100;

/// `MOB_SOUND_MAP` in declaration order (`None` = `SoundEvents.EMPTY`).
static MOB_SOUND_MAP: &[(EntityType, Option<SoundEvent>)] = &[
    (
        EntityType::BLAZE,
        Some(SoundEvent::ENTITY_PARROT_IMITATE_BLAZE),
    ),
    (
        EntityType::BOGGED,
        Some(SoundEvent::ENTITY_PARROT_IMITATE_BOGGED),
    ),
    (
        EntityType::BREEZE,
        Some(SoundEvent::ENTITY_PARROT_IMITATE_BREEZE),
    ),
    (
        EntityType::CAMEL_HUSK,
        Some(SoundEvent::ENTITY_PARROT_IMITATE_CAMEL_HUSK),
    ),
    (
        EntityType::CAVE_SPIDER,
        Some(SoundEvent::ENTITY_PARROT_IMITATE_SPIDER),
    ),
    (
        EntityType::CREAKING,
        Some(SoundEvent::ENTITY_PARROT_IMITATE_CREAKING),
    ),
    (
        EntityType::CREEPER,
        Some(SoundEvent::ENTITY_PARROT_IMITATE_CREEPER),
    ),
    (
        EntityType::DROWNED,
        Some(SoundEvent::ENTITY_PARROT_IMITATE_DROWNED),
    ),
    (
        EntityType::ELDER_GUARDIAN,
        Some(SoundEvent::ENTITY_PARROT_IMITATE_ELDER_GUARDIAN),
    ),
    (
        EntityType::ENDER_DRAGON,
        Some(SoundEvent::ENTITY_PARROT_IMITATE_ENDER_DRAGON),
    ),
    (
        EntityType::ENDERMITE,
        Some(SoundEvent::ENTITY_PARROT_IMITATE_ENDERMITE),
    ),
    (
        EntityType::EVOKER,
        Some(SoundEvent::ENTITY_PARROT_IMITATE_EVOKER),
    ),
    (
        EntityType::GHAST,
        Some(SoundEvent::ENTITY_PARROT_IMITATE_GHAST),
    ),
    (EntityType::HAPPY_GHAST, None),
    (
        EntityType::GUARDIAN,
        Some(SoundEvent::ENTITY_PARROT_IMITATE_GUARDIAN),
    ),
    (
        EntityType::HOGLIN,
        Some(SoundEvent::ENTITY_PARROT_IMITATE_HOGLIN),
    ),
    (
        EntityType::HUSK,
        Some(SoundEvent::ENTITY_PARROT_IMITATE_HUSK),
    ),
    (
        EntityType::ILLUSIONER,
        Some(SoundEvent::ENTITY_PARROT_IMITATE_ILLUSIONER),
    ),
    (
        EntityType::MAGMA_CUBE,
        Some(SoundEvent::ENTITY_PARROT_IMITATE_MAGMA_CUBE),
    ),
    (
        EntityType::PARCHED,
        Some(SoundEvent::ENTITY_PARROT_IMITATE_PARCHED),
    ),
    (
        EntityType::PHANTOM,
        Some(SoundEvent::ENTITY_PARROT_IMITATE_PHANTOM),
    ),
    (
        EntityType::PIGLIN,
        Some(SoundEvent::ENTITY_PARROT_IMITATE_PIGLIN),
    ),
    (
        EntityType::PIGLIN_BRUTE,
        Some(SoundEvent::ENTITY_PARROT_IMITATE_PIGLIN_BRUTE),
    ),
    (
        EntityType::PILLAGER,
        Some(SoundEvent::ENTITY_PARROT_IMITATE_PILLAGER),
    ),
    (
        EntityType::RAVAGER,
        Some(SoundEvent::ENTITY_PARROT_IMITATE_RAVAGER),
    ),
    (
        EntityType::SHULKER,
        Some(SoundEvent::ENTITY_PARROT_IMITATE_SHULKER),
    ),
    (
        EntityType::SILVERFISH,
        Some(SoundEvent::ENTITY_PARROT_IMITATE_SILVERFISH),
    ),
    (
        EntityType::SKELETON,
        Some(SoundEvent::ENTITY_PARROT_IMITATE_SKELETON),
    ),
    (
        EntityType::SLIME,
        Some(SoundEvent::ENTITY_PARROT_IMITATE_SLIME),
    ),
    (
        EntityType::SPIDER,
        Some(SoundEvent::ENTITY_PARROT_IMITATE_SPIDER),
    ),
    (
        EntityType::STRAY,
        Some(SoundEvent::ENTITY_PARROT_IMITATE_STRAY),
    ),
    (EntityType::VEX, Some(SoundEvent::ENTITY_PARROT_IMITATE_VEX)),
    (
        EntityType::VINDICATOR,
        Some(SoundEvent::ENTITY_PARROT_IMITATE_VINDICATOR),
    ),
    (
        EntityType::WARDEN,
        Some(SoundEvent::ENTITY_PARROT_IMITATE_WARDEN),
    ),
    (
        EntityType::WITCH,
        Some(SoundEvent::ENTITY_PARROT_IMITATE_WITCH),
    ),
    (
        EntityType::WITHER,
        Some(SoundEvent::ENTITY_PARROT_IMITATE_WITHER),
    ),
    (
        EntityType::WITHER_SKELETON,
        Some(SoundEvent::ENTITY_PARROT_IMITATE_WITHER_SKELETON),
    ),
    (
        EntityType::ZOGLIN,
        Some(SoundEvent::ENTITY_PARROT_IMITATE_ZOGLIN),
    ),
    (
        EntityType::ZOMBIE,
        Some(SoundEvent::ENTITY_PARROT_IMITATE_ZOMBIE),
    ),
    (
        EntityType::ZOMBIE_HORSE,
        Some(SoundEvent::ENTITY_PARROT_IMITATE_ZOMBIE_HORSE),
    ),
    (
        EntityType::ZOMBIE_NAUTILUS,
        Some(SoundEvent::ENTITY_PARROT_IMITATE_ZOMBIE_NAUTILUS),
    ),
    (
        EntityType::ZOMBIE_VILLAGER,
        Some(SoundEvent::ENTITY_PARROT_IMITATE_ZOMBIE_VILLAGER),
    ),
];

/// `getImitatedSound(type)` (`getOrDefault(type, PARROT_AMBIENT)`); `Some(None)` is `SoundEvents.EMPTY`.
fn get_imitated_sound(ty: EntityType) -> Option<SoundEvent> {
    MOB_SOUND_MAP
        .iter()
        .find(|(t, _)| *t == ty)
        .map_or(Some(SoundEvent::ENTITY_PARROT_AMBIENT), |(_, s)| *s)
}

/// `Parrot.getPitch(random)`.
pub fn get_pitch(random: &mut dyn RandomSource) -> f32 {
    (random.next_float() - random.next_float()) * 0.2 + 1.0
}

/// `Parrot.getAmbient(level, random)`: `None` is `SoundEvents.EMPTY`.
pub fn get_ambient(difficulty: Difficulty, random: &mut dyn RandomSource) -> Option<SoundEvent> {
    if difficulty != Difficulty::Peaceful && random.next_int_bounded(1000) == 0 {
        let index = random.next_int_bounded(MOB_SOUND_MAP.len() as i32) as usize;
        get_imitated_sound(MOB_SOUND_MAP[index].0)
    } else {
        Some(SoundEvent::ENTITY_PARROT_AMBIENT)
    }
}

/// `Parrot.imitateNearbyMobs(level, entity)`.
pub fn imitate_nearby_mobs(level: &mut ServerLevel, entity: &dyn Entity) -> bool {
    if !(entity.is_alive() && !entity.base().is_silent() && level.random().next_int_bounded(2) == 0)
    {
        return false;
    }
    let bb = entity.base().bounding_box().inflate_scalar(20.0);
    let mobs = level.get_entities_of_class(
        &bb,
        |e| e.mob().is_some() && MOB_SOUND_MAP.iter().any(|(t, _)| *t == e.get_type()),
        |_| true,
    );
    if mobs.is_empty() {
        return false;
    }
    let pick = mobs[level.random().next_int_bounded(mobs.len() as i32) as usize];
    let Some(mob) = level.get_entity(pick) else {
        return false;
    };
    if mob.base().is_silent() {
        return false;
    }
    let sound = get_imitated_sound(mob.get_type());
    let pos = entity.position();
    let pitch = get_pitch(level.random());
    if let Some(sound) = sound {
        level.play_sound_at(
            None,
            pos.x,
            pos.y,
            pos.z,
            sound,
            entity.get_sound_source(),
            0.7,
            pitch,
        );
    }
    true
}

/// `Parrot.Variant`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ParrotVariant {
    RedBlue = 0,
    Blue = 1,
    Green = 2,
    YellowBlue = 3,
    Gray = 4,
}

impl ParrotVariant {
    pub const ALL: [ParrotVariant; 5] = [
        Self::RedBlue,
        Self::Blue,
        Self::Green,
        Self::YellowBlue,
        Self::Gray,
    ];

    /// `byId(id)` (`OutOfBoundsStrategy.CLAMP`).
    pub fn by_id(id: i32) -> ParrotVariant {
        Self::ALL[id.clamp(0, 4) as usize]
    }
}

/// `Parrot.ParrotWanderGoal`.
pub struct ParrotWanderGoal {
    inner: WaterAvoidingRandomFlyingGoal,
}

impl ParrotWanderGoal {
    pub fn new(speed_modifier: f64) -> Self {
        Self {
            inner: WaterAvoidingRandomFlyingGoal::new(speed_modifier),
        }
    }

    /// `getPosition()`.
    fn get_position(
        probability: f32,
        mob: &mut dyn Entity,
        level: &mut ServerLevel,
    ) -> Option<Vec3> {
        let mut pos = None;
        if mob.is_in_water() {
            pos = land_random_pos::get_pos(mob, level, 15, 15);
        }
        if mob.base_mut().random.next_float() >= probability {
            pos = Self::get_tree_pos(mob, level);
        }
        match pos {
            None => WaterAvoidingRandomFlyingGoal::get_position(mob, level),
            some => some,
        }
    }

    /// `getTreePos()`.
    fn get_tree_pos(mob: &mut dyn Entity, level: &ServerLevel) -> Option<Vec3> {
        let mob_pos = mob.block_position();
        let (x, y, z) = (mob.base().x(), mob.base().y(), mob.base().z());
        for pos in BlockPos::between_closed_xyz(
            mth::floor_f64(x - 3.0),
            mth::floor_f64(y - 6.0),
            mth::floor_f64(z - 3.0),
            mth::floor_f64(x + 3.0),
            mth::floor_f64(y + 6.0),
            mth::floor_f64(z + 3.0),
        ) {
            if mob_pos != pos {
                let state = level.get_block_state(pos.below());
                let can_sit_on = mc_data::tags::get::<Block>("leaves")
                    .is_some_and(|t| t.contains(state.block()))
                    || state.is_tag("logs");
                if can_sit_on && level.is_empty_block(pos) && level.is_empty_block(pos.above()) {
                    return Some(Vec3::at_bottom_center_of(&pos));
                }
            }
        }
        None
    }
}

impl Goal for ParrotWanderGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        let probability = self.inner.base.probability;
        self.inner
            .base
            .base
            .can_use_with(mob, level, |m, l| Self::get_position(probability, m, l))
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
        self.inner.flags()
    }
}

/// `Parrot`.
pub struct Parrot {
    pub mob: MobData,
    pub flap: f32,
    pub flap_speed: f32,
    pub o_flap_speed: f32,
    pub o_flap: f32,
    flapping: f32,
    next_flap: f32,
    party_parrot: bool,
    jukebox: Option<BlockPos>,
    ride_cooldown_counter: i32,
}

impl Parrot {
    pub fn new(ty: EntityType, id: EntityId, _registries: &RegistryAccess) -> Self {
        let mob = MobData::new(ty, id, MobKind::TamableAnimal, |b| {
            b.define(DATA_VARIANT_ID, ParrotVariant::RedBlue as i32);
        });
        let mut parrot = Parrot {
            mob,
            flap: 0.0,
            flap_speed: 0.0,
            o_flap_speed: 0.0,
            o_flap: 0.0,
            flapping: 1.0,
            next_flap: 1.0,
            party_parrot: false,
            jukebox: None,
            ride_cooldown_counter: 0,
        };
        let mut navigation = StandardPathNavigation::flying(&parrot);
        navigation.set_can_open_doors(false);
        navigation.set_can_float(true);
        parrot.mob.set_navigation(Box::new(navigation));
        parrot.register_goals();
        parrot.mob.move_control = Some(Box::new(FlyingMoveControl::new(10, false)));
        parrot
            .mob
            .set_pathfinding_malus(PathType::FireInNeighbor, -1.0);
        parrot.mob.set_pathfinding_malus(PathType::Fire, -1.0);
        parrot.mob.set_pathfinding_malus(PathType::Cocoa, -1.0);
        parrot
    }

    fn register_goals(&mut self) {
        let float = FloatGoal::new(self);
        let look_at = LookAtPlayerGoal::new_player(self, 8.0);
        let follow_mob = FollowMobGoal::new(self, 1.0, 3.0, 7.0);
        let goals = &mut self.mob.goal_selector;
        goals.add_goal(0, Box::new(TamableAnimalPanicGoal::new(1.25)));
        goals.add_goal(0, Box::new(float));
        goals.add_goal(1, Box::new(look_at));
        goals.add_goal(2, Box::new(SitWhenOrderedToGoal::new()));
        goals.add_goal(2, Box::new(FollowOwnerGoal::new(1.0, 5.0, 1.0)));
        goals.add_goal(2, Box::new(ParrotWanderGoal::new(1.0)));
        goals.add_goal(3, Box::new(LandOnOwnersShoulderGoal::new()));
        goals.add_goal(3, Box::new(follow_mob));
    }

    pub fn get_variant(&self) -> ParrotVariant {
        ParrotVariant::by_id(self.mob.living.entity.entity_data.get(DATA_VARIANT_ID))
    }

    pub fn set_variant(&mut self, variant: ParrotVariant) {
        self.mob
            .living
            .entity
            .entity_data
            .set(DATA_VARIANT_ID, variant as i32);
    }

    pub fn is_party_parrot(&self) -> bool {
        self.party_parrot
    }

    /// `isFlying()`.
    pub fn is_flying(&self) -> bool {
        !self.base().on_ground()
    }

    /// `calculateFlapping()`.
    fn calculate_flapping(&mut self) {
        self.o_flap = self.flap;
        self.o_flap_speed = self.flap_speed;
        self.flap_speed += (if !self.base().on_ground() && !self.base().is_passenger() {
            4.0f32
        } else {
            -1.0f32
        }) * 0.3;
        self.flap_speed = mth::clamp_f32(self.flap_speed, 0.0, 1.0);
        if !self.base().on_ground() && self.flapping < 1.0 {
            self.flapping = 1.0;
        }
        self.flapping *= 0.9;
        let movement = self.base().delta_movement();
        if !self.base().on_ground() && movement.y < 0.0 {
            self.base_mut()
                .set_delta_movement(movement.multiply_xyz(1.0, 0.6, 1.0));
        }
        self.flap += self.flapping * 2.0;
    }

    fn is_owned_by_player(&self, player: &ServerPlayer) -> bool {
        tamable::get_owner_reference(self) == Some(player.uuid())
    }
}

impl ShoulderRidingAccess for Parrot {
    fn can_sit_on_shoulder(&self) -> bool {
        self.ride_cooldown_counter > RIDE_COOLDOWN
    }

    fn set_entity_on_shoulder(&mut self, level: &mut ServerLevel, owner: EntityId) -> bool {
        let mut tag = CompoundTag::new();
        {
            let lookup = |id: EntityId| level.get_entity(id);
            self.save_without_id(&mut tag, &lookup);
        }
        tag.put_string("id", "minecraft:parrot");
        let placed = level
            .with_player(owner, |level, p| p.set_entity_on_shoulder(&*level, tag))
            .unwrap_or(false);
        if placed {
            self.discard();
            return true;
        }
        false
    }
}

impl Entity for Parrot {
    crate::mob_entity_accessors!(mob);

    fn get_encode_id(&self) -> Option<&'static str> {
        Some("minecraft:parrot")
    }

    fn as_shoulder_riding(&self) -> Option<&dyn ShoulderRidingAccess> {
        Some(self)
    }

    fn as_shoulder_riding_mut(&mut self) -> Option<&mut dyn ShoulderRidingAccess> {
        Some(self)
    }

    fn finalize_spawn(
        &mut self,
        level: &mut dyn ServerLevelAccessor,
        difficulty: DifficultyInstance,
        reason: EntitySpawnReason,
        group_data: Option<Box<dyn SpawnGroupData>>,
    ) -> Option<Box<dyn SpawnGroupData>> {
        let variant = ParrotVariant::ALL[level.random().next_int_bounded(5) as usize];
        self.set_variant(variant);
        let group_data =
            group_data.or_else(|| Some(Box::new(AgeableMobGroupData::with_baby(false))));
        mob::spawn::finalize_spawn(self, level, difficulty, reason, group_data)
    }

    fn can_be_a_baby(&self) -> bool {
        false
    }

    fn tick(&mut self, level: &mut ServerLevel) {
        self.ride_cooldown_counter += 1;
        mob::tick::tick(self, level);
    }

    fn ai_step(&mut self, level: &mut ServerLevel) {
        let valid_jukebox = self.jukebox.is_some_and(|j| {
            j.closer_to_center_than(&self.position(), 3.46)
                && level.get_block_state(j).block() == Block::JUKEBOX
        });
        if !valid_jukebox {
            self.party_parrot = false;
            self.jukebox = None;
        }
        if level.random().next_int_bounded(400) == 0 {
            imitate_nearby_mobs(level, self);
        }
        mob::tick::ai_step(self, level);
        self.calculate_flapping();
    }

    fn set_record_playing_nearby(&mut self, jukebox: BlockPos, is_playing: bool) {
        self.jukebox = Some(jukebox);
        self.party_parrot = is_playing;
    }

    fn mob_interact(
        &mut self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        hand: InteractionHand,
    ) -> InteractionResult {
        let item_stack = player.get_item_in_hand(hand);
        let tag_contains = |tag: &str, stack: &ItemStack| {
            mc_data::tags::get::<Item>(tag).is_some_and(|t| t.contains(stack.item()))
        };
        if !tamable::is_tame(self) && tag_contains("parrot_food", &item_stack) {
            interact::use_player_item(level, player, hand);
            if !self.base().is_silent() {
                let pitch = 1.0
                    + (self.base_mut().random.next_float() - self.base_mut().random.next_float())
                        * 0.2;
                let pos = self.position();
                level.play_sound_at(
                    None,
                    pos.x,
                    pos.y,
                    pos.z,
                    SoundEvent::ENTITY_PARROT_EAT,
                    self.get_sound_source(),
                    1.0,
                    pitch,
                );
            }
            if self.base_mut().random.next_int_bounded(10) == 0 {
                tamable::tame(self, level, player);
                broadcast_entity_event(level, self.id(), 7);
            } else {
                broadcast_entity_event(level, self.id(), 6);
            }
            InteractionResult::SUCCESS
        } else if !tag_contains("parrot_poisonous_food", &item_stack) {
            if !self.is_flying() && tamable::is_tame(self) && self.is_owned_by_player(player) {
                let ordered = tamable::is_ordered_to_sit(self);
                tamable::set_ordered_to_sit(self, !ordered);
                InteractionResult::SUCCESS
            } else {
                interact::mob_interact(self, level, player, hand)
            }
        } else {
            interact::use_player_item(level, player, hand);
            self.add_effect(
                level,
                MobEffectInstance::new(MobEffect::POISON, 900, 0, false, true, true, None),
                None,
            );
            if player.is_creative() || !self.is_invulnerable() {
                let source = level.damage_sources().player_attack(&*player);
                self.hurt_server(level, &source, f32::MAX);
            }
            InteractionResult::SUCCESS
        }
    }

    fn is_food(&self, _stack: &ItemStack) -> bool {
        false
    }

    fn check_fall_damage(
        &mut self,
        _level: &mut ServerLevel,
        _ya: f64,
        _on_ground: bool,
        _on_state: mc_data::block::BlockState,
        _pos: BlockPos,
    ) {
    }

    fn can_mate(&self, _partner: &dyn Entity) -> bool {
        false
    }

    fn get_breed_offspring(
        &mut self,
        _level: &mut ServerLevel,
        _partner: EntityId,
    ) -> Option<Box<dyn Entity>> {
        None
    }

    fn get_ambient_sound_in(&mut self, level: &mut ServerLevel) -> Option<SoundEvent> {
        let difficulty = level.difficulty();
        get_ambient(difficulty, level.random())
    }

    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_PARROT_HURT)
    }

    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_PARROT_DEATH)
    }

    fn play_step_sound(
        &mut self,
        level: &mut ServerLevel,
        _pos: BlockPos,
        _state: mc_data::block::BlockState,
    ) {
        self.play_sound(level, SoundEvent::ENTITY_PARROT_STEP, 0.15, 1.0);
    }

    fn is_flapping(&self) -> bool {
        self.base().fly_dist > self.next_flap
    }

    fn on_flap_in(&mut self, level: &mut ServerLevel) {
        self.play_sound(level, SoundEvent::ENTITY_PARROT_FLY, 0.15, 1.0);
        self.next_flap = self.base().fly_dist + self.flap_speed / 2.0;
    }

    fn get_voice_pitch(&mut self) -> f32 {
        get_pitch(&mut self.base_mut().random)
    }

    fn get_sound_source(&self) -> SoundSource {
        SoundSource::Neutral
    }

    fn is_pushable(&self, _level: &ServerLevel) -> bool {
        true
    }

    fn do_push(&mut self, level: &mut ServerLevel, other: &mut dyn Entity) {
        if !other.is_player() {
            crate::world::entity::living_entity::push::do_push(level, self, other);
        }
    }

    fn hurt_server(&mut self, level: &mut ServerLevel, source: &DamageSource, damage: f32) -> bool {
        if self.is_invulnerable_to(level, source) {
            return false;
        }
        tamable::set_ordered_to_sit(self, false);
        crate::world::entity::living_entity::damage::hurt_server(self, level, source, damage)
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        mob::nbt::add_additional_save_data(self, out);
        out.put_int("Variant", self.get_variant() as i32);
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        mob::nbt::read_additional_save_data(self, input);
        let variant = if input.contains("Variant") {
            ParrotVariant::by_id(input.get_int_or("Variant", 0))
        } else {
            ParrotVariant::RedBlue
        };
        self.set_variant(variant);
    }

    fn omnidirectional_air_mover(&self) -> bool {
        true
    }

    fn can_fly_to_owner(&self) -> bool {
        true
    }

    fn get_leash_offset(&self) -> Vec3 {
        Vec3::new(
            0.0,
            f64::from(0.5f32 * self.base().eye_height),
            f64::from(self.base().get_bb_width() * 0.4),
        )
    }
}

/// `Parrot.checkParrotSpawnRules`.
fn check_parrot_spawn_rules(
    _ty: EntityType,
    level: &dyn ServerLevelAccessor,
    _reason: EntitySpawnReason,
    pos: BlockPos,
    _random: &mut dyn RandomSource,
) -> bool {
    level
        .get_block_state(pos.below())
        .is_tag("parrots_spawnable_on")
        && spawn_predicates::is_bright_enough_to_spawn(level, pos)
}

crate::register_spawn_placement!(EntityType::PARROT, check_parrot_spawn_rules);

#[distributed_slice(ENTITY_FACTORIES)]
static PARROT: EntityRegistration = EntityRegistration {
    entity_type: EntityType::PARROT,
    create: |ty, id, registries| Box::new(Parrot::new(ty, id, registries)),
};
