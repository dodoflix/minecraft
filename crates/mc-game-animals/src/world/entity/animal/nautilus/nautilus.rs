//! `net.minecraft.world.entity.animal.nautilus.Nautilus`: the tameable swimming mount, fed with fish buckets and pufferfish
//! to tame and breed, with a brain that charges at its enemies and a dry-out timer on land.

use super::abstract_nautilus::{
    self, AbstractNautilus, AbstractNautilusData, abstract_nautilus_entity,
    nautilus_mount_inventory,
};
use super::nautilus_ai;
use linkme::distributed_slice;
use mc_core::phys::Vec3;
use mc_data::enums::EntitySpawnReason;
use mc_data::{EntityType, SoundEvent};
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
use mc_game_core::world::entity::dimensions::{EntityAttachments, EntityDimensions};
use mc_game_core::world::entity::factory::{
    self, ENTITY_FACTORIES, EntityRegistration, EntitySpawnRequest,
};
use mc_game_core::world::entity::living_entity::damage::make_sound;
use mc_game_core::world::entity::mob::{self, MobData, animal, tamable};
use mc_game_core::world::entity::spawn_group_data::SpawnGroupData;
use mc_game_core::world::entity::{Entity, EntityId};
use mc_game_core::world::level::server_level_accessor::ServerLevelAccessor;

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
    nautilus_ai::get_activities,
);

/// `BABY_DIMENSIONS`.
fn baby_dimensions() -> EntityDimensions {
    let scaled = EntityDimensions::of_type(EntityType::NAUTILUS).scale(0.5);
    let attachments = EntityAttachments::create_default(scaled.width, scaled.height)
        .with_passenger(Vec3::new(0.0, 0.5, 0.0));
    scaled.with_attachments(attachments)
}

/// `Nautilus`.
pub struct Nautilus {
    pub mob: MobData,
    pub nautilus: AbstractNautilusData,
}

impl Nautilus {
    /// `new Nautilus(type, level)`.
    pub fn new(ty: EntityType, id: EntityId) -> Self {
        let mob = abstract_nautilus::mob_data(ty, id, |_| {});
        let mut nautilus = Nautilus {
            mob,
            nautilus: AbstractNautilusData::new(),
        };
        let brain = nautilus.make_brain(&BrainPacked::EMPTY);
        nautilus.mob.living.brain = brain;
        abstract_nautilus::init_mob(&mut nautilus);
        nautilus
    }

    /// `handleAirSupply(level, preTickAirSupply)`: the nautilus dries out on land.
    fn handle_air_supply(&mut self, level: &mut ServerLevel, pre_tick_air_supply: i32) {
        if self.is_alive() && !self.is_in_water() {
            self.base_mut().set_air_supply(pre_tick_air_supply - 1);
            if self.base().get_air_supply() <= -20 {
                self.base_mut().set_air_supply(0);
                let source = level.damage_sources().dry_out();
                self.hurt_server(level, &source, 2.0);
            }
        } else {
            self.base_mut()
                .set_air_supply(abstract_nautilus::TOTAL_AIR_SUPPLY);
        }
    }
}

impl AbstractNautilus for Nautilus {
    fn nautilus(&self) -> &AbstractNautilusData {
        &self.nautilus
    }
    fn nautilus_mut(&mut self) -> &mut AbstractNautilusData {
        &mut self.nautilus
    }
    fn get_dash_sound(&self) -> SoundEvent {
        if self.is_under_water() {
            SoundEvent::ENTITY_NAUTILUS_DASH
        } else {
            SoundEvent::ENTITY_NAUTILUS_DASH_LAND
        }
    }
    fn get_dash_ready_sound(&self) -> SoundEvent {
        if self.is_under_water() {
            SoundEvent::ENTITY_NAUTILUS_DASH_READY
        } else {
            SoundEvent::ENTITY_NAUTILUS_DASH_READY_LAND
        }
    }
}

nautilus_mount_inventory!(Nautilus);

impl Entity for Nautilus {
    mc_game_core::mob_entity_accessors!(mob);
    abstract_nautilus_entity!();

