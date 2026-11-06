//! `Llama` and `TraderLlama` (one Rust type; `trader` is `Some` for the trader llama): strength, variants, caravans, spit.

use super::abstract_chested_horse::{self, AbstractChestedHorse, DATA_ID_CHEST};
use super::llama_goals::{
    LlamaHurtByTargetGoal, TraderLlamaDefendWanderingTraderGoal, llama_attack_wolf_goal,
};
use crate::world::entity::ai::goal::LlamaFollowCaravanGoal;
use linkme::distributed_slice;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::enums::{EntitySpawnReason, Pose};
use mc_data::{EntityType, Item, SoundEvent};
use mc_game_core::define_accessor;
use mc_game_core::registry::RegistryAccess;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::damagesource::DamageSource;
use mc_game_core::world::difficulty_instance::DifficultyInstance;
use mc_game_core::world::entity::ai::goal::target::{NearestAttackableTargetGoal, TargetType};
use mc_game_core::world::entity::ai::goal::{
    BreedGoal, FloatGoal, FollowParentGoal, LookAtPlayerGoal, PanicGoal, RandomLookAroundGoal,
    RangedAttackGoal, RunAroundLikeCrazyGoal, TemptGoal, WaterAvoidingRandomStrollGoal,
};
use mc_game_core::world::entity::ai::navigation::StandardPathNavigation;
use mc_game_core::world::entity::animal::equine::abstract_horse::{
    self, AbstractHorse, AbstractHorseData, DATA_ID_FLAGS,
};
use mc_game_core::world::entity::dimensions::{EntityAttachments, EntityDimensions};
use mc_game_core::world::entity::equipment::EquipmentSlot;
use mc_game_core::world::entity::factory::{
    ENTITY_FACTORIES, EntityRegistration, EntitySpawnRequest,
};
use mc_game_core::world::entity::mob::spawn::AgeableMobGroupData;
use mc_game_core::world::entity::mob::{self, MobData, MobKind, leashable};
use mc_game_core::world::entity::spawn_group_data::SpawnGroupData;
use mc_game_core::world::entity::{Entity, EntityId, ServerPlayer};
use mc_game_core::world::item::{InteractionHand, InteractionResult, ItemStack};
use mc_game_core::world::level::server_level_accessor::ServerLevelAccessor;
use mc_nbt::CompoundTag;
use mc_protocol::components::{DataComponentMap, LLAMA_VARIANT, LlamaVariant};
use std::sync::Arc;

define_accessor!(pub DATA_STRENGTH_ID: Int = Llama);
define_accessor!(pub DATA_VARIANT_ID: Int = Llama);

const MAX_STRENGTH: i32 = 5;
const DEFAULT_DESPAWN_DELAY: i32 = 47999;

/// `FollowParentGoal`/`BreedGoal` parent-class test.
type EntityClassTest = Arc<dyn Fn(&dyn Entity) -> bool + Send + Sync>;

/// `Llama.Variant.byId(id)` (`ByIdMap.continuous(.., CLAMP)`).
pub fn variant_by_id(id: i32) -> LlamaVariant {
    LlamaVariant::ALL[id.clamp(0, LlamaVariant::ALL.len() as i32 - 1) as usize]
}

/// `Llama.LlamaGroupData`.
struct LlamaGroupData {
    base: AgeableMobGroupData,
    variant: LlamaVariant,
}

