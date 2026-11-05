//! `net.minecraft.world.entity.monster.Creeper`.

use crate::define_accessor;
use crate::registry::RegistryAccess;
use crate::server::level::ServerLevel;
use crate::server::level::level_explode::Explode;
use crate::world::damagesource::DamageSource;
use crate::world::entity::ServerPlayer;
use crate::world::entity::ai::goal::target::{HurtByTargetGoal, NearestAttackableTargetGoal, TargetType, class_of_type};
use crate::world::entity::ai::goal::{AvoidEntityGoal, FloatGoal, LookAtPlayerGoal, MeleeAttackGoal, RandomLookAroundGoal, SwellGoal, WaterAvoidingRandomStrollGoal};
use crate::world::entity::area_effect_cloud::AreaEffectCloud;
use crate::world::entity::ai::navigation::StandardPathNavigation;
use crate::world::entity::effect::MobEffectInstance;
use crate::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use crate::world::entity::living_entity::util::l;
use crate::world::entity::mob::{self, MobData, MobKind};
use crate::world::entity::{Entity, EntityId};
use crate::world::item::{InteractionHand, InteractionResult, ItemStackExt as _, hurt_and_break};
use crate::world::level::explosion::ExplosionInteraction;
use linkme::distributed_slice;
use mc_core::math::mth;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::{EntityType, GameEvent, Item, SoundEvent};
use mc_nbt::CompoundTag;
use std::sync::Arc;

define_accessor!(pub DATA_SWELL_DIR: Int = Creeper);
define_accessor!(pub DATA_IS_POWERED: Boolean = Creeper);
define_accessor!(pub DATA_IS_IGNITED: Boolean = Creeper);

const DEFAULT_MAX_SWELL: i32 = 30;
const DEFAULT_EXPLOSION_RADIUS: i32 = 3;

/// `Creeper`.
pub struct Creeper {
    pub mob: MobData,
    old_swell: i32,
    swell: i32,
    max_swell: i32,
    explosion_radius: i32,
    dropped_skulls: bool,
}

impl Creeper {
    /// `new Creeper(type, level)`.
    pub fn new(ty: EntityType, id: EntityId, _registries: &RegistryAccess) -> Self {
        let mob = MobData::new(ty, id, MobKind::Monster, |b| {
            b.define(DATA_SWELL_DIR, -1);
            b.define(DATA_IS_POWERED, false);
            b.define(DATA_IS_IGNITED, false);
        });
        let mut creeper = Creeper { mob, old_swell: 0, swell: 0, max_swell: DEFAULT_MAX_SWELL, explosion_radius: DEFAULT_EXPLOSION_RADIUS, dropped_skulls: false };
        let navigation = StandardPathNavigation::ground(&creeper);
        creeper.mob.set_navigation(Box::new(navigation));
        creeper.register_goals();
        creeper
    }

    /// `registerGoals()`.
    fn register_goals(&mut self) {
        let float = FloatGoal::new(self);
        let look_at = LookAtPlayerGoal::new_player(self, 8.0);
        let goals = &mut self.mob.goal_selector;
        goals.add_goal(1, Box::new(float));
        goals.add_goal(2, Box::new(SwellGoal::new()));
        goals.add_goal(3, Box::new(AvoidEntityGoal::new(Arc::new(|e: &dyn Entity| e.get_type() == EntityType::OCELOT), 6.0, 1.0, 1.2)));
        goals.add_goal(3, Box::new(AvoidEntityGoal::new(Arc::new(|e: &dyn Entity| e.get_type() == EntityType::CAT), 6.0, 1.0, 1.2)));
        goals.add_goal(4, Box::new(MeleeAttackGoal::new(1.0, false)));
        goals.add_goal(5, Box::new(WaterAvoidingRandomStrollGoal::new(0.8)));
        goals.add_goal(6, Box::new(look_at));
        goals.add_goal(6, Box::new(RandomLookAroundGoal::new()));
        let targets = &mut self.mob.target_selector;
        targets.add_goal(1, Box::new(NearestAttackableTargetGoal::new(TargetType::Player, true)));
        targets.add_goal(2, Box::new(HurtByTargetGoal::new(Vec::new())));
        let _ = class_of_type;
    }

