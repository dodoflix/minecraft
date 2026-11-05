//! `ZombieHorse`: eats rotten flesh, spawns with a zombie jockey, mob-controlled when ridden by a mob.

use super::abstract_horse::{self, AbstractHorse, AbstractHorseData, DATA_ID_FLAGS};
use crate::registry::RegistryAccess;
use crate::server::level::ServerLevel;
use crate::world::damagesource::DamageSource;
use crate::world::difficulty_instance::DifficultyInstance;
use crate::world::entity::ai::goal::{FloatGoal, TemptGoal};
use crate::world::entity::ai::navigation::StandardPathNavigation;
use crate::world::entity::dimensions::{EntityAttachments, EntityDimensions};
use crate::world::entity::equipment::EquipmentSlot;
use crate::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration, EntitySpawnRequest};
use crate::world::entity::mob::{self, MobData, MobKind};
use crate::world::entity::spawn_group_data::SpawnGroupData;
use crate::world::entity::{Entity, EntityId, ServerPlayer};
use crate::world::item::{InteractionHand, InteractionResult, ItemStack};
use crate::world::level::server_level_accessor::ServerLevelAccessor;
use linkme::distributed_slice;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::enums::{EntitySpawnReason, Pose};
use mc_data::{Attribute, EntityType, Item, SoundEvent};
use std::sync::Arc;

const SPEED_FACTOR: f32 = 42.16;

/// `BABY_DIMENSIONS`.
fn baby_dimensions() -> EntityDimensions {
    let ty = EntityType::ZOMBIE_HORSE;
    let base = EntityDimensions::of_type(ty);
    let attachments = EntityAttachments::create_default(base.width, base.height).with_passenger(Vec3::new(0.0, f64::from(ty.data().height - 0.25), 0.0));
    base.with_attachments(attachments).scale(0.7)
}

/// `generateZombieHorseJumpStrength(probabilityProvider)`.
fn generate_zombie_horse_jump_strength(mut probability: impl FnMut() -> f64) -> f64 {
    let a = probability();
    let b = probability();
    let c = probability();
    0.5 + a * 0.06666666666666667 + b * 0.06666666666666667 + c * 0.06666666666666667
}

/// `generateZombieHorseSpeed(probabilityProvider)`.
fn generate_zombie_horse_speed(mut probability: impl FnMut() -> f64) -> f64 {
    let a = probability();
    let b = probability();
    let c = probability();
    (9.0 + a * 1.0 + b * 1.0 + c * 1.0) / f64::from(SPEED_FACTOR)
}

/// `ZombieHorse`.
pub struct ZombieHorse {
    pub mob: MobData,
    pub horse: AbstractHorseData,
}

impl ZombieHorse {
    /// `new ZombieHorse(type, level)`.
    pub fn new(ty: EntityType, id: EntityId, _registries: &RegistryAccess) -> Self {
        let mob = MobData::new(ty, id, MobKind::Animal, |b| {
            b.define(DATA_ID_FLAGS, 0i8);
        });
        let mut horse = ZombieHorse { mob, horse: AbstractHorseData::new() };
        let navigation = StandardPathNavigation::ground(&horse);
        horse.mob.set_navigation(Box::new(navigation));
        abstract_horse::register_goals(&mut horse, |horse| {
            let float = FloatGoal::new(horse);
            let goals = &mut horse.mob.goal_selector;
            goals.add_goal(0, Box::new(float));
            goals.add_goal(3, Box::new(TemptGoal::new(1.25, Arc::new(|stack: &ItemStack| super::item_in_tag(stack, "zombie_horse_food")), false)));
        });
        horse.create_inventory();
        horse
    }
}

impl AbstractHorse for ZombieHorse {
    fn horse(&self) -> &AbstractHorseData {
        &self.horse
    }
    fn horse_mut(&mut self) -> &mut AbstractHorseData {
        &mut self.horse
    }
    fn get_eating_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_ZOMBIE_HORSE_EAT)
    }
    fn get_angry_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_ZOMBIE_HORSE_ANGRY)
    }
    fn is_mob_controlled(&self) -> bool {
        self.horse.first_passenger_is_mob
    }
    fn randomize_attributes(&mut self, random: &mut dyn RandomSource) {
        let jump = generate_zombie_horse_jump_strength(|| random.next_double());
        let speed = generate_zombie_horse_speed(|| random.next_double());
        let attributes = &mut self.mob.living.attributes;
        if let Some(i) = attributes.get_instance(Attribute::JUMP_STRENGTH) {
            i.set_base_value(jump);
        }
        if let Some(i) = attributes.get_instance(Attribute::MOVEMENT_SPEED) {
            i.set_base_value(speed);
        }
    }
}

impl Entity for ZombieHorse {
    crate::mob_entity_accessors!(mob);
    crate::abstract_horse_entity!();

