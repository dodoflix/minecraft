//! `Witch`: the potion-throwing raider (swamp huts, raids, struck villagers).

use linkme::distributed_slice;
use mc_core::ident::Identifier;
use mc_core::random::RandomSource;
use mc_data::{EntityType, GameEvent, Item, MobEffect, Potion, SoundEvent};
use mc_game_ai::world::entity::ai::goal::target::{
    NearestAttackableWitchTargetGoal, NearestHealableRaiderTargetGoal,
};
use mc_game_core::define_accessor;
use mc_game_core::registry::RegistryAccess;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::damagesource::{DamageSource, DamageTypeTags};
use mc_game_core::world::effect::potion_contents;
use mc_game_core::world::entity::ai::goal::target::{
    HurtByTargetGoal, NearestAttackableTargetGoal, TargetType,
};
use mc_game_core::world::entity::ai::goal::{
    FloatGoal, LookAtPlayerGoal, RandomLookAroundGoal, RangedAttackGoal,
    WaterAvoidingRandomStrollGoal,
};
use mc_game_core::world::entity::ai::navigation::StandardPathNavigation;
use mc_game_core::world::entity::attributes::{AttributeModifier, AttributeModifierOperation};
use mc_game_core::world::entity::equipment::EquipmentSlot;
use mc_game_core::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use mc_game_core::world::entity::living_entity::damage::{self, broadcast_entity_event};
use mc_game_core::world::entity::mob::{self, MobData, MobKind};
use mc_game_core::world::entity::projectile::spawn::spawn_projectile_using_shoot;
use mc_game_core::world::entity::projectile::throwableitemprojectile::thrown_splash_potion::ThrownSplashPotion;
use mc_game_core::world::entity::raid::raider::{self, RaiderData};
use mc_game_core::world::entity::scenario_dump::{SCENARIO_DUMP_EXTRA, ScenarioDumpExtra};
use mc_game_core::world::entity::{Entity, EntityId};
use mc_game_core::world::item::{ItemStack, behaviour};
use mc_protocol::components::{POTION_CONTENTS, POTION_DURATION_SCALE, PotionContents};
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::{Arc, LazyLock};

define_accessor!(DATA_USING_ITEM: Boolean = Witch);

/// `SPEED_MODIFIER_DRINKING_ID`.
static SPEED_MODIFIER_DRINKING_ID: LazyLock<Identifier> =
    LazyLock::new(|| Identifier::new("minecraft", "drinking").expect("valid id"));

/// `SPEED_MODIFIER_DRINKING`.
fn speed_modifier_drinking() -> AttributeModifier {
    AttributeModifier {
        id: SPEED_MODIFIER_DRINKING_ID.clone(),
        amount: -0.25,
        operation: AttributeModifierOperation::AddValue,
    }
}

/// `PotionContents.createItemStack(item, potion)`.
fn create_potion_stack(item: Item, potion: Potion) -> ItemStack {
    let mut stack = ItemStack::new(item);
    stack.set(
        POTION_CONTENTS,
        PotionContents {
            potion: Some(potion),
            ..potion_contents::empty_contents()
        },
    );
    stack
}

pub struct Witch {
    pub mob: MobData,
    raider: RaiderData,
    using_time: i32,
    /// `healRaidersGoal.cooldown`, shared with the goal.
    heal_raiders_cooldown: Arc<AtomicI32>,
    /// `attackPlayersGoal.canAttack`, shared with the goal.
    attack_players_can_attack: Arc<AtomicBool>,
}

impl Witch {
    /// `new Witch(type, level)`: `registerGoals()` and the ground navigation.
    pub fn new(ty: EntityType, id: EntityId, _registries: &RegistryAccess) -> Self {
        let mob = MobData::new(ty, id, MobKind::Monster, |b| {
            raider::define_synched_data(b);
            b.define(DATA_USING_ITEM, false);
        });
        let mut witch = Witch {
            mob,
            raider: RaiderData::default(),
            using_time: 0,
            heal_raiders_cooldown: Arc::new(AtomicI32::new(0)),
            attack_players_can_attack: Arc::new(AtomicBool::new(true)),
        };
        let navigation = StandardPathNavigation::ground(&witch);
        witch.mob.set_navigation(Box::new(navigation));
        witch.register_goals();
        witch
    }