    /// `isPowered()`.
    pub fn is_powered(&self) -> bool {
        self.mob.living.entity.entity_data.get(DATA_IS_POWERED)
    }

    /// `getSwelling(a)`.
    pub fn get_swelling(&self, a: f32) -> f32 {
        mth::lerp_f32(a, self.old_swell as f32, self.swell as f32) / (self.max_swell - 2) as f32
    }

    /// `isIgnited()`.
    pub fn is_ignited(&self) -> bool {
        self.mob.living.entity.entity_data.get(DATA_IS_IGNITED)
    }

    /// `ignite()`.
    pub fn ignite(&mut self) {
        self.mob.living.entity.entity_data.set(DATA_IS_IGNITED, true);
    }

    /// `explodeCreeper()`.
    fn explode_creeper(&mut self, level: &mut ServerLevel) {
        let explosion_multiplier = if self.is_powered() { 2.0f32 } else { 1.0f32 };
        self.mob.living.dead = true;
        let p = self.position();
        level.explode(Explode::new(Some(&*self), p.x, p.y, p.z, self.explosion_radius as f32 * explosion_multiplier, ExplosionInteraction::Mob), &mut []);
        self.spawn_lingering_cloud(level);
        crate::world::entity::living_entity::effects::trigger_on_death_mob_effects(self, level, crate::world::entity::RemovalReason::Killed);
        self.discard();
    }

    /// `spawnLingeringCloud()`.
    fn spawn_lingering_cloud(&self, level: &mut ServerLevel) {
        let active_effects: Vec<MobEffectInstance> = l(self).active_effects.values().cloned().collect();
        if active_effects.is_empty() {
            return;
        }
        let p = self.position();
        let mut cloud = AreaEffectCloud::at(level, p.x, p.y, p.z);
        cloud.set_radius(2.5);
        cloud.set_radius_on_use(-0.5);
        cloud.set_wait_time(10);
        cloud.set_duration(300);
        cloud.set_potion_duration_scale(0.25);
        cloud.set_radius_per_tick(-cloud.get_radius() / cloud.get_duration() as f32);
        for effect in &active_effects {
            cloud.add_effect(effect);
        }
        level.add_fresh_entity(Box::new(cloud));
    }
}

impl Entity for Creeper {
    crate::mob_entity_accessors!(mob);

    fn get_max_fall_distance(&self, level: &ServerLevel) -> i32 {
        if self.get_target().is_none() {
            mob::tick::comfortable_fall_distance(0.0)
        } else {
            let _ = level;
            mob::tick::comfortable_fall_distance(self.get_health() - 1.0)
        }
    }

    fn cause_fall_damage(&mut self, level: &mut ServerLevel, fall_distance: f64, damage_modifier: f32, source: &DamageSource) -> bool {
        let damaged = crate::world::entity::living_entity::damage::cause_fall_damage(self, level, fall_distance, damage_modifier, source);
        self.swell += (fall_distance * 1.5) as i32;
        if self.swell > self.max_swell - 5 {
            self.swell = self.max_swell - 5;
        }
        damaged
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        mob::nbt::add_additional_save_data(self, out);
        out.put_boolean("powered", self.is_powered());
        out.put_short("Fuse", self.max_swell as i16);
        out.put_byte("ExplosionRadius", self.explosion_radius as i8);
        out.put_boolean("ignited", self.is_ignited());
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        mob::nbt::read_additional_save_data(self, input);
        self.mob.living.entity.entity_data.set(DATA_IS_POWERED, input.get_boolean_or("powered", false));
        self.max_swell = i32::from(input.get_short_or("Fuse", DEFAULT_MAX_SWELL as i16));
        self.explosion_radius = i32::from(input.get_byte_or("ExplosionRadius", DEFAULT_EXPLOSION_RADIUS as i8));
        if input.get_boolean_or("ignited", false) {
            self.ignite();
        }
    }

