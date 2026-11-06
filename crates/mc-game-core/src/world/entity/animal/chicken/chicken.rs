//! `Chicken`: the proof mob of the mob spine (animal breeding, goals, controls, NBT, variants, egg laying).

use super::chicken_sound_variant::{self, ChickenSoundSet, ChickenSoundVariant};
use super::chicken_variant;
use crate::define_accessor;
use crate::registry::RegistryAccess;
use crate::registry::keys;
use crate::server::level::ServerLevel;
use crate::world::damagesource::DamageSource;
use crate::world::difficulty_instance::DifficultyInstance;
use crate::world::entity::ai::goal::{
    BreedGoal, FloatGoal, FollowParentGoal, LookAtPlayerGoal, PanicGoal, RandomLookAroundGoal,
    TemptGoal, WaterAvoidingRandomStrollGoal,
};
use crate::world::entity::ai::navigation::StandardPathNavigation;
use crate::world::entity::dimensions::{EntityAttachments, EntityDimensions};
use crate::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration, EntitySpawnRequest};
use crate::world::entity::mob::{self, MobData, MobKind};
use crate::world::entity::spawn_group_data::SpawnGroupData;
use crate::world::entity::variant::{self, SpawnContext};
use crate::world::entity::{Entity, EntityId};
use crate::world::item::ItemStack;
use crate::world::level::pathfinder::PathType;
use crate::world::level::server_level_accessor::ServerLevelAccessor;
use linkme::distributed_slice;
use mc_core::ident::Identifier;
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::enums::{EntitySpawnReason, Pose};
use mc_data::{EntityType, GameEvent, Item, SoundEvent};
use mc_nbt::CompoundTag;
use std::sync::Arc;

define_accessor!(pub DATA_VARIANT_ID: ChickenVariant = Chicken);
define_accessor!(pub DATA_SOUND_VARIANT_ID: ChickenSoundVariant = Chicken);

/// `Chicken`.
pub struct Chicken {
    pub mob: MobData,
    pub flap: f32,
    pub flap_speed: f32,
    pub o_flap_speed: f32,
    pub o_flap: f32,
    pub flapping: f32,
    next_flap: f32,
    pub egg_time: i32,
    pub is_chicken_jockey: bool,
    /// Identifier of the variant / sound variant entry (what `VariantUtils.writeVariant` saves) and the sound set data
    /// cached when the sound variant was set (no registry access while sounds play).
    variant_key: Identifier,
    sound_variant_key: Identifier,
    sound_variant: ChickenSoundVariant,
    /// `variant` / `sound_variant` read from NBT, resolved to registry ids by [`Entity::resolve_registry_refs`].
    pending_variant: Option<Identifier>,
    pending_sound_variant: Option<Identifier>,
}

/// `BABY_DIMENSIONS`.
fn baby_dimensions() -> EntityDimensions {
    let dims = EntityDimensions::scalable(0.3, 0.4).with_eye_height(0.28125);
    let attachments =
        EntityAttachments::create_default(0.3, 0.4).with_passenger(Vec3::new(0.0, 0.375, 0.0));
    dims.with_attachments(attachments)
}

fn id_of(registries: &RegistryAccess, registry: &str, key: &Identifier) -> Option<i32> {
    registries.registry(registry)?.id_of(key).map(|i| i as i32)
}

impl Chicken {
    /// `new Chicken(type, level)` (`registerGoals` and `createNavigation` run as in `Mob.<init>`).
    pub fn new(ty: EntityType, id: EntityId, registries: &RegistryAccess) -> Self {
        let default_variant = variant::get_default_or_any(
            registries,
            keys::CHICKEN_VARIANT,
            &Identifier::parse(chicken_variant::TEMPERATE).expect("valid id"),
        );
        let sound_registry = registries.registry_or_throw(keys::CHICKEN_SOUND_VARIANT);
        let default_sound = sound_registry
            .id_of(&Identifier::parse(chicken_sound_variant::CLASSIC).expect("valid id"))
            .unwrap_or(0);
        let mob = MobData::new(ty, id, MobKind::Animal, |b| {
            b.define(DATA_VARIANT_ID, default_variant as i32);
            b.define(DATA_SOUND_VARIANT_ID, default_sound as i32);
        });
        let variant_key = registries
            .registry_or_throw(keys::CHICKEN_VARIANT)
            .name_of(default_variant)
            .expect("variant entry");
        let sound_variant_key = sound_registry
            .name_of(default_sound)
            .expect("sound variant entry");
        let sound_variant = *registries
            .get::<ChickenSoundVariant>(keys::CHICKEN_SOUND_VARIANT, &sound_variant_key)
            .expect("typed chicken sound variant");
        let mut chicken = Chicken {
            mob,
            flap: 0.0,
            flap_speed: 0.0,
            o_flap_speed: 0.0,
            o_flap: 0.0,
            flapping: 1.0,
            next_flap: 1.0,
            egg_time: 0,
            is_chicken_jockey: false,
            variant_key,
            sound_variant_key,
            sound_variant,
            pending_variant: None,
            pending_sound_variant: None,
        };
        let navigation = StandardPathNavigation::ground(&chicken);
        chicken.mob.set_navigation(Box::new(navigation));
        chicken.register_goals();
        chicken.egg_time = chicken.mob.living.entity.random.next_int_bounded(6000) + 6000;
        chicken.mob.set_pathfinding_malus(PathType::Water, 0.0);
        chicken
    }

