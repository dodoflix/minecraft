//! `IronGolem`: village protector (neutral, anger timer), cracks as it is damaged, healed with iron ingots, offers poppies.

use super::abstract_golem::AMBIENT_SOUND_INTERVAL;
use mc_game_core::define_accessor;
use mc_game_core::registry::RegistryAccess;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::damagesource::DamageSource;
use mc_game_ai::world::entity::ai::goal::target::DefendVillageTargetGoal;
use mc_game_core::world::entity::ai::goal::target::{HurtByTargetGoal, NearestAttackableTargetGoal, TargetType};
use mc_game_ai::world::entity::ai::goal::{GolemRandomStrollInVillageGoal, MoveTowardsTargetGoal, OfferFlowerGoal, ResetUniversalAngerTargetGoal};
use mc_game_core::world::entity::ai::goal::{LookAtPlayerGoal, MeleeAttackGoal, MoveBackToVillageGoal, RandomLookAroundGoal};
use mc_game_core::world::entity::ai::navigation::StandardPathNavigation;
use mc_game_core::world::entity::crackiness::{Crackiness, CrackinessLevel};
use mc_game_core::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use mc_game_core::world::entity::living_entity::util::broadcast_entity_event;
use mc_game_core::world::entity::mob::neutral::{self, NeutralData};
use mc_game_core::world::entity::mob::{self, MobData, MobKind, interact};
use mc_game_core::world::entity::{Entity, EntityId, ServerPlayer};
use mc_game_core::world::item::enchantment::actors::Actors;
use mc_game_core::world::item::enchantment_helper;
use mc_game_core::world::item::{InteractionHand, InteractionResult, ItemStackExt};
use mc_game_core::world::level::server_level_accessor::ServerLevelAccessor;
use linkme::distributed_slice;
use mc_core::phys::Vec3;
use mc_core::random::RandomSource;
use mc_data::fluid::FluidState;
use mc_data::{Attribute, EntityType, Item, SoundEvent};
use mc_nbt::CompoundTag;
use std::sync::Arc;

define_accessor!(pub DATA_FLAGS_ID: Byte = IronGolem);

const IRON_INGOT_HEAL_AMOUNT: f32 = 25.0;

pub struct IronGolem {
    pub mob: MobData,
    attack_animation_tick: i32,
    offer_flower_tick: i32,
}

impl IronGolem {
    /// `new IronGolem(type, level)` (`registerGoals` and `createNavigation` run as in `Mob.<init>`).
    pub fn new(ty: EntityType, id: EntityId, _registries: &RegistryAccess) -> Self {
        let mob = MobData::new(ty, id, MobKind::PathfinderMob, |b| {
            b.define(DATA_FLAGS_ID, 0i8);
        });
        let mut golem = IronGolem { mob, attack_animation_tick: 0, offer_flower_tick: 0 };
        golem.mob.neutral = Some(NeutralData::default());
        let navigation = StandardPathNavigation::ground(&golem);
        golem.mob.set_navigation(Box::new(navigation));
        golem.register_goals();
        golem
    }

    /// `registerGoals()`.
    fn register_goals(&mut self) {
        let look_at = LookAtPlayerGoal::new_player(self, 6.0);
        let goals = &mut self.mob.goal_selector;
        goals.add_goal(1, Box::new(MeleeAttackGoal::new(1.0, true)));
        goals.add_goal(2, Box::new(MoveTowardsTargetGoal::new(0.9, 32.0)));
        goals.add_goal(2, Box::new(MoveBackToVillageGoal::new(0.6, false)));
        goals.add_goal(4, Box::new(GolemRandomStrollInVillageGoal::new(0.6)));
        goals.add_goal(5, Box::new(OfferFlowerGoal::new(offer_flower)));
        goals.add_goal(7, Box::new(look_at));
        goals.add_goal(8, Box::new(RandomLookAroundGoal::new()));
        let targets = &mut self.mob.target_selector;
        targets.add_goal(1, Box::new(DefendVillageTargetGoal::new()));
        targets.add_goal(2, Box::new(HurtByTargetGoal::new(vec![])));
        targets.add_goal(
            3,
            Box::new(NearestAttackableTargetGoal::build(TargetType::Player, 10, true, false, None).with_source_selector(Arc::new(|mob, target, level| neutral::is_angry_at(mob, level, target)))),
        );
        targets.add_goal(
            3,
            Box::new(NearestAttackableTargetGoal::build(
                TargetType::Class(Arc::new(|e| e.mob().is_some())),
                5,
                false,
                false,
                Some(Arc::new(|target, _level| target.is_enemy() && target.get_type() != EntityType::CREEPER)),
            )),
        );
        targets.add_goal(4, Box::new(ResetUniversalAngerTargetGoal::new(false)));
    }