impl SpawnGroupData for LlamaGroupData {
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

/// The fields of `TraderLlama`.
pub struct TraderLlamaData {
    pub despawn_delay: i32,
}

/// `Llama` / `TraderLlama`.
pub struct Llama {
    pub mob: MobData,
    pub horse: AbstractHorseData,
    baby_dimensions: EntityDimensions,
    did_spit: bool,
    caravan_head: Option<EntityId>,
    caravan_tail: Option<EntityId>,
    pub trader: Option<TraderLlamaData>,
}

impl Llama {
    /// `new Llama(type, level)` / `new TraderLlama(type, level)`.
    pub fn new(ty: EntityType, id: EntityId, _registries: &RegistryAccess) -> Self {
        let mob = MobData::new(ty, id, MobKind::Animal, |b| {
            b.define(DATA_ID_FLAGS, 0i8);
            b.define(DATA_ID_CHEST, false);
            b.define(DATA_STRENGTH_ID, 0);
            b.define(DATA_VARIANT_ID, 0);
        });
        let trader = (ty == EntityType::TRADER_LLAMA).then_some(TraderLlamaData {
            despawn_delay: DEFAULT_DESPAWN_DELAY,
        });
        let baby_dimensions = {
            let base = EntityDimensions::of_type(EntityType::LLAMA);
            let attachments =
                EntityAttachments::create_default(base.width, base.height).with_passenger(
                    Vec3::new(0.0, f64::from(EntityType::LLAMA.data().height - 0.25), -0.3),
                );
            base.with_attachments(attachments).scale(0.5)
        };
        let mut llama = Llama {
            mob,
            horse: AbstractHorseData::new(),
            baby_dimensions,
            did_spit: false,
            caravan_head: None,
            caravan_tail: None,
            trader,
        };
        let navigation = StandardPathNavigation::ground(&llama);
        llama.mob.set_navigation(Box::new(navigation));
        llama.register_goals();
        llama.create_inventory();
        llama.horse.can_gallop = false;
        if let Some(mut n) = llama.mob.navigation.take() {
            n.set_required_path_length(&llama, 40.0);
            llama.mob.navigation = Some(n);
        }
        llama
    }

    /// `isTraderLlama()`.
    pub fn is_trader_llama(&self) -> bool {
        self.trader.is_some()
    }

    /// `registerGoals()` of `Llama` (+ `TraderLlama`).
    fn register_goals(&mut self) {
        let float = FloatGoal::new(self);
        let breed = BreedGoal::with_class_test(1.0, self.class_test());
        let follow_parent = FollowParentGoal::with_class_test(1.0, self.class_test());
        let look_at = LookAtPlayerGoal::new_player(self, 6.0);
        let is_trader = self.is_trader_llama();
        let goals = &mut self.mob.goal_selector;
        goals.add_goal(0, Box::new(float));
        goals.add_goal(1, Box::new(RunAroundLikeCrazyGoal::new(1.2)));
        goals.add_goal(2, Box::new(LlamaFollowCaravanGoal::new(2.1)));
        goals.add_goal(3, Box::new(RangedAttackGoal::new(1.25, 40, 20.0)));
        goals.add_goal(3, Box::new(PanicGoal::new(1.2)));
        goals.add_goal(4, Box::new(breed));
        goals.add_goal(
            5,
            Box::new(TemptGoal::new(
                1.25,
                Arc::new(|stack: &ItemStack| {
                    mc_game_core::world::entity::animal::equine::item_in_tag(
                        stack,
                        "llama_tempt_items",
                    )
                }),
                false,
            )),
        );
        goals.add_goal(6, Box::new(follow_parent));
        goals.add_goal(7, Box::new(WaterAvoidingRandomStrollGoal::new(0.7)));
        goals.add_goal(8, Box::new(look_at));
        goals.add_goal(9, Box::new(RandomLookAroundGoal::new()));
        let targets = &mut self.mob.target_selector;
        targets.add_goal(1, Box::new(LlamaHurtByTargetGoal::new()));
        targets.add_goal(2, Box::new(llama_attack_wolf_goal()));
        if is_trader {
            self.mob
                .goal_selector
                .add_goal(1, Box::new(PanicGoal::new(2.0)));
            let targets = &mut self.mob.target_selector;
            targets.add_goal(1, Box::new(TraderLlamaDefendWanderingTraderGoal::new()));
            targets.add_goal(
                2,
                Box::new(NearestAttackableTargetGoal::with_selector(
                    TargetType::Class(Arc::new(|e: &dyn Entity| e.as_zombie().is_some())),
                    true,
                    Arc::new(|target: &dyn Entity, _level: &ServerLevel| {
                        target.get_type() != EntityType::ZOMBIFIED_PIGLIN
                    }),
                )),
            );
            targets.add_goal(
                2,
                Box::new(NearestAttackableTargetGoal::new(
                    TargetType::Class(Arc::new(|e: &dyn Entity| {
                        matches!(
                            e.get_type(),
                            EntityType::PILLAGER
                                | EntityType::VINDICATOR
                                | EntityType::EVOKER
                                | EntityType::ILLUSIONER
                        )
                    })),
                    true,
                )),
            );
        }
    }