    fn get_encode_id(&self) -> Option<&'static str> {
        Some("minecraft:zombie_horse")
    }

    fn get_dimensions(&self, _pose: Pose) -> EntityDimensions {
        if self.is_baby() { baby_dimensions() } else { EntityDimensions::of_type(self.mob.living.entity.entity_type) }
    }

    fn interact(&mut self, level: &mut ServerLevel, player: &mut ServerPlayer, hand: InteractionHand, location: Vec3) -> InteractionResult {
        self.mob.persistence_required = true;
        mob::interact::interact(self, level, player, hand, location)
    }

    fn remove_when_far_away(&self, _dist_sqr: f64) -> bool {
        true
    }

    fn ai_step(&mut self, level: &mut ServerLevel) {
        abstract_horse::ai_step(self, level);
    }

    fn drop_equipment(&mut self, level: &mut ServerLevel) {
        abstract_horse::drop_equipment(self, level);
    }

    fn can_use_slot(&self, _slot: EquipmentSlot) -> bool {
        true
    }

    fn is_immobile(&self) -> bool {
        abstract_horse::is_immobile(self)
    }

    fn cause_fall_damage(&mut self, level: &mut ServerLevel, fall_distance: f64, damage_modifier: f32, source: &DamageSource) -> bool {
        abstract_horse::cause_fall_damage(self, level, fall_distance, damage_modifier, source)
    }

    fn play_step_sound(&mut self, level: &mut ServerLevel, pos: BlockPos, state: mc_data::block::BlockState) {
        abstract_horse::play_step_sound(self, level, pos, state);
    }

    fn get_passenger_attachment_point(&self, passenger: &dyn Entity, dimensions: &EntityDimensions, scale: f32) -> Vec3 {
        let base = crate::world::entity::entity::default_passenger_attachment_point(self.as_dyn(), passenger, &dimensions.attachments);
        abstract_horse::get_passenger_attachment_point(self, base, scale)
    }

    fn is_food(&self, stack: &ItemStack) -> bool {
        super::item_in_tag(stack, "zombie_horse_food")
    }

    fn get_ambient_sound(&mut self, _level: &ServerLevel) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_ZOMBIE_HORSE_AMBIENT)
    }

    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_ZOMBIE_HORSE_DEATH)
    }

    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_ZOMBIE_HORSE_HURT)
    }

    fn mob_interact(&mut self, level: &mut ServerLevel, player: &mut ServerPlayer, hand: InteractionHand) -> InteractionResult {
        let should_open_inventory = !self.is_baby() && abstract_horse::is_tamed(self) && player.is_shift_key_down();
        if !self.base().is_vehicle() && !should_open_inventory {
            let item_stack = player.get_item_in_hand(hand);
            if !item_stack.is_empty() {
                if self.is_food(&item_stack) {
                    return abstract_horse::fed_food(self, level, player, hand);
                }
                if !abstract_horse::is_tamed(self) {
                    abstract_horse::make_mad(self, level);
                    return InteractionResult::SUCCESS;
                }
            }
            abstract_horse::mob_interact(self, level, player, hand)
        } else {
            abstract_horse::mob_interact(self, level, player, hand)
        }
    }

    fn can_mate(&self, _partner: &dyn Entity) -> bool {
        false
    }

    fn can_fall_in_love(&self) -> bool {
        false
    }

    fn can_age_up(&self) -> bool {
        false
    }

    fn can_be_leashed(&self) -> bool {
        abstract_horse::is_tamed(self) || !self.is_mob_controlled()
    }

    fn sun_protection_slot(&self) -> EquipmentSlot {
        EquipmentSlot::Body
    }

    fn charge_speed_modifier(&self) -> f32 {
        1.4
    }

    fn get_breed_offspring(&mut self, level: &mut ServerLevel, _partner: EntityId) -> Option<Box<dyn Entity>> {
        crate::world::entity::factory::create(EntityType::ZOMBIE_HORSE, level, EntitySpawnRequest { reason: EntitySpawnReason::Breeding, ignore_checks: false })
    }

    fn finalize_spawn(
        &mut self,
        level: &mut dyn ServerLevelAccessor,
        difficulty: DifficultyInstance,
        reason: EntitySpawnReason,
        group_data: Option<Box<dyn SpawnGroupData>>,
    ) -> Option<Box<dyn SpawnGroupData>> {
        if reason == EntitySpawnReason::Natural {
            if let Some(mut zombie) = level.create_entity(EntityType::ZOMBIE, EntitySpawnRequest { reason: EntitySpawnReason::Jockey, ignore_checks: false }) {
                let (p, y_rot) = (self.position(), self.base().y_rot());
                zombie.snap_to(p.x, p.y, p.z, y_rot, 0.0);
                zombie.finalize_spawn(level, difficulty.clone(), reason, None);
                mob::equipment::set_item_slot_spawn(zombie.as_mut(), EquipmentSlot::MainHand, ItemStack::new(Item::IRON_SPEAR));
                crate::world::entity::monster::zombie::start_riding_fresh_vehicle(zombie.as_mut(), self, false, false, level);
                level.add_fresh_entity_with_passengers(zombie);
            }
        }
        let group_data = abstract_horse::finalize_spawn_prepare(self, level, group_data);
        mob::spawn::finalize_spawn(self, level, difficulty, reason, group_data)
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static ZOMBIE_HORSE: EntityRegistration =
    EntityRegistration { entity_type: EntityType::ZOMBIE_HORSE, create: |ty, id, registries| Box::new(ZombieHorse::new(ty, id, registries)) };
