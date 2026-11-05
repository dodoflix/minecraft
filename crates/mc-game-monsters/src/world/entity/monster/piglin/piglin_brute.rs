//! `PiglinBrute`.

use super::abstract_piglin::{self, AbstractPiglin, DATA_IMMUNE_TO_ZOMBIFICATION};
use super::piglin_arm_pose::PiglinArmPose;
use super::piglin_brute_ai;
use mc_game_core::registry::RegistryAccess;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::damagesource::DamageSource;
use mc_game_core::world::difficulty_instance::DifficultyInstance;
use mc_game_core::world::entity::ai::brain::{Brain, Provider};
use mc_game_core::world::entity::ai::memory::memory_module_type::{ATTACK_TARGET, NEAREST_VISIBLE_ADULT_PIGLINS};
use mc_game_core::world::entity::ai::memory::{BrainPacked, MemoryTypeId};
use mc_game_core::world::entity::ai::navigation::StandardPathNavigation;
use mc_game_core::world::entity::ai::sensing::sensor_type::{HURT_BY, NEAREST_ITEMS, NEAREST_LIVING_ENTITIES, NEAREST_PLAYERS, PIGLIN_BRUTE_SPECIFIC_SENSOR};
use mc_game_core::world::entity::equipment::EquipmentSlot;
use mc_game_core::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use mc_game_core::world::entity::living_entity::damage::make_sound;
use mc_game_core::world::entity::mob::{self, MobData, MobKind};
use mc_game_core::world::entity::spawn_group_data::SpawnGroupData;
use mc_game_core::world::entity::{Entity, EntityId};
use mc_game_core::world::item::ItemStack;
use mc_game_core::world::level::server_level_accessor::ServerLevelAccessor;
use linkme::distributed_slice;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::enums::EntitySpawnReason;
use mc_data::{EntityType, Item, SoundEvent};
use mc_nbt::CompoundTag;

static BRAIN_MEMORIES: [MemoryTypeId; 1] = [NEAREST_VISIBLE_ADULT_PIGLINS.id()];

/// `BRAIN_PROVIDER`.
static BRAIN_PROVIDER: Provider = Provider::with_memories(&BRAIN_MEMORIES, &[&NEAREST_LIVING_ENTITIES, &NEAREST_PLAYERS, &NEAREST_ITEMS, &HURT_BY, &PIGLIN_BRUTE_SPECIFIC_SENSOR], piglin_brute_ai::get_activities);

pub struct PiglinBrute {
    pub mob: MobData,
    time_in_overworld: i32,
}

impl PiglinBrute {
    /// `new PiglinBrute(type, level)`.
    pub fn new(ty: EntityType, id: EntityId, _registries: &RegistryAccess) -> Self {
        let mob = MobData::new(ty, id, MobKind::Monster, |b| {
            b.define(DATA_IMMUNE_TO_ZOMBIFICATION, false);
        });
        let mut brute = PiglinBrute { mob, time_in_overworld: 0 };
        let brain = brute.make_brain(&BrainPacked::EMPTY);
        brute.mob.living.brain = brain;
        let mut navigation = StandardPathNavigation::ground(&brute);
        abstract_piglin::apply_open_doors_ability(&mut navigation);
        brute.mob.set_navigation(Box::new(navigation));
        abstract_piglin::init(&mut brute.mob);
        brute.mob.xp_reward = 20;
        brute
    }

    /// `playAngrySound()`.
    pub fn play_angry_sound(&mut self, level: &mut ServerLevel) {
        make_sound(self, level, SoundEvent::ENTITY_PIGLIN_BRUTE_ANGRY);
    }
}

impl AbstractPiglin for PiglinBrute {
    fn time_in_overworld(&self) -> i32 {
        self.time_in_overworld
    }

    fn set_time_in_overworld(&mut self, time_in_overworld: i32) {
        self.time_in_overworld = time_in_overworld;
    }

    fn can_hunt(&self) -> bool {
        false
    }

    fn finish_conversion(&mut self, level: &mut ServerLevel) {
        abstract_piglin::finish_conversion(self, level);
    }

