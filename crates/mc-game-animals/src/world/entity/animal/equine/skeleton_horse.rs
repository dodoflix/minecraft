//! `SkeletonHorse`: the skeleton trap horse (`SkeletonTrapGoal`), swims on the water surface.

use super::skeleton_trap_goal::SkeletonTrapGoal;
use linkme::distributed_slice;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::enums::{EntitySpawnReason, Pose};
use mc_data::{Attribute, EntityType, SoundEvent};
use mc_game_core::registry::RegistryAccess;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::damagesource::DamageSource;
use mc_game_core::world::difficulty_instance::DifficultyInstance;
use mc_game_core::world::entity::ai::navigation::StandardPathNavigation;
use mc_game_core::world::entity::animal::equine::abstract_horse::{
    self, AbstractHorse, AbstractHorseData, DATA_ID_FLAGS,
};
use mc_game_core::world::entity::dimensions::{EntityAttachments, EntityDimensions};
use mc_game_core::world::entity::equipment::EquipmentSlot;
use mc_game_core::world::entity::factory::{
    ENTITY_FACTORIES, EntityRegistration, EntitySpawnRequest,
};
use mc_game_core::world::entity::mob::{self, MobData, MobKind};
use mc_game_core::world::entity::spawn_group_data::SpawnGroupData;
use mc_game_core::world::entity::spawn_predicates;
use mc_game_core::world::entity::{Entity, EntityId, ServerPlayer};
use mc_game_core::world::item::{InteractionHand, InteractionResult, ItemStack};
use mc_game_core::world::level::server_level_accessor::ServerLevelAccessor;
use mc_nbt::CompoundTag;

const TRAP_MAX_LIFE: i32 = 18000;

/// `BABY_DIMENSIONS`.
fn baby_dimensions() -> EntityDimensions {
    let ty = EntityType::SKELETON_HORSE;
    let base = EntityDimensions::of_type(ty);
    let attachments = EntityAttachments::create_default(base.width, base.height)
        .with_passenger(Vec3::new(0.0, f64::from(ty.data().height - 0.25), 0.0));
    base.with_attachments(attachments).scale(0.7)
}

/// `SkeletonHorse`.
pub struct SkeletonHorse {
    pub mob: MobData,
    pub horse: AbstractHorseData,
    is_trap: bool,
    trap_time: i32,
}

impl SkeletonHorse {
    /// `new SkeletonHorse(type, level)`.
    pub fn new(ty: EntityType, id: EntityId, _registries: &RegistryAccess) -> Self {
        let mob = MobData::new(ty, id, MobKind::Animal, |b| {
            b.define(DATA_ID_FLAGS, 0i8);
        });
        let mut horse = SkeletonHorse {
            mob,
            horse: AbstractHorseData::new(),
            is_trap: false,
            trap_time: 0,
        };
        let navigation = StandardPathNavigation::ground(&horse);
        horse.mob.set_navigation(Box::new(navigation));
        abstract_horse::register_goals(&mut horse, |_| {});
        horse.create_inventory();
        // `skeletonTrapGoal` (see `SkeletonTrapGoal`): registered while `isTrap`; kept registered and gated by the flag
        horse
            .mob
            .goal_selector
            .add_goal(1, Box::new(SkeletonTrapGoal));
        horse
    }

    /// `isTrap()`.
    pub fn is_trap(&self) -> bool {
        self.is_trap
    }

    /// `setTrap(trap)`.
    pub fn set_trap(&mut self, trap: bool) {
        self.is_trap = trap;
    }

    /// `getSwimSound()` (counts gallop steps while ridden).
    fn next_swim_sound(&mut self) -> SoundEvent {
        if self.base().on_ground() {
            if !self.base().is_vehicle() {
                return SoundEvent::ENTITY_SKELETON_HORSE_STEP_WATER;
            }
            self.horse.gallop_sound_counter += 1;
            if self.horse.gallop_sound_counter > 5 && self.horse.gallop_sound_counter % 3 == 0 {
                return SoundEvent::ENTITY_SKELETON_HORSE_GALLOP_WATER;
            }
            if self.horse.gallop_sound_counter <= 5 {
                return SoundEvent::ENTITY_SKELETON_HORSE_STEP_WATER;
            }
        }
        SoundEvent::ENTITY_SKELETON_HORSE_SWIM
    }
}