    /// The class `FollowParentGoal`/`BreedGoal` match: `Llama.class` (incl. trader llamas) or `TraderLlama.class`.
    fn class_test(&self) -> EntityClassTest {
        if self.is_trader_llama() {
            Arc::new(|e: &dyn Entity| e.get_type() == EntityType::TRADER_LLAMA)
        } else {
            Arc::new(|e: &dyn Entity| e.as_any().is::<Llama>())
        }
    }

    /// `setStrength(strength)`.
    fn set_strength(&mut self, strength: i32) {
        self.mob
            .living
            .entity
            .entity_data
            .set(DATA_STRENGTH_ID, strength.clamp(1, MAX_STRENGTH));
    }

    /// `setRandomStrength(random)`.
    fn set_random_strength(&mut self, random: &mut dyn RandomSource) {
        let max_strength = if random.next_float() < 0.04 { 5 } else { 3 };
        let strength = 1 + random.next_int_bounded(max_strength);
        self.set_strength(strength);
    }

    /// `getStrength()`.
    pub fn get_strength(&self) -> i32 {
        self.mob.living.entity.entity_data.get(DATA_STRENGTH_ID)
    }

    /// `getVariant()`.
    pub fn get_variant(&self) -> LlamaVariant {
        variant_by_id(self.mob.living.entity.entity_data.get(DATA_VARIANT_ID))
    }

    /// `setVariant(variant)`.
    pub fn set_variant(&mut self, variant: LlamaVariant) {
        self.mob
            .living
            .entity
            .entity_data
            .set(DATA_VARIANT_ID, variant.id());
    }

    /// `setDidSpit(b)`.
    pub fn set_did_spit(&mut self, did_spit: bool) {
        self.did_spit = did_spit;
    }

    /// `didSpit`.
    pub fn did_spit(&self) -> bool {
        self.did_spit
    }

    /// `leaveCaravan()`.
    pub fn leave_caravan(&mut self, level: &mut ServerLevel) {
        if let Some(head) = self.caravan_head.take() {
            level.with_entity(head, |_, h| {
                if let Some(llama) = h.as_any_mut().downcast_mut::<Llama>() {
                    llama.caravan_tail = None;
                }
            });
        }
    }

    /// `joinCaravan(tail)` (`tail` is the llama this one follows).
    pub fn join_caravan(&mut self, level: &mut ServerLevel, head: EntityId) {
        self.caravan_head = Some(head);
        let me = self.id();
        level.with_entity(head, |_, h| {
            if let Some(llama) = h.as_any_mut().downcast_mut::<Llama>() {
                llama.caravan_tail = Some(me);
            }
        });
    }

    /// `hasCaravanTail()`.
    pub fn has_caravan_tail(&self) -> bool {
        self.caravan_tail.is_some()
    }

    /// `inCaravan()`.
    pub fn in_caravan(&self) -> bool {
        self.caravan_head.is_some()
    }

    /// `getCaravanHead()`.
    pub fn get_caravan_head(&self) -> Option<EntityId> {
        self.caravan_head
    }

