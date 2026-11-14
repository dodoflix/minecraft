//! `Pig`: saddle steering with a carrot on a stick (`ItemSteerable`), variants, zombification by lightning.

use super::pig_sound_variant::{PigSoundSet, PigSoundVariant};
use super::pig_variant;
use linkme::distributed_slice;
use mc_core::ident::Identifier;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::enums::{EntitySpawnReason, Pose};
use mc_data::{Attribute, EntityType, Item, SoundEvent};
use mc_game_core::define_accessor;
use mc_game_core::registry::RegistryAccess;
use mc_game_core::registry::keys;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::damagesource::DamageSource;
use mc_game_core::world::difficulty_instance::DifficultyInstance;
use mc_game_core::world::entity::ai::goal::{
    BreedGoal, FloatGoal, FollowParentGoal, LookAtPlayerGoal, PanicGoal, RandomLookAroundGoal,
    TemptGoal, WaterAvoidingRandomStrollGoal,
};
use mc_game_core::world::entity::ai::navigation::StandardPathNavigation;
use mc_game_core::world::entity::dimensions::{EntityAttachments, EntityDimensions};
use mc_game_core::world::entity::equipment::EquipmentSlot;
use mc_game_core::world::entity::factory::{
    ENTITY_FACTORIES, EntityRegistration, EntitySpawnRequest,
};
use mc_game_core::world::entity::item_based_steering::ItemBasedSteering;
use mc_game_core::world::entity::mob::convert::{self, ConversionParams};
use mc_game_core::world::entity::mob::{self, MobData, MobKind};
use mc_game_core::world::entity::spawn_group_data::SpawnGroupData;
use mc_game_core::world::entity::variant::{self, SpawnContext};
use mc_game_core::world::entity::{Entity, EntityId, ServerPlayer};
use mc_game_core::world::item::{InteractionHand, InteractionResult, ItemStack};
use mc_game_core::world::level::server_level_accessor::ServerLevelAccessor;
use mc_nbt::CompoundTag;
use mc_protocol::components::{DataComponentMap, Holder, PIG_SOUND_VARIANT, PIG_VARIANT};
use mc_world::level_storage::Difficulty;
use std::sync::Arc;

define_accessor!(pub DATA_BOOST_TIME: Int = Pig);
define_accessor!(pub DATA_VARIANT_ID: PigVariant = Pig);
define_accessor!(pub DATA_SOUND_VARIANT_ID: PigSoundVariant = Pig);

/// `BABY_DIMENSIONS`.
fn baby_dimensions() -> EntityDimensions {
    EntityDimensions::scalable(0.45, 0.45)
        .with_eye_height(0.40625)
        .with_attachments(
            EntityAttachments::create_default(0.45, 0.45).with_passenger(Vec3::new(0.0, 0.5, 0.0)),
        )
}

/// `ItemTags.PIG_FOOD`.
fn is_pig_food(stack: &ItemStack) -> bool {
    mc_data::tags::get::<Item>("pig_food").is_some_and(|t| t.contains(stack.item()))
}

/// `Pig`.
pub struct Pig {
    pub mob: MobData,
    steering: ItemBasedSteering,
    registries: RegistryAccess,
    variant_key: Identifier,
    sound_variant_key: Identifier,
    sound_variant: PigSoundVariant,
    /// `getControllingPassenger()`: the saddled pig's first passenger when it is a player holding the carrot on a stick
    /// (refreshed every tick and on boarding: the level is not available to the getter).
    controller: Option<EntityId>,
    first_passenger_is_player: bool,
}