    /// `getAttackDamage()`.
    fn get_attack_damage(&self) -> f32 {
        self.mob.living.get_attribute_value(Attribute::ATTACK_DAMAGE) as f32
    }

    /// `getCrackiness()`.
    pub fn get_crackiness(&self) -> CrackinessLevel {
        Crackiness::GOLEM.by_fraction(self.mob.living.get_health() / self.mob.living.get_max_health())
    }

    pub fn get_attack_animation_tick(&self) -> i32 {
        self.attack_animation_tick
    }

    pub fn get_offer_flower_tick(&self) -> i32 {
        self.offer_flower_tick
    }

    /// `offerFlower(offer)`.
    pub fn offer_flower(&mut self, level: &mut ServerLevel, offer: bool) {
        if offer {
            self.offer_flower_tick = 400;
            broadcast_entity_event(level, self.id(), 11);
        } else {
            self.offer_flower_tick = 0;
            broadcast_entity_event(level, self.id(), 34);
        }
    }

    /// `isPlayerCreated()`.
    pub fn is_player_created(&self) -> bool {
        (self.mob.living.entity.entity_data.get(DATA_FLAGS_ID) & 1) != 0
    }

    /// `setPlayerCreated(value)`.
    pub fn set_player_created(&mut self, value: bool) {
        let current = self.mob.living.entity.entity_data.get(DATA_FLAGS_ID);
        self.mob.living.entity.entity_data.set(DATA_FLAGS_ID, if value { current | 1 } else { current & -2 });
    }
}

/// `OfferFlowerGoal`'s `golem.offerFlower(offering)`.
fn offer_flower(golem: &mut dyn Entity, level: &mut ServerLevel, offer: bool) {
    if let Some(g) = golem.as_any_mut().downcast_mut::<IronGolem>() {
        g.offer_flower(level, offer);
    }
}

impl Entity for IronGolem {
    mc_game_core::mob_entity_accessors!(mob);