    /// `spit(target)`.
    fn spit(&mut self, level: &mut ServerLevel, target: EntityId) {
        let Some((tx, ty, tz)) = level.get_entity(target).map(|t| {
            (
                t.base().x(),
                t.base().y() + f64::from(t.base().get_bb_height()) * 0.3333333333333333,
                t.base().z(),
            )
        }) else {
            return;
        };
        let spit = mc_game_core::world::entity::projectile::llama_spit::LlamaSpit::from_llama(
            level,
            self.as_dyn(),
        );
        let (sx, sy, sz) = (self.base().x(), spit.base().y(), self.base().z());
        let _ = (sx, sz);
        let xd = tx - self.base().x();
        let yd = ty - sy;
        let zd = tz - self.base().z();
        let yo = (xd * xd + zd * zd).sqrt() * f64::from(0.2f32);
        let uncertainty = self.ranged_attack_uncertainty(level);
        mc_game_core::world::entity::projectile::spawn::spawn_projectile_using_shoot(
            level,
            Box::new(spit),
            &ItemStack::empty(),
            xd,
            yd + yo,
            zd,
            1.5,
            uncertainty,
        );
        if !self.base().is_silent() {
            let pitch = 1.0
                + (self.base_mut().random.next_float() - self.base_mut().random.next_float()) * 0.2;
            let p = self.position();
            let source = self.get_sound_source();
            level.play_sound_at(
                None,
                p.x,
                p.y,
                p.z,
                SoundEvent::ENTITY_LLAMA_SPIT,
                source,
                1.0,
                pitch,
            );
        }
        self.did_spit = true;
    }

    /// `makeNewLlama()`.
    fn make_new_llama(&mut self, level: &mut ServerLevel) -> Option<Box<dyn Entity>> {
        if self.is_trader_llama() {
            let mut baby = mc_game_core::world::entity::factory::create(
                EntityType::TRADER_LLAMA,
                level,
                EntitySpawnRequest {
                    reason: EntitySpawnReason::Breeding,
                    ignore_checks: false,
                },
            )?;
            baby.mob_mut()?.persistence_required = true;
            Some(baby)
        } else {
            mc_game_core::world::entity::factory::create(
                EntityType::LLAMA,
                level,
                EntitySpawnRequest {
                    reason: EntitySpawnReason::Breeding,
                    ignore_checks: false,
                },
            )
        }
    }

    /// `setDespawnDelay(despawnDelay)`.
    pub fn set_despawn_delay(&mut self, despawn_delay: i32) {
        if let Some(t) = self.trader.as_mut() {
            t.despawn_delay = despawn_delay;
        }
    }

    /// `TraderLlama.maybeDespawn()`.
    fn maybe_despawn(&mut self, level: &mut ServerLevel) {
        if self.trader.is_none() || !self.can_despawn(level) {
            return;
        }
        let holder_delay = leashable::get_leash_holder(self)
            .and_then(|h| level.get_entity(h))
            .and_then(|h| h.wandering_trader_despawn_delay());
        let delay = match holder_delay {
            Some(d) => d - 1,
            None => self.trader.as_ref().map_or(0, |t| t.despawn_delay) - 1,
        };
        if let Some(t) = self.trader.as_mut() {
            t.despawn_delay = delay;
        }
        if delay <= 0 {
            leashable::remove_leash(self, level);
            self.discard();
        }
    }

    /// `TraderLlama.canDespawn()`.
    fn can_despawn(&self, level: &ServerLevel) -> bool {
        let leashed_to_trader = self.is_leashed_to_wandering_trader(level);
        !abstract_horse::is_tamed(self)
            && !(leashable::is_leashed(self) && !leashed_to_trader)
            && !self.has_exactly_one_player_passenger(level)
            && !mob::ageable::is_age_locked(self)
            && !self.is_persistence_required()
    }

    fn is_leashed_to_wandering_trader(&self, level: &ServerLevel) -> bool {
        leashable::get_leash_holder(self)
            .and_then(|h| level.get_entity(h))
            .is_some_and(|h| h.wandering_trader_despawn_delay().is_some())
    }