impl Pig {
    /// `new Pig(type, level)`.
    pub fn new(ty: EntityType, id: EntityId, registries: &RegistryAccess) -> Self {
        let default_variant = variant::get_default_or_any(
            registries,
            keys::PIG_VARIANT,
            &Identifier::parse(pig_variant::TEMPERATE).expect("valid id"),
        );
        let sound_registry = registries.registry_or_throw(keys::PIG_SOUND_VARIANT);
        let default_sound = sound_registry
            .id_of(&Identifier::parse("minecraft:classic").expect("valid id"))
            .unwrap_or(0);
        let mob = MobData::new(ty, id, MobKind::Animal, |b| {
            b.define(DATA_BOOST_TIME, 0);
            b.define(DATA_VARIANT_ID, default_variant as i32);
            b.define(DATA_SOUND_VARIANT_ID, default_sound as i32);
        });
        let variant_key = registries
            .registry_or_throw(keys::PIG_VARIANT)
            .name_of(default_variant)
            .expect("variant entry");
        let sound_variant_key = sound_registry
            .name_of(default_sound)
            .expect("sound variant entry");
        let sound_variant = *registries
            .get::<PigSoundVariant>(keys::PIG_SOUND_VARIANT, &sound_variant_key)
            .expect("typed pig sound variant");
        let mut pig = Pig {
            mob,
            steering: ItemBasedSteering::new(DATA_BOOST_TIME),
            registries: registries.clone(),
            variant_key,
            sound_variant_key,
            sound_variant,
            controller: None,
            first_passenger_is_player: false,
        };
        let navigation = StandardPathNavigation::ground(&pig);
        pig.mob.set_navigation(Box::new(navigation));
        pig.register_goals();
        pig
    }

    /// `registerGoals()`.
    fn register_goals(&mut self) {
        let float = FloatGoal::new(self);
        let breed = BreedGoal::new(self, 1.0);
        let follow_parent = FollowParentGoal::new(self, 1.1);
        let look_at = LookAtPlayerGoal::new_player(self, 6.0);
        let goals = &mut self.mob.goal_selector;
        goals.add_goal(0, Box::new(float));
        goals.add_goal(1, Box::new(PanicGoal::new(1.25)));
        goals.add_goal(3, Box::new(breed));
        goals.add_goal(
            4,
            Box::new(TemptGoal::new(
                1.2,
                Arc::new(|i: &ItemStack| i.is(Item::CARROT_ON_A_STICK)),
                false,
            )),
        );
        goals.add_goal(
            4,
            Box::new(TemptGoal::new(
                1.2,
                Arc::new(|i: &ItemStack| is_pig_food(i)),
                false,
            )),
        );
        goals.add_goal(5, Box::new(follow_parent));
        goals.add_goal(6, Box::new(WaterAvoidingRandomStrollGoal::new(1.0)));
        goals.add_goal(7, Box::new(look_at));
        goals.add_goal(8, Box::new(RandomLookAroundGoal::new()));
    }

    fn get_sound_set(&self) -> &PigSoundSet {
        if self.is_baby() {
            &self.sound_variant.baby_sounds
        } else {
            &self.sound_variant.adult_sounds
        }
    }

    /// `setVariant(variant)` by registry id.
    pub fn set_variant(&mut self, id: u32) {
        self.mob
            .living
            .entity
            .entity_data
            .set(DATA_VARIANT_ID, id as i32);
        if let Some(name) = self
            .registries
            .registry(keys::PIG_VARIANT)
            .and_then(|r| r.name_of(id))
        {
            self.variant_key = name;
        }
    }

    /// `getVariant()` (registry id).
    pub fn get_variant(&self) -> i32 {
        self.mob.living.entity.entity_data.get(DATA_VARIANT_ID)
    }

    /// `setSoundVariant(soundVariant)` by registry id.
    fn set_sound_variant(&mut self, id: u32) {
        self.mob
            .living
            .entity
            .entity_data
            .set(DATA_SOUND_VARIANT_ID, id as i32);
        if let Some(name) = self
            .registries
            .registry(keys::PIG_SOUND_VARIANT)
            .and_then(|r| r.name_of(id))
        {
            if let Some(v) = self
                .registries
                .get::<PigSoundVariant>(keys::PIG_SOUND_VARIANT, &name)
            {
                self.sound_variant = *v;
            }
            self.sound_variant_key = name;
        }
    }

