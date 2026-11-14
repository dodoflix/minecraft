//! `Illusioner`: the bow-and-spell illager (blindness, mirror image invisibility).
//! Not implemented: the client-side mirror image offsets (`aiStep` runs only on the client).

use super::super::skeleton::goals::get_mob_projectile;
use linkme::distributed_slice;
use mc_core::random::RandomSource;
use mc_data::enums::EntitySpawnReason;
use mc_data::{EntityType, Item, MobEffect, SoundEvent};
use mc_game_ai::world::entity::ai::goal::{AvoidEntityGoal, RangedBowAttackGoal};
use mc_game_core::registry::RegistryAccess;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::damagesource::DamageSource;
use mc_game_core::world::difficulty_instance::DifficultyInstance;
use mc_game_core::world::entity::ai::goal::look_at_player::LookAtType;
use mc_game_core::world::entity::ai::goal::target::{
    HurtByTargetGoal, NearestAttackableTargetGoal, TargetType, class_of_type, class_of_types,
};
use mc_game_core::world::entity::ai::goal::{FloatGoal, LookAtPlayerGoal, RandomStrollGoal};
use mc_game_core::world::entity::ai::navigation::StandardPathNavigation;
use mc_game_core::world::entity::effect::MobEffectInstance;
use mc_game_core::world::entity::equipment::EquipmentSlot;
use mc_game_core::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use mc_game_core::world::entity::mob::{self, MobData, MobKind};
use mc_game_core::world::entity::monster::illager::abstract_illager;
use mc_game_core::world::entity::monster::illager::spellcaster_illager::{
    self, IllagerSpell, SpellcasterCastingSpellGoal, SpellcasterData, SpellcasterUseSpellGoal,
    UseSpell, UseSpellState,
};
use mc_game_core::world::entity::projectile::spawn::spawn_projectile_using_shoot;
use mc_game_core::world::entity::raid::raid::Raid;
use mc_game_core::world::entity::raid::raider::{self, RaiderData};
use mc_game_core::world::entity::spawn_group_data::SpawnGroupData;
use mc_game_core::world::entity::{Entity, EntityId};
use mc_game_core::world::item::projectile_weapon_item::{get_mob_arrow, get_weapon_holding_hand};
use mc_game_core::world::item::{InteractionHand, ItemStack};
use mc_game_core::world::level::server_level_accessor::ServerLevelAccessor;
use mc_nbt::CompoundTag;
use mc_world::level_storage::Difficulty;
use std::sync::Arc;

pub struct Illusioner {
    pub mob: MobData,
    raider: RaiderData,
    spellcaster: SpellcasterData,
}

impl Illusioner {
    /// `new Illusioner(type, level)`: `registerGoals()`, the ground navigation and `xpReward = 5`.
    pub fn new(ty: EntityType, id: EntityId, _registries: &RegistryAccess) -> Self {
        let mob = MobData::new(ty, id, MobKind::Monster, |b| {
            raider::define_synched_data(b);
            spellcaster_illager::define_synched_data(b);
        });
        let mut illusioner = Illusioner {
            mob,
            raider: RaiderData::default(),
            spellcaster: SpellcasterData::new(SoundEvent::ENTITY_ILLUSIONER_CAST_SPELL),
        };
        let navigation = StandardPathNavigation::ground(&illusioner);
        illusioner.mob.set_navigation(Box::new(navigation));
        illusioner.mob.xp_reward = 5;
        illusioner.register_goals();
        illusioner
    }