    fn get_encode_id(&self) -> Option<&'static str> {
        Some("minecraft:iron_golem")
    }

    fn decrease_air_supply(&mut self, current_supply: i32) -> i32 {
        current_supply
    }

    fn do_push(&mut self, level: &mut ServerLevel, other: &mut dyn Entity) {
        if other.is_enemy() && other.get_type() != EntityType::CREEPER && self.base_mut().random.next_int_bounded(20) == 0 {
            self.set_target(level, Some(other.id()));
        }
        mc_game_core::world::entity::living_entity::push::do_push(level, self, other);
    }

    fn ai_step(&mut self, level: &mut ServerLevel) {
        mob::tick::ai_step(self, level);
        if self.attack_animation_tick > 0 {
            self.attack_animation_tick -= 1;
        }
        if self.offer_flower_tick > 0 {
            self.offer_flower_tick -= 1;
        }
        neutral::update_persistent_anger(self, level, true);
    }

    fn can_spawn_sprint_particle(&mut self) -> bool {
        self.base().delta_movement().horizontal_distance_sqr() > f64::from(2.5000003E-7f32) && self.base_mut().random.next_int_bounded(5) == 0
    }

    fn can_attack(&self, level: &ServerLevel, target: &dyn Entity) -> bool {
        if self.is_player_created() && target.is_player() {
            return false;
        }
        if target.get_type() == EntityType::CREEPER {
            return false;
        }
        // `super.canAttack(target)`: the `Mob` / `LivingEntity` chain
        target.get_type() != EntityType::GHAST
            && !(target.is_player() && level.difficulty() == mc_world::level_storage::Difficulty::Peaceful)
            && target.can_be_seen_as_enemy()
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        mob::nbt::add_additional_save_data(self, out);
        out.put_boolean("PlayerCreated", self.is_player_created());
        neutral::add_persistent_anger_save_data(self, out);
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        mob::nbt::read_additional_save_data(self, input);
        self.set_player_created(input.get_boolean_or("PlayerCreated", false));
        neutral::read_persistent_anger_save_data(self, None, input);
    }

    fn start_persistent_anger_timer(&mut self, level: &mut ServerLevel) {
        // `PERSISTENT_ANGER_TIME = TimeUtil.rangeOfSeconds(20, 39)` = UniformInt(400, 780)
        let time = self.base_mut().random.next_int_between_inclusive(400, 780);
        neutral::set_time_to_remain_angry(self, level, i64::from(time));
    }

    fn do_hurt_target(&mut self, level: &mut ServerLevel, target: &mut dyn Entity) -> bool {
        self.attack_animation_tick = 10;
        broadcast_entity_event(level, self.id(), 4);
        let attack_damage = self.get_attack_damage();
        let damage = if (attack_damage as i32) > 0 { attack_damage / 2.0 + self.base_mut().random.next_int_bounded(attack_damage as i32) as f32 } else { attack_damage };
        let damage_source = level.damage_sources().mob_attack(&*self);
        let hurt = target.hurt_server(level, &damage_source, damage);
        if hurt {
            let knockback_resistance = target.living().map_or(0.0, |l| l.get_attribute_value(Attribute::KNOCKBACK_RESISTANCE));
            let scale = (1.0 - knockback_resistance).max(0.0);
            let movement = target.base().delta_movement();
            target.base_mut().set_delta_movement(movement.add(Vec3::new(0.0, f64::from(0.4f32) * scale, 0.0)));
            let target_id = target.id();
            enchantment_helper::do_post_attack_effects(level, &mut Actors::of(&mut *self).and(&mut *target), target_id, &damage_source);
        }
        self.play_sound(level, SoundEvent::ENTITY_IRON_GOLEM_ATTACK, 1.0, 1.0);
        hurt
    }

    fn hurt_server(&mut self, level: &mut ServerLevel, source: &DamageSource, damage: f32) -> bool {
        let previous_crackiness = self.get_crackiness();
        let was_hurt = mc_game_core::world::entity::living_entity::damage::hurt_server(self, level, source, damage);
        if was_hurt && self.get_crackiness() != previous_crackiness {
            self.play_sound(level, SoundEvent::ENTITY_IRON_GOLEM_DAMAGE, 1.0, 1.0);
        }
        was_hurt
    }

    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_IRON_GOLEM_HURT)
    }

    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_IRON_GOLEM_DEATH)
    }

    fn get_ambient_sound_interval(&self) -> i32 {
        AMBIENT_SOUND_INTERVAL
    }

    fn remove_when_far_away(&self, _distance_to_closest_player: f64) -> bool {
        false
    }

    fn mob_interact(&mut self, level: &mut ServerLevel, player: &mut ServerPlayer, hand: InteractionHand) -> InteractionResult {
        let item_stack = player.get_item_in_hand(hand);
        if !item_stack.is(Item::IRON_INGOT) {
            return InteractionResult::PASS;
        }
        let health_before = self.get_health();
        self.heal(level, IRON_INGOT_HEAL_AMOUNT);
        if self.get_health() == health_before {
            return InteractionResult::PASS;
        }
        let pitch = 1.0 + (self.base_mut().random.next_float() - self.base_mut().random.next_float()) * 0.2;
        self.play_sound(level, SoundEvent::ENTITY_IRON_GOLEM_REPAIR, 1.0, pitch);
        let infinite = player.player.abilities.instabuild;
        interact::with_hand_stack(player, hand, |stack, _| stack.consume(1, infinite));
        InteractionResult::SUCCESS
    }

    fn play_step_sound(&mut self, level: &mut ServerLevel, _pos: mc_core::pos::BlockPos, _state: mc_data::block::BlockState) {
        self.play_sound(level, SoundEvent::ENTITY_IRON_GOLEM_STEP, 1.0, 1.0);
    }

    fn check_spawn_obstruction(&self, level: &dyn ServerLevelAccessor) -> bool {
        use mc_game_core::world::level::spawner::natural_spawner::is_valid_empty_spawn_block;
        let pos = self.block_position();
        let below_pos = pos.below();
        let below = level.get_block_state(below_pos);
        if !mc_game_core::world::entity::ai::navigation::entity_can_stand_on(below, level, below_pos, self) {
            return false;
        }
        for i in 1..3 {
            let above_pos = pos.above_n(i);
            let above = level.get_block_state(above_pos);
            if !is_valid_empty_spawn_block(level, above_pos, above, above.get_fluid_state(), EntityType::IRON_GOLEM) {
                return false;
            }
        }
        is_valid_empty_spawn_block(level, pos, level.get_block_state(pos), FluidState::EMPTY, EntityType::IRON_GOLEM) && level.is_unobstructed_entity(self)
    }

    fn get_leash_offset(&self) -> Vec3 {
        Vec3::new(0.0, f64::from(0.875f32 * self.base().eye_height), f64::from(self.base().get_bb_width() * 0.4))
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static IRON_GOLEM: EntityRegistration = EntityRegistration { entity_type: EntityType::IRON_GOLEM, create: |ty, id, registries| Box::new(IronGolem::new(ty, id, registries)) };