    /// `registerGoals()`.
    fn register_goals(&mut self) {
        let float = FloatGoal::new(self);
        let breed = BreedGoal::new(self, 1.0);
        let follow_parent = FollowParentGoal::new(self, 1.1);
        let look_at = LookAtPlayerGoal::new_player(self, 6.0);
        let goals = &mut self.mob.goal_selector;
        goals.add_goal(0, Box::new(float));
        goals.add_goal(1, Box::new(PanicGoal::new(1.4)));
        goals.add_goal(2, Box::new(breed));
        goals.add_goal(
            3,
            Box::new(TemptGoal::new(
                1.0,
                Arc::new(|stack: &ItemStack| is_chicken_food(stack)),
                false,
            )),
        );
        goals.add_goal(4, Box::new(follow_parent));
        goals.add_goal(5, Box::new(WaterAvoidingRandomStrollGoal::new(1.0)));
        goals.add_goal(6, Box::new(look_at));
        goals.add_goal(7, Box::new(RandomLookAroundGoal::new()));
    }

    fn get_sound_set(&self) -> &ChickenSoundSet {
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
            .registry(keys::CHICKEN_VARIANT)
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
    fn set_sound_variant(&mut self, registries: &RegistryAccess, id: u32) {
        self.mob
            .living
            .entity
            .entity_data
            .set(DATA_SOUND_VARIANT_ID, id as i32);
        if let Some(name) = registries
            .registry(keys::CHICKEN_SOUND_VARIANT)
            .and_then(|r| r.name_of(id))
        {
            if let Some(v) =
                registries.get::<ChickenSoundVariant>(keys::CHICKEN_SOUND_VARIANT, &name)
            {
                self.sound_variant = *v;
            }
            self.sound_variant_key = name;
        }
    }

    pub fn is_chicken_jockey(&self) -> bool {
        self.is_chicken_jockey
    }

    pub fn set_chicken_jockey(&mut self, is_chicken_jockey: bool) {
        self.is_chicken_jockey = is_chicken_jockey;
    }
}

/// `ItemTags.CHICKEN_FOOD`.
fn is_chicken_food(stack: &ItemStack) -> bool {
    mc_data::tags::get::<Item>("chicken_food").is_some_and(|t| t.contains(stack.item()))
}

impl Entity for Chicken {
    crate::mob_entity_accessors!(mob);

    fn get_encode_id(&self) -> Option<&'static str> {
        Some("minecraft:chicken")
    }

    fn get_dimensions(&self, _pose: Pose) -> EntityDimensions {
        if self.is_baby() {
            baby_dimensions()
        } else {
            { EntityDimensions::of_type(self.mob.living.entity.entity_type) }
        }
    }

