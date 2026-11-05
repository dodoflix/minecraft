//! `Horse`: variants + markings, breeding (mule when crossed with a donkey).

use super::abstract_horse::{self, AbstractHorse, AbstractHorseData, DATA_ID_FLAGS};
use super::markings::Markings;
use crate::define_accessor;
use crate::registry::RegistryAccess;
use crate::server::level::ServerLevel;
use crate::world::damagesource::DamageSource;
use crate::world::difficulty_instance::DifficultyInstance;
use crate::world::entity::dimensions::{EntityAttachments, EntityDimensions};
use crate::world::entity::equipment::EquipmentSlot;
use crate::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration, EntitySpawnRequest};
use crate::world::entity::mob::spawn::AgeableMobGroupData;
use crate::world::entity::mob::{self, MobData, MobKind};
use crate::world::entity::spawn_group_data::SpawnGroupData;
use crate::world::entity::ai::navigation::StandardPathNavigation;
use crate::world::entity::{Entity, EntityId, ServerPlayer};
use crate::world::item::{InteractionHand, InteractionResult, ItemStack};
use crate::world::level::server_level_accessor::ServerLevelAccessor;
use linkme::distributed_slice;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::SoundType;
use mc_data::enums::{EntitySpawnReason, Pose};
use mc_data::{Attribute, EntityType, Item, SoundEvent};
use mc_nbt::CompoundTag;
use mc_protocol::components::{DataComponentMap, HORSE_VARIANT, HorseVariant};

define_accessor!(pub DATA_ID_TYPE_VARIANT: Int = Horse);

/// `BABY_DIMENSIONS`.
fn baby_dimensions() -> EntityDimensions {
    let ty = EntityType::HORSE;
    let base = EntityDimensions::of_type(ty);
    let attachments = EntityAttachments::create_default(base.width, base.height).with_passenger(Vec3::new(0.0, f64::from(ty.data().height - 0.125), 0.0));
    base.with_attachments(attachments).scale(0.7)
}

/// `Horse.HorseGroupData`.
pub struct HorseGroupData {
    base: AgeableMobGroupData,
    pub variant: HorseVariant,
}

impl SpawnGroupData for HorseGroupData {
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

/// `Horse`.
pub struct Horse {
    pub mob: MobData,
    pub horse: AbstractHorseData,
}

impl Horse {
    /// `new Horse(type, level)`.
    pub fn new(ty: EntityType, id: EntityId, _registries: &RegistryAccess) -> Self {
        let mob = MobData::new(ty, id, MobKind::Animal, |b| {
            b.define(DATA_ID_FLAGS, 0i8);
            b.define(DATA_ID_TYPE_VARIANT, 0);
        });
        let mut horse = Horse { mob, horse: AbstractHorseData::new() };
        let navigation = StandardPathNavigation::ground(&horse);
        horse.mob.set_navigation(Box::new(navigation));
        abstract_horse::register_goals(&mut horse, abstract_horse::add_behaviour_goals);
        horse.create_inventory();
        horse
    }

    fn type_variant(&self) -> i32 {
        self.mob.living.entity.entity_data.get(DATA_ID_TYPE_VARIANT)
    }

    fn set_type_variant(&mut self, value: i32) {
        self.mob.living.entity.entity_data.set(DATA_ID_TYPE_VARIANT, value);
    }

    /// `setVariantAndMarkings(variant, markings)`.
    pub fn set_variant_and_markings(&mut self, variant: HorseVariant, markings: Markings) {
        self.set_type_variant((variant.id() & 0xFF) | ((markings.id() << 8) & 0xFF00));
    }

    /// `getVariant()`.
    pub fn get_variant(&self) -> HorseVariant {
        HorseVariant::ALL[(self.type_variant() & 0xFF).rem_euclid(HorseVariant::ALL.len() as i32) as usize]
    }

    /// `setVariant(variant)`.
    pub fn set_variant(&mut self, variant: HorseVariant) {
        let current = self.type_variant();
        self.set_type_variant((variant.id() & 0xFF) | (current & -256));
    }