    /// `registerGoals()`.
    fn register_goals(&mut self) {
        raider::register_goals(&mut self.mob);
        let float = FloatGoal::new(self);
        let look_at_player = LookAtPlayerGoal::with_probability(self, LookAtType::Player, 3.0, 1.0);
        let look_at_mob = LookAtPlayerGoal::new(
            self,
            LookAtType::Class(Arc::new(|e: &dyn Entity| e.mob().is_some())),
            8.0,
        );
        let sel = &mut self.mob.goal_selector;
        sel.add_goal(0, Box::new(float));
        sel.add_goal(1, Box::new(SpellcasterCastingSpellGoal::new()));
        sel.add_goal(
            3,
            Box::new(AvoidEntityGoal::new(
                Arc::new(|e: &dyn Entity| e.get_type() == EntityType::CREAKING),
                8.0,
                1.0,
                1.2,
            )),
        );
        sel.add_goal(
            4,
            Box::new(SpellcasterUseSpellGoal::new(IllusionerMirrorSpell)),
        );
        sel.add_goal(
            5,
            Box::new(SpellcasterUseSpellGoal::new(IllusionerBlindnessSpell {
                last_target_id: None,
            })),
        );
        sel.add_goal(6, Box::new(RangedBowAttackGoal::new(0.5, 20, 15.0)));
        sel.add_goal(8, Box::new(RandomStrollGoal::new(0.6)));
        sel.add_goal(9, Box::new(look_at_player));
        sel.add_goal(10, Box::new(look_at_mob));
        let t = &mut self.mob.target_selector;
        t.add_goal(
            1,
            Box::new(
                HurtByTargetGoal::new(vec![Arc::new(|e: &dyn Entity| e.raider().is_some())])
                    .set_alert_others(Vec::new()),
            ),
        );
        t.add_goal(
            2,
            Box::new(
                NearestAttackableTargetGoal::new(TargetType::Player, true)
                    .set_unseen_memory_ticks(300),
            ),
        );
        t.add_goal(
            3,
            Box::new(
                NearestAttackableTargetGoal::new(
                    TargetType::Class(class_of_types(&[
                        EntityType::VILLAGER,
                        EntityType::WANDERING_TRADER,
                    ])),
                    false,
                )
                .set_unseen_memory_ticks(300),
            ),
        );
        t.add_goal(
            3,
            Box::new(
                NearestAttackableTargetGoal::new(
                    TargetType::Class(class_of_type(EntityType::IRON_GOLEM)),
                    false,
                )
                .set_unseen_memory_ticks(300),
            ),
        );
    }
}

impl Entity for Illusioner {
    mc_game_core::mob_entity_accessors!(mob);
    mc_game_core::raider_entity!(
        raider;
        ai_step,
        update_no_action_time,
        die,
        hurt_server,
        can_join_patrol,
        remove_when_far_away,
        requires_custom_persistence,
        pick_up_item,
        after_loaded
    );

    fn spellcaster(&self) -> Option<&SpellcasterData> {
        Some(&self.spellcaster)
    }

    fn spellcaster_mut(&mut self) -> Option<&mut SpellcasterData> {
        Some(&mut self.spellcaster)
    }

    fn get_encode_id(&self) -> Option<&'static str> {
        Some("minecraft:illusioner")
    }

    fn custom_server_ai_step(&mut self, _level: &mut ServerLevel) {
        spellcaster_illager::custom_server_ai_step(self);
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        spellcaster_illager::add_additional_save_data(self, out);
    }

    fn read_additional_save_data(&mut self, level: Option<&ServerLevel>, input: &CompoundTag) {
        spellcaster_illager::read_additional_save_data(self, level, input);
    }

    fn can_attack(&self, level: &ServerLevel, target: &dyn Entity) -> bool {
        abstract_illager::can_attack(self, level, target)
    }

    fn considers_entity_as_ally(&self, level: &ServerLevel, other: &dyn Entity) -> bool {
        abstract_illager::considers_entity_as_ally(self, level, other)
    }

    fn finalize_spawn(
        &mut self,
        level: &mut dyn ServerLevelAccessor,
        difficulty: DifficultyInstance,
        reason: EntitySpawnReason,
        group_data: Option<Box<dyn SpawnGroupData>>,
    ) -> Option<Box<dyn SpawnGroupData>> {
        mob::equipment::set_item_slot_spawn(
            self,
            EquipmentSlot::MainHand,
            ItemStack::new(Item::BOW),
        );
        raider::finalize_spawn(self, level, difficulty, reason, group_data)
    }

    fn get_ambient_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_ILLUSIONER_AMBIENT)
    }

    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_ILLUSIONER_DEATH)
    }

    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_ILLUSIONER_HURT)
    }

    fn get_celebrate_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_ILLUSIONER_AMBIENT)
    }

    fn apply_raid_buffs(
        &mut self,
        _level: &mut ServerLevel,
        _wave: i32,
        _is_captain: bool,
        _raid: &Raid,
    ) {
    }

    fn perform_ranged_attack(&mut self, level: &mut ServerLevel, target: EntityId, power: f32) {
        let bow_slot = if get_weapon_holding_hand(self, Item::BOW) == InteractionHand::MainHand {
            EquipmentSlot::MainHand
        } else {
            EquipmentSlot::OffHand
        };
        let bow_item = self.get_item_by_slot(bow_slot);
        let projectile = get_mob_projectile(self);
        let arrow = get_mob_arrow(level, self, &projectile, power, Some(&bow_item));
        let Some((tx, tz, ty_third)) = level.get_entity(target).map(|t| {
            (
                t.base().x(),
                t.base().z(),
                t.base().get_y_progress(0.3333333333333333),
            )
        }) else {
            return;
        };
        let xd = tx - self.base().x();
        let yd = ty_third - arrow.base().y();
        let zd = tz - self.base().z();
        let distance_to_target = (xd * xd + zd * zd).sqrt();
        let uncertainty = self.ranged_attack_uncertainty(level);
        spawn_projectile_using_shoot(
            level,
            arrow,
            &projectile,
            xd,
            yd + distance_to_target * f64::from(0.2f32),
            zd,
            1.6,
            uncertainty,
        );
        let pitch = 1.0 / (self.base_mut().random.next_float() * 0.4 + 0.8);
        self.play_sound(level, SoundEvent::ENTITY_SKELETON_SHOOT, 1.0, pitch);
    }
}

