//! `net.minecraft.world.entity.animal.nautilus.ZombieNautilus`: the undead nautilus a drowned rides; a mount that cannot
//! breed, with a variant by biome and a brain that charges at its enemies.

use super::abstract_nautilus::{
    self, AbstractNautilus, AbstractNautilusData, abstract_nautilus_entity,
    nautilus_mount_inventory,
};
use super::{nautilus_ai, zombie_nautilus_ai, zombie_nautilus_variant::ZombieNautilusVariant};
use linkme::distributed_slice;
use mc_core::ident::Identifier;
use mc_data::enums::EntitySpawnReason;
use mc_data::{EntityType, SoundEvent};
use mc_game_core::define_accessor;
use mc_game_core::registry::RegistryAccess;
use mc_game_core::registry::keys;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::damagesource::DamageSource;
use mc_game_core::world::difficulty_instance::DifficultyInstance;
use mc_game_core::world::entity::ai::brain::{Brain, Provider};
use mc_game_core::world::entity::ai::memory::BrainPacked;
use mc_game_core::world::entity::ai::memory::memory_module_type::{
    ANGRY_AT, ATTACK_TARGET_COOLDOWN,
};
use mc_game_core::world::entity::ai::sensing::sensor_type::{
    HURT_BY, NAUTILUS_TEMPTATIONS, NEAREST_ADULT, NEAREST_LIVING_ENTITIES, NEAREST_PLAYERS,
};
use mc_game_core::world::entity::equipment::EquipmentSlot;
use mc_game_core::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use mc_game_core::world::entity::living_entity::damage::make_sound;
use mc_game_core::world::entity::mob::{self, MobData, animal};
use mc_game_core::world::entity::spawn_group_data::SpawnGroupData;
use mc_game_core::world::entity::variant::{self, SpawnContext};
use mc_game_core::world::entity::{Entity, EntityId};
use mc_game_core::world::item::ItemStack;
use mc_game_core::world::level::server_level_accessor::ServerLevelAccessor;
use mc_nbt::CompoundTag;
use mc_protocol::components::{DataComponentMap, Holder, ZOMBIE_NAUTILUS_VARIANT};

define_accessor!(pub DATA_VARIANT_ID: ZombieNautilusVariant = ZombieNautilus);

/// `BRAIN_PROVIDER`.
static BRAIN_PROVIDER: Provider = Provider::with_memories(
    &[ANGRY_AT.id(), ATTACK_TARGET_COOLDOWN.id()],
    &[
        &NEAREST_LIVING_ENTITIES,
        &NEAREST_ADULT,
        &NEAREST_PLAYERS,
        &HURT_BY,
        &NAUTILUS_TEMPTATIONS,
    ],
    zombie_nautilus_ai::get_activities,
);

/// `ZombieNautilusVariants.TEMPERATE`.
const TEMPERATE: &str = "minecraft:temperate";

/// `ZombieNautilus`.
pub struct ZombieNautilus {
    pub mob: MobData,
    pub nautilus: AbstractNautilusData,
    registries: RegistryAccess,
    variant_key: Identifier,
}

impl ZombieNautilus {
    /// `new ZombieNautilus(type, level)`.
    pub fn new(ty: EntityType, id: EntityId, registries: &RegistryAccess) -> Self {
        let default_variant = variant::get_default_or_any(
            registries,
            keys::ZOMBIE_NAUTILUS_VARIANT,
            &Identifier::parse(TEMPERATE).expect("valid id"),
        );
        let mob = abstract_nautilus::mob_data(ty, id, |b| {
            b.define(DATA_VARIANT_ID, default_variant as i32);
        });
        let variant_key = registries
            .registry_or_throw(keys::ZOMBIE_NAUTILUS_VARIANT)
            .name_of(default_variant)
            .expect("variant entry");
        let mut zombie = ZombieNautilus {
            mob,
            nautilus: AbstractNautilusData::new(),
            registries: registries.clone(),
            variant_key,
        };
        let brain = zombie.make_brain(&BrainPacked::EMPTY);
        zombie.mob.living.brain = brain;
        abstract_nautilus::init_mob(&mut zombie);
        zombie
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
            .registry(keys::ZOMBIE_NAUTILUS_VARIANT)
            .and_then(|r| r.name_of(id))
        {
            self.variant_key = name;
        }
    }

    /// `getVariant()` (registry id).
    pub fn get_variant(&self) -> i32 {
        self.mob.living.entity.entity_data.get(DATA_VARIANT_ID)
    }

    /// The registry name of the variant.
    pub fn variant_key(&self) -> &Identifier {
        &self.variant_key
    }
}