    /// `getMarkings()`.
    pub fn get_markings(&self) -> Markings {
        Markings::by_id((self.type_variant() & 0xFF00) >> 8)
    }
}

impl AbstractHorse for Horse {
    fn horse(&self) -> &AbstractHorseData {
        &self.horse
    }
    fn horse_mut(&mut self) -> &mut AbstractHorseData {
        &mut self.horse
    }

    fn get_eating_sound(&self) -> Option<SoundEvent> {
        Some(if self.is_baby() { SoundEvent::ENTITY_BABY_HORSE_EAT } else { SoundEvent::ENTITY_HORSE_EAT })
    }

    fn get_angry_sound(&self) -> Option<SoundEvent> {
        Some(if self.is_baby() { SoundEvent::ENTITY_BABY_HORSE_ANGRY } else { SoundEvent::ENTITY_HORSE_ANGRY })
    }

    fn randomize_attributes(&mut self, random: &mut dyn RandomSource) {
        let health = f64::from(abstract_horse::generate_max_health(|bound| random.next_int_bounded(bound)));
        let speed = abstract_horse::generate_speed(|| random.next_double());
        let jump = abstract_horse::generate_jump_strength(|| random.next_double());
        let attributes = &mut self.mob.living.attributes;
        if let Some(i) = attributes.get_instance(Attribute::MAX_HEALTH) {
            i.set_base_value(health);
        }
        if let Some(i) = attributes.get_instance(Attribute::MOVEMENT_SPEED) {
            i.set_base_value(speed);
        }
        if let Some(i) = attributes.get_instance(Attribute::JUMP_STRENGTH) {
            i.set_base_value(jump);
        }
    }

    fn play_gallop_sound(&mut self, level: &mut ServerLevel, sound_type: &SoundType) {
        self.play_sound(level, SoundEvent::ENTITY_HORSE_GALLOP, sound_type.volume * 0.15, sound_type.pitch);
        if self.base_mut().random.next_int_bounded(10) == 0 {
            let breathe = if self.is_baby() { SoundEvent::ENTITY_BABY_HORSE_BREATHE } else { SoundEvent::ENTITY_HORSE_BREATHE };
            self.play_sound(level, breathe, sound_type.volume * 0.6, sound_type.pitch);
        }
    }
}

impl Entity for Horse {
    crate::mob_entity_accessors!(mob);
    crate::abstract_horse_entity!();

    fn get_encode_id(&self) -> Option<&'static str> {
        Some("minecraft:horse")
    }

    fn get_dimensions(&self, _pose: Pose) -> EntityDimensions {
        if self.is_baby() { baby_dimensions() } else { EntityDimensions::of_type(self.mob.living.entity.entity_type) }
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
        super::item_in_tag(stack, "horse_food")
    }