/// `SkeletonHorse.checkSkeletonHorseSpawnRules`.
pub fn check_skeleton_horse_spawn_rules(
    ty: EntityType,
    level: &dyn ServerLevelAccessor,
    reason: EntitySpawnReason,
    pos: BlockPos,
    random: &mut dyn RandomSource,
) -> bool {
    if !spawn_predicates::is_spawner(reason) {
        spawn_predicates::check_animal_spawn_rules(ty, level, reason, pos, random)
    } else {
        spawn_predicates::ignores_light_requirements(reason)
            || spawn_predicates::is_bright_enough_to_spawn(level, pos)
    }
}

mc_game_core::register_spawn_placement!(
    EntityType::SKELETON_HORSE,
    check_skeleton_horse_spawn_rules
);

impl AbstractHorse for SkeletonHorse {
    fn horse(&self) -> &AbstractHorseData {
        &self.horse
    }
    fn horse_mut(&mut self) -> &mut AbstractHorseData {
        &mut self.horse
    }

    fn randomize_attributes(&mut self, random: &mut dyn RandomSource) {
        let jump = abstract_horse::generate_jump_strength(|| random.next_double());
        if let Some(i) = self
            .mob
            .living
            .attributes
            .get_instance(Attribute::JUMP_STRENGTH)
        {
            i.set_base_value(jump);
        }
    }

    fn play_jump_sound(&mut self, level: &mut ServerLevel) {
        if self.is_in_water() {
            self.play_sound(
                level,
                SoundEvent::ENTITY_SKELETON_HORSE_JUMP_WATER,
                0.4,
                1.0,
            );
        } else {
            self.play_sound(level, SoundEvent::ENTITY_HORSE_JUMP, 0.4, 1.0);
        }
    }
}

impl Entity for SkeletonHorse {
    mc_game_core::mob_entity_accessors!(mob);
    mc_game_core::abstract_horse_entity!();

    fn get_encode_id(&self) -> Option<&'static str> {
        Some("minecraft:skeleton_horse")
    }

    fn get_dimensions(&self, _pose: Pose) -> EntityDimensions {
        if self.is_baby() {
            baby_dimensions()
        } else {
            EntityDimensions::of_type(self.mob.living.entity.entity_type)
        }
    }

    fn ai_step(&mut self, level: &mut ServerLevel) {
        abstract_horse::ai_step(self, level);
        if !self.is_persistence_required() && self.is_trap() {
            let time = self.trap_time;
            self.trap_time += 1;
            if time >= TRAP_MAX_LIFE {
                self.discard();
            }
        }
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

    fn play_swim_sound(&mut self, level: &mut ServerLevel, volume: f32) {
        let volume = if self.base().on_ground() {
            0.3
        } else {
            f32::min(0.1, volume * 25.0)
        };
        let sound = self.next_swim_sound();
        let a = self.base_mut().random.next_float();
        let b = self.base_mut().random.next_float();
        self.play_sound(level, sound, volume, 1.0 + (a - b) * 0.4);
    }

    fn get_water_slow_down(&self) -> f32 {
        0.96
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
        Some(if self.is_eye_in_fluid("water") {
            SoundEvent::ENTITY_SKELETON_HORSE_AMBIENT_WATER
        } else {
            SoundEvent::ENTITY_SKELETON_HORSE_AMBIENT
        })
    }

    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_SKELETON_HORSE_DEATH)
    }

    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_SKELETON_HORSE_HURT)
    }

    fn mob_interact(
        &mut self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        hand: InteractionHand,
    ) -> InteractionResult {
        if !abstract_horse::is_tamed(self) {
            InteractionResult::PASS
        } else {
            abstract_horse::mob_interact(self, level, player, hand)
        }
    }

    fn can_mate(&self, _partner: &dyn Entity) -> bool {
        false
    }

    fn can_age_up(&self) -> bool {
        false
    }

    fn get_breed_offspring(
        &mut self,
        level: &mut ServerLevel,
        _partner: EntityId,
    ) -> Option<Box<dyn Entity>> {
        mc_game_core::world::entity::factory::create(
            EntityType::SKELETON_HORSE,
            level,
            EntitySpawnRequest {
                reason: EntitySpawnReason::Breeding,
                ignore_checks: false,
            },
        )
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
        abstract_horse::add_additional_save_data(self, out);
        out.put_boolean("SkeletonTrap", self.is_trap());
        out.put_int("SkeletonTrapTime", self.trap_time);
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        abstract_horse::read_additional_save_data(self, input);
        self.set_trap(input.get_boolean_or("SkeletonTrap", false));
        self.trap_time = input.get_int_or("SkeletonTrapTime", 0);
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static SKELETON_HORSE: EntityRegistration = EntityRegistration {
    entity_type: EntityType::SKELETON_HORSE,
    create: |ty, id, registries| Box::new(SkeletonHorse::new(ty, id, registries)),
};