    /// `registerGoals()`.
    fn register_goals(&mut self) {
        raider::register_goals(&mut self.mob);
        let raider_class: TargetType =
            TargetType::Class(Arc::new(|e: &dyn Entity| e.raider().is_some()));
        let heal_raiders = NearestHealableRaiderTargetGoal::new(
            NearestAttackableTargetGoal::build(raider_class, 500, true, false, None)
                .with_source_selector(Arc::new(|witch, target, level| {
                    raider::has_active_raid(witch, level) && target.get_type() != EntityType::WITCH
                })),
            self.heal_raiders_cooldown.clone(),
        );
        let attack_players = NearestAttackableWitchTargetGoal::new(
            NearestAttackableTargetGoal::build(TargetType::Player, 10, true, false, None),
            self.attack_players_can_attack.clone(),
        );
        let float = FloatGoal::new(self);
        let look_at_player = LookAtPlayerGoal::new_player(self, 8.0);
        let sel = &mut self.mob.goal_selector;
        sel.add_goal(1, Box::new(float));
        sel.add_goal(2, Box::new(RangedAttackGoal::new(1.0, 60, 10.0)));
        sel.add_goal(2, Box::new(WaterAvoidingRandomStrollGoal::new(1.0)));
        sel.add_goal(3, Box::new(look_at_player));
        sel.add_goal(3, Box::new(RandomLookAroundGoal::new()));
        let t = &mut self.mob.target_selector;
        t.add_goal(
            1,
            Box::new(HurtByTargetGoal::new(vec![Arc::new(|e: &dyn Entity| {
                e.raider().is_some()
            })])),
        );
        t.add_goal(2, Box::new(heal_raiders));
        t.add_goal(3, Box::new(attack_players));
    }

    /// `setUsingItem(using)`.
    fn set_using_item(&mut self, using: bool) {
        self.base_mut().entity_data.set(DATA_USING_ITEM, using);
    }

    /// `isDrinkingPotion()`.
    pub fn is_drinking_potion(&self) -> bool {
        self.base().entity_data.get(DATA_USING_ITEM)
    }

    /// The potion a witch that is not drinking decides to drink this tick: four chained `nextFloat` branches.
    fn choose_potion_to_drink(&mut self, level: &ServerLevel) -> Option<Potion> {
        if self.base_mut().random.next_float() < 0.15
            && self.is_eye_in_fluid("water")
            && !self.has_effect(MobEffect::WATER_BREATHING)
        {
            Some(Potion::WATER_BREATHING)
        } else if self.base_mut().random.next_float() < 0.15
            && (self.is_on_fire()
                || mob::ext::get_last_damage_source(level, self, 40)
                    .is_some_and(|s| s.is_tag(level, DamageTypeTags::IS_FIRE)))
            && !self.has_effect(MobEffect::FIRE_RESISTANCE)
        {
            Some(Potion::FIRE_RESISTANCE)
        } else if self.base_mut().random.next_float() < 0.05
            && self.get_health() < self.mob.living.get_max_health()
        {
            Some(Potion::HEALING)
        } else if self.base_mut().random.next_float() < 0.5
            && !self.has_effect(MobEffect::SPEED)
            && self.get_target().is_some_and(|id| {
                level
                    .get_entity(id)
                    .is_some_and(|t| t.base().distance_to_sqr_vec(self.position()) > 121.0)
            })
        {
            Some(Potion::SWIFTNESS)
        } else {
            None
        }
    }

    /// The `isDrinkingPotion()` branch of `aiStep()`: when the use time is up the drunk potion's effects apply.
    fn finish_drinking(&mut self, level: &mut ServerLevel) {
        self.set_using_item(false);
        let stack = self.get_item_by_slot(EquipmentSlot::MainHand);
        mob::equipment::set_item_slot(self, level, EquipmentSlot::MainHand, ItemStack::empty());
        let contents = stack.get(&POTION_CONTENTS).cloned();
        if stack.is(Item::POTION)
            && let Some(contents) = contents
        {
            let scale = stack.get(&POTION_DURATION_SCALE).copied().unwrap_or(1.0);
            let mut effects = Vec::new();
            potion_contents::for_each_effect(&contents, scale, |effect| effects.push(effect));
            for effect in effects {
                self.add_effect(level, effect, None);
            }
        }
        self.game_event(level, GameEvent::DRINK);
        if let Some(speed) = self
            .mob
            .living
            .attributes
            .get_instance(mc_data::Attribute::MOVEMENT_SPEED)
        {
            speed.remove_modifier(&SPEED_MODIFIER_DRINKING_ID);
        }
    }

    /// Starts drinking `potion`: the potion in the main hand, the use time, the sound and the slowing modifier.
    fn start_drinking(&mut self, level: &mut ServerLevel, potion: Potion) {
        let stack = create_potion_stack(Item::POTION, potion);
        mob::equipment::set_item_slot(self, level, EquipmentSlot::MainHand, stack.clone());
        self.using_time = behaviour(stack.item()).get_use_duration(&stack);
        self.set_using_item(true);
        if !self.base().is_silent() {
            let pitch = 0.8 + self.base_mut().random.next_float() * 0.4;
            self.play_sound(level, SoundEvent::ENTITY_WITCH_DRINK, 1.0, pitch);
        }
        if let Some(speed) = self
            .mob
            .living
            .attributes
            .get_instance(mc_data::Attribute::MOVEMENT_SPEED)
        {
            speed.remove_modifier(&SPEED_MODIFIER_DRINKING_ID);
            speed.add_transient_modifier(speed_modifier_drinking());
        }
    }
}