    fn get_ambient_sound(&self) -> Option<SoundEvent> {
        Some(if self.is_baby() { SoundEvent::ENTITY_BABY_HORSE_AMBIENT } else { SoundEvent::ENTITY_HORSE_AMBIENT })
    }

    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(if self.is_baby() { SoundEvent::ENTITY_BABY_HORSE_DEATH } else { SoundEvent::ENTITY_HORSE_DEATH })
    }

    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        Some(if self.is_baby() { SoundEvent::ENTITY_BABY_HORSE_HURT } else { SoundEvent::ENTITY_HORSE_HURT })
    }

    fn mob_interact(&mut self, level: &mut ServerLevel, player: &mut ServerPlayer, hand: InteractionHand) -> InteractionResult {
        let should_open_inventory = !self.is_baby() && abstract_horse::is_tamed(self) && player.is_shift_key_down();
        if !self.base().is_vehicle() && !should_open_inventory && (!self.is_baby() || !abstract_horse::is_holding(player, Item::GOLDEN_DANDELION)) {
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

    fn can_mate(&self, partner: &dyn Entity) -> bool {
        if partner.id() == self.id() {
            return false;
        }
        let is_donkey_or_horse = partner.get_type() == EntityType::DONKEY || partner.as_any().is::<Horse>();
        if !is_donkey_or_horse {
            return false;
        }
        abstract_horse::can_parent(self) && partner.as_abstract_horse().is_some_and(|h| h.can_parent())
    }

    fn get_breed_offspring(&mut self, level: &mut ServerLevel, partner: EntityId) -> Option<Box<dyn Entity>> {
        let partner_is_donkey = level.get_entity(partner).is_some_and(|p| p.get_type() == EntityType::DONKEY);
        if partner_is_donkey {
            let mut baby = crate::world::entity::factory::create(EntityType::MULE, level, EntitySpawnRequest { reason: EntitySpawnReason::Breeding, ignore_checks: false })?;
            abstract_horse::set_offspring_attributes(self, level, partner, baby.as_mut());
            return Some(baby);
        }
        let mut baby = crate::world::entity::factory::create(EntityType::HORSE, level, EntitySpawnRequest { reason: EntitySpawnReason::Breeding, ignore_checks: false })?;
        let (partner_variant, partner_markings) = level.get_entity(partner).and_then(|p| p.as_any().downcast_ref::<Horse>()).map(|p| (p.get_variant(), p.get_markings()))?;
        let select_skin = self.base_mut().random.next_int_bounded(9);
        let variant = if select_skin < 4 {
            self.get_variant()
        } else if select_skin < 8 {
            partner_variant
        } else {
            let n = self.base_mut().random.next_int_bounded(HorseVariant::ALL.len() as i32);
            HorseVariant::ALL[n as usize]
        };
        let select_marking = self.base_mut().random.next_int_bounded(5);
        let markings = if select_marking < 2 {
            self.get_markings()
        } else if select_marking < 4 {
            partner_markings
        } else {
            let n = self.base_mut().random.next_int_bounded(Markings::ALL.len() as i32);
            Markings::ALL[n as usize]
        };
        if let Some(horse) = baby.as_any_mut().downcast_mut::<Horse>() {
            horse.set_variant_and_markings(variant, markings);
        }
        abstract_horse::set_offspring_attributes(self, level, partner, baby.as_mut());
        Some(baby)
    }

    fn hurt_armor(&mut self, level: &mut ServerLevel, source: &DamageSource, damage: f32) {
        crate::world::entity::living_entity::damage::do_hurt_equipment(self.as_dyn_mut(), level, source, damage, &[EquipmentSlot::Body]);
    }

    fn finalize_spawn(
        &mut self,
        level: &mut dyn ServerLevelAccessor,
        difficulty: DifficultyInstance,
        reason: EntitySpawnReason,
        group_data: Option<Box<dyn SpawnGroupData>>,
    ) -> Option<Box<dyn SpawnGroupData>> {
        let variant;
        let group_data = match group_data {
            Some(data) if data.as_any().is::<HorseGroupData>() => {
                variant = data.as_any().downcast_ref::<HorseGroupData>().expect("horse group data").variant;
                data
            }
            _ => {
                let n = level.random().next_int_bounded(HorseVariant::ALL.len() as i32);
                variant = HorseVariant::ALL[n as usize];
                Box::new(HorseGroupData { base: AgeableMobGroupData::with_baby(true), variant }) as Box<dyn SpawnGroupData>
            }
        };
        let m = level.random().next_int_bounded(Markings::ALL.len() as i32);
        self.set_variant_and_markings(variant, Markings::ALL[m as usize]);
        let group_data = abstract_horse::finalize_spawn_prepare(self, level, Some(group_data));
        mob::spawn::finalize_spawn(self, level, difficulty, reason, group_data)
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        abstract_horse::add_additional_save_data(self, out);
        out.put_int("Variant", self.type_variant());
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        abstract_horse::read_additional_save_data(self, input);
        let variant = input.get_int_or("Variant", 0);
        self.set_type_variant(variant);
    }

    fn entity_components(&self) -> DataComponentMap {
        let base = crate::advancements::predicates::entity::support::base_entity_components(self.base());
        DataComponentMap::builder().add_all(&base).set(HORSE_VARIANT, self.get_variant()).build()
    }

    fn apply_implicit_components(&mut self, stack: &ItemStack) {
        if let Some(variant) = stack.get(&HORSE_VARIANT) {
            self.set_variant(*variant);
        }
        crate::world::entity::spawn_config::apply_base_implicit_components(self, stack);
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static HORSE: EntityRegistration = EntityRegistration { entity_type: EntityType::HORSE, create: |ty, id, registries| Box::new(Horse::new(ty, id, registries)) };
