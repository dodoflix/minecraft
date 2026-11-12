//! `Donkey`.

use super::abstract_chested_horse::{self, AbstractChestedHorse, DATA_ID_CHEST};
use linkme::distributed_slice;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::enums::{EntitySpawnReason, Pose};
use mc_data::{EntityType, SoundEvent};
use mc_game_core::registry::RegistryAccess;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::damagesource::DamageSource;
use mc_game_core::world::difficulty_instance::DifficultyInstance;
use mc_game_core::world::entity::ai::navigation::StandardPathNavigation;
use mc_game_core::world::entity::animal::equine::abstract_horse::{
    self, AbstractHorse, AbstractHorseData, DATA_ID_FLAGS,
};
use mc_game_core::world::entity::dimensions::EntityDimensions;
use mc_game_core::world::entity::equipment::EquipmentSlot;
use mc_game_core::world::entity::factory::{
    ENTITY_FACTORIES, EntityRegistration, EntitySpawnRequest,
};
use mc_game_core::world::entity::mob::{self, MobData, MobKind};
use mc_game_core::world::entity::spawn_group_data::SpawnGroupData;
use mc_game_core::world::entity::{Entity, EntityId, ServerPlayer};
use mc_game_core::world::item::{InteractionHand, InteractionResult, ItemStack};
use mc_game_core::world::level::server_level_accessor::ServerLevelAccessor;
use mc_nbt::CompoundTag;

/// `Donkey`.
pub struct Donkey {
    pub mob: MobData,
    pub horse: AbstractHorseData,
    baby_dimensions: EntityDimensions,
}

impl Donkey {
    /// `new Donkey(type, level)`.
    pub fn new(ty: EntityType, id: EntityId, _registries: &RegistryAccess) -> Self {
        let mob = MobData::new(ty, id, MobKind::Animal, |b| {
            b.define(DATA_ID_FLAGS, 0i8);
            b.define(DATA_ID_CHEST, false);
        });
        let mut donkey = Donkey {
            mob,
            horse: AbstractHorseData::new(),
            baby_dimensions: abstract_chested_horse::baby_dimensions(ty),
        };
        let navigation = StandardPathNavigation::ground(&donkey);
        donkey.mob.set_navigation(Box::new(navigation));
        abstract_horse::register_goals(&mut donkey, abstract_horse::add_behaviour_goals);
        donkey.create_inventory();
        donkey.horse.can_gallop = false;
        donkey
    }
}

impl AbstractHorse for Donkey {
    fn horse(&self) -> &AbstractHorseData {
        &self.horse
    }
    fn horse_mut(&mut self) -> &mut AbstractHorseData {
        &mut self.horse
    }
    fn get_eating_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_DONKEY_EAT)
    }
    fn get_angry_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_DONKEY_ANGRY)
    }
    fn get_inventory_columns(&self) -> i32 {
        abstract_chested_horse::chested_columns(self)
    }
    fn randomize_attributes(&mut self, random: &mut dyn RandomSource) {
        abstract_chested_horse::randomize_attributes(self, random);
    }
    fn play_jump_sound(&mut self, level: &mut ServerLevel) {
        self.play_sound(level, SoundEvent::ENTITY_DONKEY_JUMP, 0.4, 1.0);
    }
}

impl AbstractChestedHorse for Donkey {}

impl Entity for Donkey {
    mc_game_core::mob_entity_accessors!(mob);
    mc_game_core::abstract_horse_entity!();

    fn get_encode_id(&self) -> Option<&'static str> {
        Some("minecraft:donkey")
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
    }

    fn drop_equipment(&mut self, level: &mut ServerLevel) {
        abstract_chested_horse::drop_equipment(self, level);
    }

    fn chested_horse_has_chest(&self) -> Option<bool> {
        Some(abstract_chested_horse::has_chest(self))
    }

    fn chested_horse_set_chest_slot(&mut self, stack: &ItemStack) -> bool {
        abstract_chested_horse::chest_slot_set(self, stack)
    }

    fn can_use_slot(&self, slot: EquipmentSlot) -> bool {
        abstract_horse::can_use_slot(self, slot)
    }

    fn is_immobile(&self) -> bool {
        abstract_horse::is_immobile(self)
    }

    fn cause_fall_damage(
        &mut self,
        level: &mut ServerLevel,
        fall_distance: f64,
        damage_modifier: f32,
        source: &DamageSource,
    ) -> bool {
        abstract_horse::cause_fall_damage(self, level, fall_distance, damage_modifier, source)
    }

    fn play_step_sound(
        &mut self,
        level: &mut ServerLevel,
        pos: BlockPos,
        state: mc_data::block::BlockState,
    ) {
        abstract_horse::play_step_sound(self, level, pos, state);
    }

    fn get_passenger_attachment_point(
        &self,
        passenger: &dyn Entity,
        dimensions: &EntityDimensions,
        scale: f32,
    ) -> Vec3 {
        let base = mc_game_core::world::entity::entity::default_passenger_attachment_point(
            self.as_dyn(),
            passenger,
            &dimensions.attachments,
        );
        abstract_horse::get_passenger_attachment_point(self, base, scale)
    }

    fn is_food(&self, stack: &ItemStack) -> bool {
        mc_game_core::world::entity::animal::equine::item_in_tag(stack, "horse_food")
    }

    fn get_ambient_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_DONKEY_AMBIENT)
    }

    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_DONKEY_DEATH)
    }

    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_DONKEY_HURT)
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
        if partner.id() == self.id() {
            return false;
        }
        let is_donkey_or_horse =
            partner.get_type() == EntityType::HORSE || partner.as_any().is::<Donkey>();
        if !is_donkey_or_horse {
            return false;
        }
        abstract_horse::can_parent(self)
            && partner.as_abstract_horse().is_some_and(|h| h.can_parent())
    }

    fn get_breed_offspring(
        &mut self,
        level: &mut ServerLevel,
        partner: EntityId,
    ) -> Option<Box<dyn Entity>> {
        let baby_type = if level
            .get_entity(partner)
            .is_some_and(|p| p.get_type() == EntityType::HORSE)
        {
            EntityType::MULE
        } else {
            EntityType::DONKEY
        };
        let mut baby = mc_game_core::world::entity::factory::create(
            baby_type,
            level,
            EntitySpawnRequest {
                reason: EntitySpawnReason::Breeding,
                ignore_checks: false,
            },
        )?;
        abstract_horse::set_offspring_attributes(self, level, partner, baby.as_mut());
        Some(baby)
    }

    fn finalize_spawn(
        &mut self,
        level: &mut dyn ServerLevelAccessor,
        difficulty: DifficultyInstance,
        reason: EntitySpawnReason,
        group_data: Option<Box<dyn SpawnGroupData>>,
    ) -> Option<Box<dyn SpawnGroupData>> {
        let group_data = abstract_horse::finalize_spawn_prepare(self, level, group_data);
        mob::spawn::finalize_spawn(self, level, difficulty, reason, group_data)
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        abstract_chested_horse::add_additional_save_data(self, out);
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        abstract_chested_horse::read_additional_save_data(self, input);
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static DONKEY: EntityRegistration = EntityRegistration {
    entity_type: EntityType::DONKEY,
    create: |ty, id, registries| Box::new(Donkey::new(ty, id, registries)),
};