    fn ai_step(&mut self, level: &mut ServerLevel) {
        mob::tick::ai_step(self, level);
        self.o_flap = self.flap;
        self.o_flap_speed = self.flap_speed;
        self.flap_speed += (if self.base().on_ground() {
            -1.0f32
        } else {
            4.0f32
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
        if self.is_alive() && !self.is_baby() && !self.is_chicken_jockey() {
            self.egg_time -= 1;
            if self.egg_time <= 0 {
                let laid = mob::ext::drop_from_gift_loot_table(
                    level,
                    self,
                    &Identifier::parse("minecraft:gameplay/chicken_lay").expect("valid id"),
                    |level, e, stack| {
                        crate::world::entity::item::spawn_at_location(level, e, stack, Vec3::ZERO);
                    },
                );
                if laid {
                    let pitch = (self.base_mut().random.next_float()
                        - self.base_mut().random.next_float())
                        * 0.2
                        + 1.0;
                    self.play_sound(level, SoundEvent::ENTITY_CHICKEN_EGG, 1.0, pitch);
                    self.game_event(level, GameEvent::ENTITY_PLACE);
                }
                self.egg_time = self.base_mut().random.next_int_bounded(6000) + 6000;
            }
        }
    }

    fn is_flapping(&self) -> bool {
        self.base().fly_dist > self.next_flap
    }

    fn on_flap(&mut self) {
        self.next_flap = self.base().fly_dist + self.flap_speed / 2.0;
    }

    fn get_ambient_sound(&self) -> Option<SoundEvent> {
        Some(self.get_sound_set().ambient_sound)
    }

    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        Some(self.get_sound_set().hurt_sound)
    }

    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(self.get_sound_set().death_sound)
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

    fn get_breed_offspring(
        &mut self,
        level: &mut ServerLevel,
        partner: EntityId,
    ) -> Option<Box<dyn Entity>> {
        let mut baby = crate::world::entity::factory::create(
            EntityType::CHICKEN,
            level,
            EntitySpawnRequest {
                reason: EntitySpawnReason::Breeding,
                ignore_checks: false,
            },
        )?;
        let own_variant = self.get_variant();
        let partner_variant = level
            .get_entity(partner)
            .and_then(|p| p.as_any().downcast_ref::<Chicken>())
            .map(|p| p.get_variant());
        if let (Some(partner_variant), Some(chicken)) =
            (partner_variant, baby.as_any_mut().downcast_mut::<Chicken>())
        {
            let pick = if self.base_mut().random.next_boolean() {
                own_variant
            } else {
                partner_variant
            };
            let registries = level.registries.clone();
            chicken.set_variant(&registries, pick as u32);
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
        let selected = {
            let pos = self.block_position();
            let context = SpawnContext::create(&*level, pos);
            let entries: Vec<(u32, _)> = registries
                .registry_or_throw(keys::CHICKEN_VARIANT)
                .entries()
                .iter()
                .enumerate()
                .filter_map(|(i, _)| {
                    registries
                        .registry(keys::CHICKEN_VARIANT)
                        .and_then(|r| r.get_by_id::<chicken_variant::ChickenVariant>(i as u32))
                        .map(|v| (i as u32, v.spawn_conditions.clone()))
                })
                .collect();
            let winners = variant::select(entries.into_iter(), &context);
            variant::get_random_safe(&winners, level.random())
        };
        if let Some(id) = selected {
            self.set_variant(&registries, id);
        }
        let sound_count = registries
            .registry_or_throw(keys::CHICKEN_SOUND_VARIANT)
            .len() as u32;
        if sound_count > 0 {
            let picked = level.random().next_int_bounded(sound_count as i32) as u32;
            self.set_sound_variant(&registries, picked);
        }
        mob::spawn::finalize_spawn(self, level, difficulty, reason, group_data)
    }

    fn is_food(&self, stack: &ItemStack) -> bool {
        is_chicken_food(stack)
    }

    fn get_base_experience_reward(&mut self, _level: &ServerLevel) -> i32 {
        if self.is_chicken_jockey() {
            10
        } else {
            mob::animal::get_base_experience_reward(self)
        }
    }

    fn remove_when_far_away(&self, _dist_sqr: f64) -> bool {
        self.is_chicken_jockey()
    }

    fn position_rider_with(
        &self,
        passenger: &mut dyn Entity,
        move_function: crate::world::entity::entity::MoveFunction,
    ) {
        let position = self.get_passenger_riding_position(passenger.as_dyn());
        let offset = passenger.get_vehicle_attachment_point(self.as_dyn());
        move_function.apply(
            passenger,
            position.x - offset.x,
            position.y - offset.y,
            position.z - offset.z,
        );
        if let Some(l) = passenger.living_mut() {
            l.y_body_rot = self.mob.living.y_body_rot;
        }
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        mob::nbt::add_additional_save_data(self, out);
        out.put_boolean("IsChickenJockey", self.is_chicken_jockey);
        out.put_int("EggLayTime", self.egg_time);
        variant::write_variant(out, &self.variant_key);
        out.put_string("sound_variant", &self.sound_variant_key.to_string());
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        mob::nbt::read_additional_save_data(self, input);
        self.is_chicken_jockey = input.get_boolean_or("IsChickenJockey", false);
        if input.contains("EggLayTime") {
            self.egg_time = input.get_int_or("EggLayTime", 0);
        }
        self.pending_variant = variant::read_variant_key(input);
        self.pending_sound_variant = input
            .get_string("sound_variant")
            .and_then(Identifier::try_parse);
    }

    fn resolve_registry_refs(&mut self, registries: &RegistryAccess) {
        if let Some(key) = self.pending_variant.take() {
            if let Some(id) = id_of(registries, keys::CHICKEN_VARIANT, &key) {
                self.set_variant(registries, id as u32);
            }
        }
        if let Some(key) = self.pending_sound_variant.take() {
            if let Some(id) = id_of(registries, keys::CHICKEN_SOUND_VARIANT, &key) {
                self.set_sound_variant(registries, id as u32);
            }
        }
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static CHICKEN: EntityRegistration = EntityRegistration {
    entity_type: EntityType::CHICKEN,
    create: |ty, id, registries| Box::new(Chicken::new(ty, id, registries)),
};
