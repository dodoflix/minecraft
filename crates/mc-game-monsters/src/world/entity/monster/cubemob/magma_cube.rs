//! `MagmaCube`.

use mc_game_core::world::entity::monster::cubemob::abstract_cube_mob::{self as acm, CubeMob, CubeMobAttackGoal, CubeMobData, CubeMobFloatGoal, CubeMobKeepOnJumpingGoal, CubeMobMoveControl, CubeMobRandomDirectionGoal, ID_SIZE};
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::damagesource::DamageSource;
use mc_game_core::world::difficulty_instance::DifficultyInstance;
use mc_game_core::world::entity::ai::navigation::StandardPathNavigation;
use mc_game_core::world::entity::dimensions::EntityDimensions;
use mc_game_core::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use mc_game_core::world::entity::living_entity::util::FluidKind;
use mc_game_core::world::entity::mob::spawn::AgeableMobGroupData;
use mc_game_core::world::entity::mob::{MobData, MobKind};
use mc_game_core::world::entity::spawn_group_data::SpawnGroupData;
use mc_game_core::world::entity::{Entity, EntityId, RemovalReason, ServerPlayer};
use mc_game_core::world::level::server_level_accessor::ServerLevelAccessor;
use linkme::distributed_slice;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::enums::{EntitySpawnReason, Pose};
use mc_data::{Attribute, EntityType, ParticleType, SoundEvent};
use mc_nbt::CompoundTag;
use mc_protocol::packets::play::SoundSource;
use mc_world::level_storage::Difficulty;

/// `MagmaCube`.
pub struct MagmaCube {
    pub mob: MobData,
    pub cube: CubeMobData,
}

impl MagmaCube {
    /// `new MagmaCube(type, level)`.
    pub fn new(ty: EntityType, id: EntityId) -> Self {
        let mob = MobData::new(ty, id, MobKind::AgeableMob, |b| {
            b.define(ID_SIZE, 1);
        });
        let mut cube = MagmaCube { mob, cube: CubeMobData::default() };
        cube.mob.enemy = true;
        let navigation = StandardPathNavigation::ground(&cube);
        cube.mob.set_navigation(Box::new(navigation));
        cube.register_goals();
        cube.fixup_dimensions();
        cube.mob.move_control = Some(Box::new(CubeMobMoveControl::new(cube.base().y_rot())));
        cube
    }

    /// `registerGoals()` of `AbstractCubeMob` with `MagmaCube`'s behaviour and targeting goals.
    fn register_goals(&mut self) {
        let float = CubeMobFloatGoal::new(self);
        let goals = &mut self.mob.goal_selector;
        goals.add_goal(1, Box::new(float));
        goals.add_goal(4, Box::new(CubeMobRandomDirectionGoal::new()));
        goals.add_goal(5, Box::new(CubeMobKeepOnJumpingGoal));
        goals.add_goal(2, Box::new(CubeMobAttackGoal::new()));
        acm::add_slime_targeting_goals(&mut self.mob.target_selector);
    }
}

impl CubeMob for MagmaCube {
    fn cube(&self) -> &CubeMobData {
        &self.cube
    }
    fn cube_mut(&mut self) -> &mut CubeMobData {
        &mut self.cube
    }
    fn get_particle_type(&self) -> Option<ParticleType> {
        Some(ParticleType::FLAME)
    }
    fn get_squish_sound(&self) -> SoundEvent {
        if self.is_tiny() { SoundEvent::ENTITY_MAGMA_CUBE_SQUISH_SMALL } else { SoundEvent::ENTITY_MAGMA_CUBE_SQUISH }
    }
    fn get_jump_sound(&self) -> SoundEvent {
        SoundEvent::ENTITY_MAGMA_CUBE_JUMP
    }
    fn get_jump_delay(&mut self) -> i32 {
        self.base_mut().random.next_int_bounded(20).wrapping_add(10).wrapping_mul(4)
    }
    fn decrease_squish(&mut self) {
        self.cube.target_squish *= 0.9;
    }
    fn can_deal_damage(&self) -> bool {
        self.is_effective_ai()
    }
    fn get_attack_damage(&self) -> f32 {
        self.mob.living.get_attribute_value(Attribute::ATTACK_DAMAGE) as f32 + 2.0
    }
    fn set_size(&mut self, size: i32, update_health: bool) {
        acm::set_size(self, size, update_health);
        let actual_size = self.get_size();
        if let Some(damage) = self.mob.living.attributes.get_instance(Attribute::ATTACK_DAMAGE) {
            damage.set_base_value(f64::from(actual_size));
        }
        if let Some(armor) = self.mob.living.attributes.get_instance(Attribute::ARMOR) {
            armor.set_base_value(f64::from(size.wrapping_mul(3)));
        }
        self.mob.xp_reward = actual_size;
    }
}