    fn get_encode_id(&self) -> Option<&'static str> {
        Some("minecraft:nautilus")
    }

    fn get_dimensions(&self, _pose: mc_data::enums::Pose) -> EntityDimensions {
        if self.is_baby() {
            baby_dimensions()
        } else {
            EntityDimensions::of_type(self.mob.living.entity.entity_type)
        }
    }

    fn make_brain(&mut self, packed: &BrainPacked) -> Brain {
        BRAIN_PROVIDER.make_brain(self, packed)
    }

    fn get_breed_offspring(
        &mut self,
        level: &mut ServerLevel,
        _partner: EntityId,
    ) -> Option<Box<dyn Entity>> {
        let mut baby = factory::create(
            EntityType::NAUTILUS,
            level,
            EntitySpawnRequest {
                reason: EntitySpawnReason::Breeding,
                ignore_checks: false,
            },
        )?;
        if tamable::is_tame(self) {
            tamable::set_owner_reference(&mut *baby, tamable::get_owner_reference(self));
            baby.set_tame(None, true, true);
        }
        Some(baby)
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
        abstract_nautilus::finalize_spawn(self, level, difficulty, reason, group_data)
    }

    fn base_tick(&mut self, level: &mut ServerLevel) {
        let air_supply = self.base().get_air_supply();
        mob::tick::base_tick(self, level);
        if !self.mob.is_no_ai() {
            self.handle_air_supply(level, air_supply);
        }
    }

    fn get_max_air_supply(&self) -> i32 {
        abstract_nautilus::TOTAL_AIR_SUPPLY
    }

    fn can_be_leashed(&self) -> bool {
        !abstract_nautilus::is_aggravated(self)
    }

    fn get_ambient_sound(&self) -> Option<SoundEvent> {
        Some(match (self.is_baby(), self.is_under_water()) {
            (true, true) => SoundEvent::ENTITY_BABY_NAUTILUS_AMBIENT,
            (true, false) => SoundEvent::ENTITY_BABY_NAUTILUS_AMBIENT_LAND,
            (false, true) => SoundEvent::ENTITY_NAUTILUS_AMBIENT,
            (false, false) => SoundEvent::ENTITY_NAUTILUS_AMBIENT_LAND,
        })
    }

    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        Some(match (self.is_baby(), self.is_under_water()) {
            (true, true) => SoundEvent::ENTITY_BABY_NAUTILUS_HURT,
            (true, false) => SoundEvent::ENTITY_BABY_NAUTILUS_HURT_LAND,
            (false, true) => SoundEvent::ENTITY_NAUTILUS_HURT,
            (false, false) => SoundEvent::ENTITY_NAUTILUS_HURT_LAND,
        })
    }

    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(match (self.is_baby(), self.is_under_water()) {
            (true, true) => SoundEvent::ENTITY_BABY_NAUTILUS_DEATH,
            (true, false) => SoundEvent::ENTITY_BABY_NAUTILUS_DEATH_LAND,
            (false, true) => SoundEvent::ENTITY_NAUTILUS_DEATH,
            (false, false) => SoundEvent::ENTITY_NAUTILUS_DEATH_LAND,
        })
    }

    fn play_eating_sound(&mut self, level: &mut ServerLevel) {
        let sound = if self.is_baby() {
            SoundEvent::ENTITY_BABY_NAUTILUS_EAT
        } else {
            SoundEvent::ENTITY_NAUTILUS_EAT
        };
        make_sound(self, level, sound);
    }

    fn get_swim_sound(&self) -> SoundEvent {
        if self.is_baby() {
            SoundEvent::ENTITY_BABY_NAUTILUS_SWIM
        } else {
            SoundEvent::ENTITY_NAUTILUS_SWIM
        }
    }
}

mc_game_core::register_spawn_placement!(
    EntityType::NAUTILUS,
    abstract_nautilus::check_nautilus_spawn_rules
);

#[distributed_slice(ENTITY_FACTORIES)]
static NAUTILUS: EntityRegistration = EntityRegistration {
    entity_type: EntityType::NAUTILUS,
    create: |ty, id, _registries| Box::new(Nautilus::new(ty, id)),
};