impl Entity for Witch {
    mc_game_core::mob_entity_accessors!(mob);
    mc_game_core::raider_entity!(
        raider;
        update_no_action_time,
        die,
        hurt_server,
        can_join_patrol,
        remove_when_far_away,
        requires_custom_persistence,
        finalize_spawn,
        pick_up_item,
        nbt,
        after_loaded
    );

    fn get_encode_id(&self) -> Option<&'static str> {
        Some("minecraft:witch")
    }

    fn ai_step(&mut self, level: &mut ServerLevel) {
        if self.is_alive() {
            self.heal_raiders_cooldown.fetch_sub(1, Ordering::Relaxed);
            let cooldown_over = self.heal_raiders_cooldown.load(Ordering::Relaxed) <= 0;
            self.attack_players_can_attack
                .store(cooldown_over, Ordering::Relaxed);
            if self.is_drinking_potion() {
                let remaining = self.using_time;
                self.using_time -= 1;
                if remaining <= 0 {
                    self.finish_drinking(level);
                }
            } else if let Some(potion) = self.choose_potion_to_drink(level) {
                self.start_drinking(level, potion);
            }
            if self.base_mut().random.next_float() < 7.5E-4 {
                broadcast_entity_event(level, self.id(), 15);
            }
        }
        raider::ai_step(self, level);
    }

    fn get_ambient_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_WITCH_AMBIENT)
    }

    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_WITCH_HURT)
    }

    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_WITCH_DEATH)
    }

    fn get_celebrate_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_WITCH_CELEBRATE)
    }

    fn get_damage_after_magic_absorb(
        &mut self,
        level: &mut ServerLevel,
        source: &DamageSource,
        damage: f32,
    ) -> f32 {
        let mut damage =
            damage::get_damage_after_magic_absorb(self.as_dyn_mut(), level, source, damage);
        if source.get_entity() == Some(self.id()) {
            damage = 0.0;
        }
        if source.is_tag(level, DamageTypeTags::WITCH_RESISTANT_TO) {
            damage *= 0.15;
        }
        damage
    }

    fn perform_ranged_attack(&mut self, level: &mut ServerLevel, target: EntityId, _power: f32) {
        if self.is_drinking_potion() {
            return;
        }
        let Some(t) = level.get_entity(target) else {
            return;
        };
        let movement = t.base().delta_movement();
        let xd = t.base().x() + movement.x - self.base().x();
        let yd = t.base().get_eye_y() - f64::from(1.1f32) - self.base().y();
        let zd = t.base().z() + movement.z - self.base().z();
        let dist = (xd * xd + zd * zd).sqrt();
        let is_raider = t.raider().is_some();
        let health = t.get_health();
        let has_slowness = t.has_effect(MobEffect::SLOWNESS);
        let has_poison = t.has_effect(MobEffect::POISON);
        let has_weakness = t.has_effect(MobEffect::WEAKNESS);
        let mut potion = Potion::HARMING;
        if is_raider {
            potion = if health <= 4.0 {
                Potion::HEALING
            } else {
                Potion::REGENERATION
            };
            self.set_target(level, None);
        } else if dist >= 8.0 && !has_slowness {
            potion = Potion::SLOWNESS;
        } else if health >= 8.0 && !has_poison {
            potion = Potion::POISON;
        } else if dist <= 3.0 && !has_weakness && self.base_mut().random.next_float() < 0.25 {
            potion = Potion::WEAKNESS;
        }
        let stack = create_potion_stack(Item::SPLASH_POTION, potion);
        let projectile = ThrownSplashPotion::with_owner(level, &*self, &stack);
        let uncertainty = self.ranged_attack_uncertainty(level);
        spawn_projectile_using_shoot(
            level,
            Box::new(projectile),
            &stack,
            xd,
            yd + dist * 0.2,
            zd,
            if dist <= 2.0 { 0.45 } else { 0.75 },
            uncertainty,
        );
        if !self.base().is_silent() {
            let pitch = 0.8 + self.base_mut().random.next_float() * 0.4;
            self.play_sound(level, SoundEvent::ENTITY_WITCH_THROW, 1.0, pitch);
        }
    }

    fn ranged_attack_uncertainty(&self, _level: &ServerLevel) -> f32 {
        8.0
    }

    fn can_be_leader(&self) -> bool {
        false
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static WITCH: EntityRegistration = EntityRegistration {
    entity_type: EntityType::WITCH,
    create: |ty, id, registries| Box::new(Witch::new(ty, id, registries)),
};

/// Scenario-harness fields of a witch (order 41; the twin of the `Witch` entry of `EntityExtras.java`).
#[distributed_slice(SCENARIO_DUMP_EXTRA)]
static WITCH_DUMP: ScenarioDumpExtra = ScenarioDumpExtra {
    order: 41,
    applies: |e| e.as_any().is::<Witch>(),
    fields: |e, _level, _ctx| {
        let witch = e.as_any().downcast_ref::<Witch>().expect("witch");
        vec![
            (
                "drinking".into(),
                if witch.is_drinking_potion() { "1" } else { "0" }.into(),
            ),
            ("usingtime".into(), witch.using_time.to_string()),
        ]
    },
};