    fn tick(&mut self, level: &mut ServerLevel) {
        if self.is_alive() {
            self.old_swell = self.swell;
            if self.is_ignited() {
                self.set_swell_dir(1);
            }
            let swell_dir = self.get_swell_dir();
            if swell_dir > 0 && self.swell == 0 {
                self.play_sound(level, SoundEvent::ENTITY_CREEPER_PRIMED, 1.0, 0.5);
                self.game_event(level, GameEvent::PRIME_FUSE);
            }
            self.swell += swell_dir;
            if self.swell < 0 {
                self.swell = 0;
            }
            if self.swell >= self.max_swell {
                self.swell = self.max_swell;
                self.explode_creeper(level);
            }
        }
        mob::tick::tick(self, level);
    }

    fn set_target(&mut self, level: &ServerLevel, target: Option<EntityId>) {
        if !target.and_then(|t| level.get_entity(t)).is_some_and(|t| t.get_type() == EntityType::GOAT) {
            mob::tick::set_target(self, level, target);
        }
    }

    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_CREEPER_HURT)
    }

    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_CREEPER_DEATH)
    }

    fn do_hurt_target(&mut self, _level: &mut ServerLevel, _target: &mut dyn Entity) -> bool {
        true
    }

    fn get_swell_dir(&self) -> i32 {
        self.mob.living.entity.entity_data.get(DATA_SWELL_DIR)
    }

    fn set_swell_dir(&mut self, dir: i32) {
        self.mob.living.entity.entity_data.set(DATA_SWELL_DIR, dir);
    }

    fn thunder_hit(&mut self, level: &mut ServerLevel) {
        // `Entity.thunderHit`
        let ticks = self.get_remaining_fire_ticks().wrapping_add(1);
        self.set_remaining_fire_ticks(ticks);
        if self.get_remaining_fire_ticks() == 0 {
            self.ignite_for_seconds(8.0);
        }
        let source = level.damage_sources().lightning_bolt();
        self.hurt_server(level, &source, 5.0);
        self.mob.living.entity.entity_data.set(DATA_IS_POWERED, true);
    }

    fn mob_interact(&mut self, level: &mut ServerLevel, player: &mut ServerPlayer, hand: InteractionHand) -> InteractionResult {
        let item_stack = player.get_item_in_hand(hand);
        if mc_data::tags::get::<Item>("creeper_igniters").is_some_and(|t| t.contains(item_stack.item())) {
            let sound = if item_stack.is(Item::FIRE_CHARGE) { SoundEvent::ITEM_FIRECHARGE_USE } else { SoundEvent::ITEM_FLINTANDSTEEL_USE };
            let pitch = self.base_mut().random.next_float() * 0.4 + 0.8;
            let p = self.position();
            let source = self.get_sound_source();
            level.play_sound_at(Some(player.id()), p.x, p.y, p.z, sound, source, 1.0, pitch);
            self.ignite();
            mob::interact::with_hand_stack(player, hand, |stack, player| {
                if !stack.is_damageable_item() {
                    stack.shrink(1);
                } else {
                    hurt_and_break(stack, 1, level, player, hand.as_equipment_slot());
                }
            });
            InteractionResult::SUCCESS
        } else {
            mob::interact::mob_interact(self, level, player, hand)
        }
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static CREEPER: EntityRegistration = EntityRegistration { entity_type: EntityType::CREEPER, create: |ty, id, registries| Box::new(Creeper::new(ty, id, registries)) };

#[allow(dead_code)]
fn _unused(_: BlockPos) {}