    /// `isSaddled()`.
    pub fn is_saddled(&self) -> bool {
        !self.get_item_by_slot(EquipmentSlot::Saddle).is_empty()
    }

    fn refresh_controller(&mut self, level: &ServerLevel) {
        let first = self.base().passengers.first().copied();
        self.controller = first
            .filter(|_| self.is_saddled())
            .and_then(|id| level.get_entity(id))
            .filter(|p| {
                p.is_player()
                    && (p
                        .get_item_by_slot(EquipmentSlot::MainHand)
                        .is(Item::CARROT_ON_A_STICK)
                        || p.get_item_by_slot(EquipmentSlot::OffHand)
                            .is(Item::CARROT_ON_A_STICK))
            })
            .map(|p| p.id());
    }
}

impl Entity for Pig {
    mc_game_core::mob_entity_accessors!(mob);

    fn get_encode_id(&self) -> Option<&'static str> {
        Some("minecraft:pig")
    }

    fn get_dimensions(&self, _pose: Pose) -> EntityDimensions {
        if self.is_baby() {
            baby_dimensions()
        } else {
            EntityDimensions::of_type(self.mob.living.entity.entity_type)
        }
    }

    fn tick(&mut self, level: &mut ServerLevel) {
        self.refresh_controller(level);
        mob::tick::tick(self, level);
    }

    fn get_controlling_passenger(&self) -> Option<EntityId> {
        self.controller
            .or_else(|| mc_game_core::world::entity::mob::controlling_passenger(self))
    }

    fn add_passenger(&mut self, passenger: &dyn Entity, first_passenger_is_player: bool) {
        let id = passenger.id();
        let is_player = passenger.is_player();
        let passengers = &mut self.mob.living.entity.passengers;
        if passengers.is_empty() || !(is_player && !first_passenger_is_player) {
            passengers.push(id);
        } else {
            passengers.insert(0, id);
        }
        self.first_passenger_is_player = self.base().passengers.first() == Some(&id) && is_player;
        if self.first_passenger_is_player && self.is_saddled() {
            let holds = passenger
                .get_item_by_slot(EquipmentSlot::MainHand)
                .is(Item::CARROT_ON_A_STICK)
                || passenger
                    .get_item_by_slot(EquipmentSlot::OffHand)
                    .is(Item::CARROT_ON_A_STICK);
            self.controller = holds.then_some(id);
        }
    }

    fn on_passenger_removed(&mut self, level: &mut ServerLevel, _passenger: EntityId) {
        self.refresh_controller(level);
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

    fn play_eating_sound(&mut self, level: &mut ServerLevel) {
        let sound = self.get_sound_set().eat_sound;
        mc_game_core::world::entity::living_entity::damage::make_sound(self, level, sound);
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

    fn mob_interact(
        &mut self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        hand: InteractionHand,
    ) -> InteractionResult {
        let has_food = self.is_food(&player.get_item_in_hand(hand));
        if !has_food
            && self.is_saddled()
            && !self.base().is_vehicle()
            && !player.is_shift_key_down()
        {
            level.start_riding_held(player, self, false, true);
            return InteractionResult::SUCCESS;
        }
        let result = mob::interact::mob_interact(self, level, player, hand);
        if !result.consumes_action() {
            let item_stack = player.get_item_in_hand(hand);
            if self.is_equippable_in_slot(&item_stack, EquipmentSlot::Saddle) {
                mob::interact::with_hand_stack(player, hand, |s, p| {
                    mc_game_core::world::item::interact_living_entity(
                        level,
                        s,
                        p,
                        self.as_dyn_mut(),
                        hand,
                    )
                })
            } else {
                InteractionResult::PASS
            }
        } else {
            result
        }
    }

    fn can_dispenser_equip_into_slot(&self, slot: EquipmentSlot) -> bool {
        slot == EquipmentSlot::Saddle
            || mob::equipment::can_dispenser_equip_into_slot(self.as_dyn())
    }

    fn can_use_slot(&self, slot: EquipmentSlot) -> bool {
        if slot != EquipmentSlot::Saddle {
            true
        } else {
            self.is_alive() && !self.is_baby()
        }
    }

    fn get_equip_sound_override(&self, slot: EquipmentSlot) -> Option<SoundEvent> {
        (slot == EquipmentSlot::Saddle).then_some(SoundEvent::ENTITY_PIG_SADDLE)
    }

    fn thunder_hit(&mut self, level: &mut ServerLevel) {
        if level.difficulty() != Difficulty::Peaceful {
            let params = ConversionParams::single(self, level, false, true);
            let converted = convert::convert_to(
                self,
                level,
                EntityType::ZOMBIFIED_PIGLIN,
                &params,
                EntitySpawnReason::Conversion,
                |_, _| {},
            );
            if let Some(id) = converted {
                let difficulty =
                    ServerLevelAccessor::get_current_difficulty_at(&*level, self.block_position());
                let random = &mut self.mob.living.entity.random;
                level.with_entity(id, |_, zp| {
                    mob::equipment::populate_default_equipment_slots(zp, random, &difficulty);
                    if let Some(m) = zp.mob_mut() {
                        m.persistence_required = true;
                    }
                });
                return;
            }
        }
        let t = self.get_remaining_fire_ticks().wrapping_add(1);
        self.set_remaining_fire_ticks(t);
        if self.get_remaining_fire_ticks() == 0 {
            self.ignite_for_seconds(8.0);
        }
        let source = level.damage_sources().lightning_bolt();
        self.hurt_server(level, &source, 5.0);
    }

    fn tick_ridden(&mut self, level: &mut ServerLevel, controller: EntityId, _ridden_input: Vec3) {
        let Some((y_rot, x_rot)) = level
            .get_entity(controller)
            .map(|c| (c.base().y_rot(), c.base().x_rot()))
        else {
            return;
        };
        self.set_rot(y_rot, x_rot * 0.5);
        let y = self.base().y_rot();
        self.base_mut().y_rot_o = y;
        self.mob.living.y_body_rot = y;
        self.set_y_head_rot(y);
        self.steering
            .tick_boost(&self.mob.living.entity.entity_data);
    }

    fn get_ridden_input(
        &self,
        _level: &ServerLevel,
        _controller: EntityId,
        _self_input: Vec3,
    ) -> Vec3 {
        Vec3::new(0.0, 0.0, 1.0)
    }

    fn get_ridden_speed(&self, _level: &ServerLevel, _controller: EntityId) -> f32 {
        let speed = mob::attribute_value(self, Attribute::MOVEMENT_SPEED);
        (speed
            * 0.225
            * f64::from(
                self.steering
                    .boost_factor(&self.mob.living.entity.entity_data),
            )) as f32
    }

    fn boost(&mut self) -> bool {
        let data = &mut self.mob.living.entity.entity_data;
        self.steering
            .boost(data, &mut self.mob.living.entity.random)
    }

    fn get_breed_offspring(
        &mut self,
        level: &mut ServerLevel,
        partner: EntityId,
    ) -> Option<Box<dyn Entity>> {
        let mut baby = mc_game_core::world::entity::factory::create(
            EntityType::PIG,
            level,
            EntitySpawnRequest {
                reason: EntitySpawnReason::Breeding,
                ignore_checks: false,
            },
        )?;
        let own = self.get_variant();
        let partner_variant = level
            .get_entity(partner)
            .and_then(|p| p.as_any().downcast_ref::<Pig>())
            .map(Pig::get_variant);
        if let (Some(partner_variant), Some(pig)) =
            (partner_variant, baby.as_any_mut().downcast_mut::<Pig>())
        {
            let pick = if self.base_mut().random.next_boolean() {
                own
            } else {
                partner_variant
            };
            pig.set_variant(pick as u32);
        }
        Some(baby)
    }

    fn is_food(&self, stack: &ItemStack) -> bool {
        is_pig_food(stack)
    }

    fn finalize_spawn(
        &mut self,
        level: &mut dyn ServerLevelAccessor,
        difficulty: DifficultyInstance,
        reason: EntitySpawnReason,
        group_data: Option<Box<dyn SpawnGroupData>>,
    ) -> Option<Box<dyn SpawnGroupData>> {
        let pos = self.block_position();
        let mut selected = None;
        level.with_random_taken(&mut |lvl, random| {
            let context = SpawnContext::create(lvl, pos);
            selected = variant::select_variant_to_spawn::<pig_variant::PigVariant>(
                &context,
                keys::PIG_VARIANT,
                random,
            );
            true
        });
        if let Some(id) = selected.and_then(|key| {
            self.registries
                .registry(keys::PIG_VARIANT)
                .and_then(|r| r.id_of(&key))
        }) {
            self.set_variant(id);
        }
        let registries = self.registries.clone();
        if let Some(key) =
            variant::pick_random_entry(&registries, keys::PIG_SOUND_VARIANT, level.random())
            && let Some(id) = registries
                .registry(keys::PIG_SOUND_VARIANT)
                .and_then(|r| r.id_of(&key))
        {
            self.set_sound_variant(id);
        }
        mob::spawn::finalize_spawn(self, level, difficulty, reason, group_data)
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        mob::nbt::add_additional_save_data(self, out);
        variant::write_variant(out, &self.variant_key);
        out.put_string("sound_variant", self.sound_variant_key.to_string());
    }

    fn read_additional_save_data(&mut self, level: Option<&ServerLevel>, input: &CompoundTag) {
        mob::nbt::read_additional_save_data(self, level, input);
        if let Some(id) = variant::read_variant_key(input).and_then(|k| {
            self.registries
                .registry(keys::PIG_VARIANT)
                .and_then(|r| r.id_of(&k))
        }) {
            self.set_variant(id);
        }
        if let Some(id) = input
            .get_string("sound_variant")
            .and_then(Identifier::try_parse)
            .and_then(|k| {
                self.registries
                    .registry(keys::PIG_SOUND_VARIANT)
                    .and_then(|r| r.id_of(&k))
            })
        {
            self.set_sound_variant(id);
        }
    }

    fn entity_components(&self) -> DataComponentMap {
        let base = mc_game_core::advancements::predicates::entity::support::base_entity_components(
            self.base(),
        );
        DataComponentMap::builder()
            .add_all(&base)
            .set(PIG_VARIANT, Holder::Reference(self.variant_key.clone()))
            .set(
                PIG_SOUND_VARIANT,
                Holder::Reference(self.sound_variant_key.clone()),
            )
            .build()
    }

    fn apply_implicit_components(&mut self, stack: &ItemStack) {
        if let Some(Holder::Reference(key)) = stack.get(&PIG_VARIANT)
            && let Some(id) = self
                .registries
                .registry(keys::PIG_VARIANT)
                .and_then(|r| r.id_of(key))
        {
            self.set_variant(id);
        }
        if let Some(Holder::Reference(key)) = stack.get(&PIG_SOUND_VARIANT)
            && let Some(id) = self
                .registries
                .registry(keys::PIG_SOUND_VARIANT)
                .and_then(|r| r.id_of(key))
        {
            self.set_sound_variant(id);
        }
        mc_game_core::world::entity::spawn_config::apply_base_implicit_components(self, stack);
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static PIG: EntityRegistration = EntityRegistration {
    entity_type: EntityType::PIG,
    create: |ty, id, registries| Box::new(Pig::new(ty, id, registries)),
};