impl Entity for MagmaCube {
    mc_game_core::mob_entity_accessors!(mob);

    fn get_encode_id(&self) -> Option<&'static str> {
        Some("minecraft:magma_cube")
    }
    fn as_cube_mob(&self) -> Option<&dyn CubeMob> {
        Some(self)
    }
    fn as_cube_mob_mut(&mut self) -> Option<&mut dyn CubeMob> {
        Some(self)
    }
    fn cube_mob_size(&self) -> Option<i32> {
        Some(self.get_size())
    }
    fn get_dimensions(&self, pose: Pose) -> EntityDimensions {
        acm::get_dimensions(self, pose)
    }
    fn refresh_dimensions(&mut self) {
        acm::refresh_dimensions(self);
    }
    fn on_synced_data_updated(&mut self, accessor_id: u8) {
        acm::on_synced_data_updated(self, accessor_id);
    }
    fn tick(&mut self, level: &mut ServerLevel) {
        acm::tick(self, level);
    }
    fn before_remove(&mut self, level: &mut ServerLevel, reason: RemovalReason) {
        acm::before_remove(self, level, reason);
    }
    fn push_entity(&mut self, level: &mut ServerLevel, other: &mut dyn Entity) {
        acm::push_entity(self, level, other);
    }
    fn player_touch(&mut self, level: &mut ServerLevel, player: &mut ServerPlayer) {
        acm::player_touch(self, level, player);
    }
    fn jump_from_ground(&mut self, level: &ServerLevel) {
        let size_jump_boost_power = self.get_size() as f32 * 0.1;
        acm::jump_from_ground(self, level, size_jump_boost_power);
    }
    fn jump_in_liquid(&mut self, fluid: FluidKind) {
        if fluid == FluidKind::Lava {
            let movement = self.base().delta_movement();
            let y = f64::from(0.22f32 + self.get_size() as f32 * 0.05);
            self.base_mut().set_delta_movement_xyz(movement.x, y, movement.z);
            self.base_mut().needs_sync = true;
        } else {
            mc_game_core::world::entity::living_entity::jump::jump_in_liquid(self);
        }
    }
    fn is_on_fire(&self) -> bool {
        false
    }
    fn get_sound_source(&self) -> SoundSource {
        acm::HOSTILE
    }
    fn get_sound_volume(&self) -> f32 {
        0.4 * self.get_size() as f32
    }
    fn get_max_head_x_rot(&self) -> i32 {
        0
    }
    fn get_passenger_attachment_point(&self, _passenger: &dyn Entity, dimensions: &EntityDimensions, scale: f32) -> Vec3 {
        acm::get_passenger_attachment_point(self, dimensions, scale)
    }
    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        Some(if self.is_tiny() { SoundEvent::ENTITY_MAGMA_CUBE_HURT_SMALL } else { SoundEvent::ENTITY_MAGMA_CUBE_HURT })
    }
    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(if self.is_tiny() { SoundEvent::ENTITY_MAGMA_CUBE_DEATH_SMALL } else { SoundEvent::ENTITY_MAGMA_CUBE_DEATH })
    }
    fn can_be_a_baby(&self) -> bool {
        false
    }
    fn finalize_spawn(
        &mut self,
        level: &mut dyn ServerLevelAccessor,
        difficulty: DifficultyInstance,
        reason: EntitySpawnReason,
        group_data: Option<Box<dyn SpawnGroupData>>,
    ) -> Option<Box<dyn SpawnGroupData>> {
        let group_data = group_data.unwrap_or_else(|| Box::new(AgeableMobGroupData::with_baby(false)));
        acm::finalize_spawn(self, level, difficulty, reason, Some(group_data))
    }
    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        acm::add_additional_save_data(self, out);
    }
    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        acm::read_additional_save_data(self, input);
    }
}

/// `MagmaCube.checkMagmaCubeSpawnRules(type, level, spawnReason, pos, random)`.
pub fn check_magma_cube_spawn_rules(_ty: EntityType, level: &dyn ServerLevelAccessor, _reason: EntitySpawnReason, _pos: BlockPos, _random: &mut dyn RandomSource) -> bool {
    level.get_difficulty() != Difficulty::Peaceful
}

mc_game_core::register_spawn_placement!(EntityType::MAGMA_CUBE, check_magma_cube_spawn_rules);

#[distributed_slice(ENTITY_FACTORIES)]
static MAGMA_CUBE: EntityRegistration = EntityRegistration { entity_type: EntityType::MAGMA_CUBE, create: |ty, id, _registries| Box::new(MagmaCube::new(ty, id)) };