    fn play_converted_sound(&mut self, level: &mut ServerLevel) {
        make_sound(self, level, SoundEvent::ENTITY_PIGLIN_BRUTE_CONVERTED_TO_ZOMBIFIED);
    }

    fn get_arm_pose(&self) -> PiglinArmPose {
        if self.mob.is_aggressive() && abstract_piglin::is_holding_melee_weapon(self) { PiglinArmPose::AttackingWithMeleeWeapon } else { PiglinArmPose::Default }
    }
}

impl Entity for PiglinBrute {
    mc_game_core::mob_entity_accessors!(mob);

    fn get_encode_id(&self) -> Option<&'static str> {
        Some("minecraft:piglin_brute")
    }

    fn make_brain(&mut self, packed: &BrainPacked) -> Brain {
        BRAIN_PROVIDER.make_brain(self, packed)
    }

    fn get_target(&self) -> Option<EntityId> {
        self.brain().get_memory(&ATTACK_TARGET).map(|r| r.0)
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        abstract_piglin::add_additional_save_data(self, self.time_in_overworld, out);
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        self.time_in_overworld = abstract_piglin::read_additional_save_data(self, input);
    }

    fn finalize_spawn(
        &mut self,
        level: &mut dyn ServerLevelAccessor,
        difficulty: DifficultyInstance,
        reason: EntitySpawnReason,
        group_data: Option<Box<dyn SpawnGroupData>>,
    ) -> Option<Box<dyn SpawnGroupData>> {
        if let Some(server_level) = level.as_server_level() {
            let dimension = server_level.dimension.clone();
            piglin_brute_ai::init_memories(self, dimension);
        }
        self.populate_default_equipment_slots(level.random(), &difficulty);
        mob::spawn::finalize_spawn(self, level, difficulty, reason, group_data)
    }

    fn populate_default_equipment_slots(&mut self, _random: &mut dyn RandomSource, _difficulty: &DifficultyInstance) {
        mob::equipment::set_item_slot_spawn(self, EquipmentSlot::MainHand, ItemStack::new(Item::GOLDEN_AXE));
    }

    fn wants_to_pick_up(&self, _level: &ServerLevel, item_stack: &ItemStack) -> bool {
        // `itemStack.is(GOLDEN_AXE) ? super.wantsToPickUp(...) : false` (`Mob.wantsToPickUp` = `canHoldItem`)
        item_stack.item() == Item::GOLDEN_AXE && self.can_hold_item(item_stack)
    }

    fn custom_server_ai_step(&mut self, level: &mut ServerLevel) {
        Brain::tick(level, self);
        piglin_brute_ai::update_activity(level, self);
        piglin_brute_ai::maybe_play_activity_sound(level, self);
        abstract_piglin::custom_server_ai_step(self, level);
    }

    fn hurt_server(&mut self, level: &mut ServerLevel, source: &DamageSource, damage: f32) -> bool {
        let was_hurt = mc_game_core::world::entity::living_entity::damage::hurt_server(self, level, source, damage);
        if was_hurt && source.causing_is_living() {
            mob::ext::with_causing_entity(level, source, |level, attacker| piglin_brute_ai::was_hurt_by(level, self, attacker));
        }
        was_hurt
    }

    fn get_ambient_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_PIGLIN_BRUTE_AMBIENT)
    }

    fn play_ambient_sound(&mut self, level: &mut ServerLevel) {
        abstract_piglin::play_ambient_sound(self, level);
    }

    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_PIGLIN_BRUTE_HURT)
    }

    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_PIGLIN_BRUTE_DEATH)
    }

    fn play_step_sound(&mut self, level: &mut ServerLevel, _pos: BlockPos, _state: mc_data::block::BlockState) {
        self.play_sound(level, SoundEvent::ENTITY_PIGLIN_BRUTE_STEP, 0.15, 1.0);
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static PIGLIN_BRUTE: EntityRegistration = EntityRegistration { entity_type: EntityType::PIGLIN_BRUTE, create: |ty, id, registries| Box::new(PiglinBrute::new(ty, id, registries)) };