impl AbstractNautilus for ZombieNautilus {
    fn nautilus(&self) -> &AbstractNautilusData {
        &self.nautilus
    }
    fn nautilus_mut(&mut self) -> &mut AbstractNautilusData {
        &mut self.nautilus
    }
    fn get_dash_sound(&self) -> SoundEvent {
        if self.is_under_water() {
            SoundEvent::ENTITY_ZOMBIE_NAUTILUS_DASH
        } else {
            SoundEvent::ENTITY_ZOMBIE_NAUTILUS_DASH_LAND
        }
    }
    fn get_dash_ready_sound(&self) -> SoundEvent {
        if self.is_under_water() {
            SoundEvent::ENTITY_ZOMBIE_NAUTILUS_DASH_READY
        } else {
            SoundEvent::ENTITY_ZOMBIE_NAUTILUS_DASH_READY_LAND
        }
    }
}

nautilus_mount_inventory!(ZombieNautilus);

impl Entity for ZombieNautilus {
    mc_game_core::mob_entity_accessors!(mob);
    abstract_nautilus_entity!();

    fn get_encode_id(&self) -> Option<&'static str> {
        Some("minecraft:zombie_nautilus")
    }

    fn make_brain(&mut self, packed: &BrainPacked) -> Brain {
        BRAIN_PROVIDER.make_brain(self, packed)
    }

    fn get_breed_offspring(
        &mut self,
        _level: &mut ServerLevel,
        _partner: EntityId,
    ) -> Option<Box<dyn Entity>> {
        None
    }

    fn sun_protection_slot(&self) -> EquipmentSlot {
        EquipmentSlot::Body
    }

    fn custom_server_ai_step(&mut self, level: &mut ServerLevel) {
        Brain::tick(level, self);
        nautilus_ai::update_activity(self);
        abstract_nautilus::check_restriction(self);
        animal::custom_server_ai_step(self);
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
            selected = variant::select_variant_to_spawn::<ZombieNautilusVariant>(
                &context,
                keys::ZOMBIE_NAUTILUS_VARIANT,
                random,
            );
            true
        });
        if let Some(id) = selected.and_then(|key| {
            self.registries
                .registry(keys::ZOMBIE_NAUTILUS_VARIANT)
                .and_then(|r| r.id_of(&key))
        }) {
            self.set_variant(id);
        }
        abstract_nautilus::finalize_spawn(self, level, difficulty, reason, group_data)
    }

    fn can_be_leashed(&self) -> bool {
        !abstract_nautilus::is_aggravated(self) && !abstract_nautilus::is_mob_controlled(self)
    }

    fn can_be_a_baby(&self) -> bool {
        false
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        mob::nbt::add_additional_save_data(self, out);
        variant::write_variant(out, &self.variant_key);
    }

    fn read_additional_save_data(&mut self, level: Option<&ServerLevel>, input: &CompoundTag) {
        mob::nbt::read_additional_save_data(self, level, input);
        if let Some(id) = variant::read_variant_key(input).and_then(|k| {
            self.registries
                .registry(keys::ZOMBIE_NAUTILUS_VARIANT)
                .and_then(|r| r.id_of(&k))
        }) {
            self.set_variant(id);
        }
    }

    fn entity_components(&self) -> DataComponentMap {
        let base = mc_game_core::advancements::predicates::entity::support::base_entity_components(
            self.base(),
        );
        DataComponentMap::builder()
            .add_all(&base)
            .set(
                ZOMBIE_NAUTILUS_VARIANT,
                Holder::Reference(self.variant_key.clone()),
            )
            .build()
    }

    fn apply_implicit_components(&mut self, stack: &ItemStack) {
        if let Some(Holder::Reference(key)) = stack.get(&ZOMBIE_NAUTILUS_VARIANT)
            && let Some(id) = self
                .registries
                .registry(keys::ZOMBIE_NAUTILUS_VARIANT)
                .and_then(|r| r.id_of(key))
        {
            self.set_variant(id);
        }
        mc_game_core::world::entity::spawn_config::apply_base_implicit_components(self, stack);
    }

    fn get_ambient_sound(&self) -> Option<SoundEvent> {
        Some(if self.is_under_water() {
            SoundEvent::ENTITY_ZOMBIE_NAUTILUS_AMBIENT
        } else {
            SoundEvent::ENTITY_ZOMBIE_NAUTILUS_AMBIENT_LAND
        })
    }

    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        Some(if self.is_under_water() {
            SoundEvent::ENTITY_ZOMBIE_NAUTILUS_HURT
        } else {
            SoundEvent::ENTITY_ZOMBIE_NAUTILUS_HURT_LAND
        })
    }

    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(if self.is_under_water() {
            SoundEvent::ENTITY_ZOMBIE_NAUTILUS_DEATH
        } else {
            SoundEvent::ENTITY_ZOMBIE_NAUTILUS_DEATH_LAND
        })
    }

    fn play_eating_sound(&mut self, level: &mut ServerLevel) {
        make_sound(self, level, SoundEvent::ENTITY_ZOMBIE_NAUTILUS_EAT);
    }

    fn get_swim_sound(&self) -> SoundEvent {
        SoundEvent::ENTITY_ZOMBIE_NAUTILUS_SWIM
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static ZOMBIE_NAUTILUS: EntityRegistration = EntityRegistration {
    entity_type: EntityType::ZOMBIE_NAUTILUS,
    create: |ty, id, registries| Box::new(ZombieNautilus::new(ty, id, registries)),
};