    fn has_exactly_one_player_passenger(&self, level: &ServerLevel) -> bool {
        let passengers = &self.base().passengers;
        passengers.len() == 1
            && level
                .get_entity(passengers[0])
                .is_some_and(|p| p.is_player())
    }
}

impl AbstractHorse for Llama {
    fn horse(&self) -> &AbstractHorseData {
        &self.horse
    }
    fn horse_mut(&mut self) -> &mut AbstractHorseData {
        &mut self.horse
    }
    fn get_eating_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_LLAMA_EAT)
    }
    fn get_angry_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_LLAMA_ANGRY)
    }
    fn can_perform_rearing(&self) -> bool {
        false
    }
    fn get_inventory_columns(&self) -> i32 {
        if abstract_chested_horse::has_chest(self) {
            self.get_strength()
        } else {
            0
        }
    }
    fn can_eat_grass(&self) -> bool {
        false
    }
    fn get_max_temper(&self) -> i32 {
        30
    }
    fn randomize_attributes(&mut self, random: &mut dyn RandomSource) {
        abstract_chested_horse::randomize_attributes(self, random);
    }
    fn handle_eating(
        &mut self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        stack: &ItemStack,
    ) -> bool {
        let mut age_up = 0;
        let mut temper = 0;
        let mut heal = 0.0f32;
        let mut item_used = false;
        if stack.is(Item::WHEAT) {
            age_up = 10;
            temper = 3;
            heal = 2.0;
        } else if stack.is(Item::HAY_BLOCK) {
            age_up = 90;
            temper = 6;
            heal = 10.0;
            if abstract_horse::is_tamed(self)
                && mob::ageable::get_age(self) == 0
                && self.can_fall_in_love()
            {
                item_used = true;
                mob::animal::set_in_love(self, level, Some(&*player));
            }
        }
        if self.get_health() < self.mob.living.get_max_health() && heal > 0.0 {
            self.heal(level, heal);
            item_used = true;
        }
        if self.is_baby() && age_up > 0 && !mob::ageable::is_age_locked(self) {
            mob::ageable::age_up(self, level, age_up, false);
            item_used = true;
        }
        if temper > 0
            && (item_used || !abstract_horse::is_tamed(self))
            && self.horse.temper < self.get_max_temper()
        {
            abstract_horse::modify_temper(self, temper);
            item_used = true;
        }
        if item_used
            && !self.base().is_silent()
            && let Some(sound) = self.get_eating_sound()
        {
            let pitch = 1.0
                + (self.base_mut().random.next_float() - self.base_mut().random.next_float()) * 0.2;
            let p = self.position();
            let source = self.get_sound_source();
            level.play_sound_at(None, p.x, p.y, p.z, sound, source, 1.0, pitch);
        }
        item_used
    }
    fn do_player_ride(&mut self, level: &mut ServerLevel, player: &mut ServerPlayer) {
        if self.is_trader_llama()
            && leashable::get_leash_holder(self)
                .and_then(|h| level.get_entity(h))
                .is_some_and(|h| h.wandering_trader_despawn_delay().is_some())
        {
            return;
        }
        abstract_horse::do_player_ride(self, level, player);
    }
    fn follow_mommy(&mut self, level: &mut ServerLevel) {
        if !self.in_caravan() && self.is_baby() {
            abstract_horse::follow_mommy(self, level);
        }
    }
}

impl AbstractChestedHorse for Llama {
    fn play_chest_equips_sound(&mut self, level: &mut ServerLevel) {
        let pitch =
            (self.base_mut().random.next_float() - self.base_mut().random.next_float()) * 0.2 + 1.0;
        self.play_sound(level, SoundEvent::ENTITY_LLAMA_CHEST, 1.0, pitch);
    }
}

impl Entity for Llama {
    mc_game_core::mob_entity_accessors!(mob);
    mc_game_core::abstract_horse_entity!();