/// `Illusioner.IllusionerBlindnessSpellGoal`.
struct IllusionerBlindnessSpell {
    last_target_id: Option<EntityId>,
}

impl UseSpell for IllusionerBlindnessSpell {
    fn name(&self) -> &'static str {
        "Illusioner$IllusionerBlindnessSpellGoal"
    }

    fn spell(&self) -> IllagerSpell {
        IllagerSpell::Blindness
    }

    fn spell_prepare_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_ILLUSIONER_PREPARE_BLINDNESS)
    }

    fn casting_time(&self) -> i32 {
        20
    }

    fn casting_interval(&self) -> i32 {
        180
    }

    fn can_use(
        &mut self,
        state: &UseSpellState,
        mob: &mut dyn Entity,
        level: &mut ServerLevel,
    ) -> bool {
        if !state.can_use(mob, level) {
            return false;
        }
        match mob::tick::get_target_checked(mob, level) {
            None => false,
            Some(target) if Some(target) == self.last_target_id => false,
            Some(_) => {
                ServerLevelAccessor::get_current_difficulty_at(&*level, mob.block_position())
                    .is_harder_than(Difficulty::Normal.get_id() as f32)
            }
        }
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        if let Some(target) = mob::tick::get_target_checked(mob, level) {
            self.last_target_id = Some(target);
        }
    }

    fn perform_spell_casting(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        let Some(target) = mob::tick::get_target_checked(mob, level) else {
            return;
        };
        let source = mob.id();
        level.with_entity(target, |level, target| {
            target.add_effect(
                level,
                MobEffectInstance::of(MobEffect::BLINDNESS, 400, 0),
                Some(source),
            );
        });
    }
}

/// `Illusioner.IllusionerMirrorSpellGoal`.
struct IllusionerMirrorSpell;

impl UseSpell for IllusionerMirrorSpell {
    fn name(&self) -> &'static str {
        "Illusioner$IllusionerMirrorSpellGoal"
    }

    fn spell(&self) -> IllagerSpell {
        IllagerSpell::Disappear
    }

    fn spell_prepare_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_ILLUSIONER_PREPARE_MIRROR)
    }

    fn casting_time(&self) -> i32 {
        20
    }

    fn casting_interval(&self) -> i32 {
        340
    }

    fn can_use(
        &mut self,
        state: &UseSpellState,
        mob: &mut dyn Entity,
        level: &mut ServerLevel,
    ) -> bool {
        state.can_use(mob, level) && !mob.has_effect(MobEffect::INVISIBILITY)
    }

    fn perform_spell_casting(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        mob.add_effect(
            level,
            MobEffectInstance::of(MobEffect::INVISIBILITY, 1200, 0),
            None,
        );
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static ILLUSIONER: EntityRegistration = EntityRegistration {
    entity_type: EntityType::ILLUSIONER,
    create: |ty, id, registries| Box::new(Illusioner::new(ty, id, registries)),
};