    fn get_encode_id(&self) -> Option<&'static str> {
        Some(if self.is_trader_llama() {
            "minecraft:trader_llama"
        } else {
            "minecraft:llama"
        })
    }

    fn get_dimensions(&self, _pose: Pose) -> EntityDimensions {
        if self.is_baby() {
            self.baby_dimensions.clone()
        } else {
            EntityDimensions::of_type(self.mob.living.entity.entity_type)
        }
    }

    fn ai_step(&mut self, level: &mut ServerLevel) {
        abstract_horse::ai_step(self, level);
        self.maybe_despawn(level);
    }

    fn drop_equipment(&mut self, level: &mut ServerLevel) {
        abstract_chested_horse::drop_equipment(self, level);
    }

    fn can_use_slot(&self, _slot: EquipmentSlot) -> bool {
        true
    }

    fn is_immobile(&self) -> bool {
        self.is_dead_or_dying() || abstract_horse::is_eating(self)
    }

    fn cause_fall_damage(
        &mut self,
        level: &mut ServerLevel,
        fall_distance: f64,
        damage_modifier: f32,
        source: &DamageSource,
    ) -> bool {
        let dmg = mc_game_core::world::entity::living_entity::damage::calculate_fall_damage(
            self,
            fall_distance,
            damage_modifier,
        );
        if dmg <= 0 {
            return false;
        }
        if fall_distance >= 6.0 {
            self.hurt_server(level, source, dmg as f32);
            mc_game_core::world::entity::entity::propagate_fall_to_passengers(
                self,
                level,
                fall_distance,
                damage_modifier,
                source,
            );
        }
        mc_game_core::world::entity::living_entity::damage::play_block_fall_sound(self, level);
        true
    }

    fn play_step_sound(
        &mut self,
        level: &mut ServerLevel,
        _pos: BlockPos,
        _state: mc_data::block::BlockState,
    ) {
        self.play_sound(level, SoundEvent::ENTITY_LLAMA_STEP, 0.15, 1.0);
    }

    fn get_passenger_attachment_point(
        &self,
        passenger: &dyn Entity,
        dimensions: &EntityDimensions,
        _scale: f32,
    ) -> Vec3 {
        mc_game_core::world::entity::entity::default_passenger_attachment_point(
            self.as_dyn(),
            passenger,
            &dimensions.attachments,
        )
    }

    fn is_food(&self, stack: &ItemStack) -> bool {
        mc_game_core::world::entity::animal::equine::item_in_tag(stack, "llama_food")
    }

    fn get_ambient_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_LLAMA_AMBIENT)
    }

    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_LLAMA_DEATH)
    }

    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_LLAMA_HURT)
    }

    fn mob_interact(
        &mut self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        hand: InteractionHand,
    ) -> InteractionResult {
        abstract_chested_horse::mob_interact(self, level, player, hand)
    }

    fn can_mate(&self, partner: &dyn Entity) -> bool {
        partner.id() != self.id()
            && partner
                .as_any()
                .downcast_ref::<Llama>()
                .is_some_and(|llama| {
                    abstract_horse::can_parent(self) && abstract_horse::can_parent(llama)
                })
    }

    fn get_breed_offspring(
        &mut self,
        level: &mut ServerLevel,
        partner: EntityId,
    ) -> Option<Box<dyn Entity>> {
        let mut baby = self.make_new_llama(level)?;
        abstract_horse::set_offspring_attributes(self, level, partner, baby.as_mut());
        let (other_strength, other_variant) = level
            .get_entity(partner)
            .and_then(|p| p.as_any().downcast_ref::<Llama>())
            .map(|p| (p.get_strength(), p.get_variant()))?;
        let max_strength = self.get_strength().max(other_strength);
        let mut baby_strength = self.base_mut().random.next_int_bounded(max_strength) + 1;
        if self.base_mut().random.next_float() < 0.03 {
            baby_strength += 1;
        }
        let variant = if self.base_mut().random.next_boolean() {
            self.get_variant()
        } else {
            other_variant
        };
        if let Some(llama) = baby.as_any_mut().downcast_mut::<Llama>() {
            llama.set_strength(baby_strength);
            llama.set_variant(variant);
        }
        Some(baby)
    }

    fn follow_leash_speed(&self) -> f64 {
        2.0
    }

    fn support_quad_leash(&self) -> bool {
        false
    }

    fn perform_ranged_attack(&mut self, level: &mut ServerLevel, target: EntityId, _power: f32) {
        self.spit(level, target);
    }

    fn ranged_attack_uncertainty(&self, _level: &ServerLevel) -> f32 {
        10.0
    }

    fn finalize_spawn(
        &mut self,
        level: &mut dyn ServerLevelAccessor,
        difficulty: DifficultyInstance,
        reason: EntitySpawnReason,
        group_data: Option<Box<dyn SpawnGroupData>>,
    ) -> Option<Box<dyn SpawnGroupData>> {
        let mut group_data = group_data;
        if self.is_trader_llama() {
            if reason == EntitySpawnReason::Event {
                mob::ageable::set_age(self, 0);
            }
            if group_data.is_none() {
                group_data = Some(Box::new(AgeableMobGroupData::with_baby(false)));
            }
        }
        self.set_random_strength(level.random());
        let variant;
        let group_data = match group_data {
            Some(data) if data.as_any().is::<LlamaGroupData>() => {
                variant = data
                    .as_any()
                    .downcast_ref::<LlamaGroupData>()
                    .expect("llama group data")
                    .variant;
                data
            }
            _ => {
                let n = level
                    .random()
                    .next_int_bounded(LlamaVariant::ALL.len() as i32);
                variant = LlamaVariant::ALL[n as usize];
                Box::new(LlamaGroupData {
                    base: AgeableMobGroupData::with_baby(true),
                    variant,
                }) as Box<dyn SpawnGroupData>
            }
        };
        self.set_variant(variant);
        let group_data = abstract_horse::finalize_spawn_prepare(self, level, Some(group_data));
        mob::spawn::finalize_spawn(self, level, difficulty, reason, group_data)
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        abstract_chested_horse::add_additional_save_data(self, out);
        out.put_int("Variant", self.get_variant().id());
        out.put_int("Strength", self.get_strength());
        if let Some(t) = &self.trader {
            out.put_int("DespawnDelay", t.despawn_delay);
        }
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        self.set_strength(input.get_int_or("Strength", 0));
        abstract_chested_horse::read_additional_save_data(self, input);
        let variant = input
            .get_int("Variant")
            .map_or(LlamaVariant::Creamy, variant_by_id);
        self.set_variant(variant);
        if self.trader.is_some() {
            self.trader = Some(TraderLlamaData {
                despawn_delay: input.get_int_or("DespawnDelay", DEFAULT_DESPAWN_DELAY),
            });
        }
    }

    fn entity_components(&self) -> DataComponentMap {
        let base = mc_game_core::advancements::predicates::entity::support::base_entity_components(
            self.base(),
        );
        DataComponentMap::builder()
            .add_all(&base)
            .set(LLAMA_VARIANT, self.get_variant())
            .build()
    }

    fn apply_implicit_components(&mut self, stack: &ItemStack) {
        if let Some(variant) = stack.get(&LLAMA_VARIANT) {
            self.set_variant(*variant);
        }
        mc_game_core::world::entity::spawn_config::apply_base_implicit_components(self, stack);
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static LLAMA: EntityRegistration = EntityRegistration {
    entity_type: EntityType::LLAMA,
    create: |ty, id, registries| Box::new(Llama::new(ty, id, registries)),
};

#[distributed_slice(ENTITY_FACTORIES)]
static TRADER_LLAMA: EntityRegistration = EntityRegistration {
    entity_type: EntityType::TRADER_LLAMA,
    create: |ty, id, registries| Box::new(Llama::new(ty, id, registries)),
};
