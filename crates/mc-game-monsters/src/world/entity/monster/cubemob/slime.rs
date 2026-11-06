//! `Slime`.

use linkme::distributed_slice;
use mc_core::ident::Identifier;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::enums::{EntitySpawnReason, Pose};
use mc_data::{Attribute, EntityType, ParticleType, SoundEvent};
use mc_game_core::registry::keys;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::damagesource::DamageSource;
use mc_game_core::world::difficulty_instance::DifficultyInstance;
use mc_game_core::world::entity::ai::navigation::StandardPathNavigation;
use mc_game_core::world::entity::dimensions::EntityDimensions;
use mc_game_core::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use mc_game_core::world::entity::mob::spawn::AgeableMobGroupData;
use mc_game_core::world::entity::mob::{self, MobData, MobKind};
use mc_game_core::world::entity::monster::cubemob::abstract_cube_mob::{
    self as acm, CubeMob, CubeMobAttackGoal, CubeMobData, CubeMobFloatGoal,
    CubeMobKeepOnJumpingGoal, CubeMobMoveControl, CubeMobRandomDirectionGoal, ID_SIZE,
};
use mc_game_core::world::entity::spawn_group_data::SpawnGroupData;
use mc_game_core::world::entity::spawn_predicates::{check_mob_spawn_rules, is_spawner};
use mc_game_core::world::entity::{Entity, EntityId, RemovalReason, ServerPlayer};
use mc_game_core::world::level::LevelReader as _;
use mc_game_core::world::level::server_level_accessor::ServerLevelAccessor;
use mc_nbt::CompoundTag;
use mc_protocol::packets::play::SoundSource;
use mc_world::level_storage::Difficulty;

/// `Slime`.
pub struct Slime {
    pub mob: MobData,
    pub cube: CubeMobData,
}

impl Slime {
    /// `new Slime(type, level)`.
    pub fn new(ty: EntityType, id: EntityId) -> Self {
        let mob = MobData::new(ty, id, MobKind::AgeableMob, |b| {
            b.define(ID_SIZE, 1);
        });
        let mut slime = Slime {
            mob,
            cube: CubeMobData::default(),
        };
        slime.mob.enemy = true;
        let navigation = StandardPathNavigation::ground(&slime);
        slime.mob.set_navigation(Box::new(navigation));
        slime.register_goals();
        // `AbstractCubeMob.<init>`
        slime.fixup_dimensions();
        slime.mob.move_control = Some(Box::new(CubeMobMoveControl::new(slime.base().y_rot())));
        slime
    }

    /// `registerGoals()` of `AbstractCubeMob` with `Slime`'s behaviour and targeting goals.
    fn register_goals(&mut self) {
        let float = CubeMobFloatGoal::new(self);
        let goals = &mut self.mob.goal_selector;
        goals.add_goal(1, Box::new(float));
        goals.add_goal(4, Box::new(CubeMobRandomDirectionGoal::new()));
        goals.add_goal(5, Box::new(CubeMobKeepOnJumpingGoal));
        // `addBehaviourGoals()`
        goals.add_goal(2, Box::new(CubeMobAttackGoal::new()));
        acm::add_slime_targeting_goals(&mut self.mob.target_selector);
    }
}

impl CubeMob for Slime {
    fn cube(&self) -> &CubeMobData {
        &self.cube
    }
    fn cube_mut(&mut self) -> &mut CubeMobData {
        &mut self.cube
    }
    fn get_particle_type(&self) -> Option<ParticleType> {
        Some(ParticleType::ITEM_SLIME)
    }
    fn get_squish_sound(&self) -> SoundEvent {
        if self.is_tiny() {
            SoundEvent::ENTITY_SLIME_SQUISH_SMALL
        } else {
            SoundEvent::ENTITY_SLIME_SQUISH
        }
    }
    fn get_jump_sound(&self) -> SoundEvent {
        if self.is_tiny() {
            SoundEvent::ENTITY_SLIME_JUMP_SMALL
        } else {
            SoundEvent::ENTITY_SLIME_JUMP
        }
    }
    fn set_size(&mut self, size: i32, update_health: bool) {
        acm::set_size(self, size, update_health);
        let actual_size = self.get_size();
        if let Some(damage) = self
            .mob
            .living
            .attributes
            .get_instance(Attribute::ATTACK_DAMAGE)
        {
            damage.set_base_value(f64::from(actual_size));
        }
        self.mob.xp_reward = actual_size;
    }
}

impl Entity for Slime {
    mc_game_core::mob_entity_accessors!(mob);

    fn get_encode_id(&self) -> Option<&'static str> {
        Some("minecraft:slime")
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
        acm::jump_from_ground(self, level, 0.0);
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
    fn get_passenger_attachment_point(
        &self,
        _passenger: &dyn Entity,
        dimensions: &EntityDimensions,
        scale: f32,
    ) -> Vec3 {
        acm::get_passenger_attachment_point(self, dimensions, scale)
    }
    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        Some(if self.is_tiny() {
            SoundEvent::ENTITY_SLIME_HURT_SMALL
        } else {
            SoundEvent::ENTITY_SLIME_HURT
        })
    }
    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(if self.is_tiny() {
            SoundEvent::ENTITY_SLIME_DEATH_SMALL
        } else {
            SoundEvent::ENTITY_SLIME_DEATH
        })
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
        let group_data =
            group_data.unwrap_or_else(|| Box::new(AgeableMobGroupData::with_baby(false)));
        acm::finalize_spawn(self, level, difficulty, reason, Some(group_data))
    }
    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        acm::add_additional_save_data(self, out);
    }
    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        acm::read_additional_save_data(self, input);
    }
}

/// `Slime.checkSlimeSpawnRules(type, level, spawnReason, pos, random)`.
pub fn check_slime_spawn_rules(
    ty: EntityType,
    level: &dyn ServerLevelAccessor,
    reason: EntitySpawnReason,
    pos: BlockPos,
    random: &mut dyn RandomSource,
) -> bool {
    if level.get_difficulty() == Difficulty::Peaceful {
        return false;
    }
    if is_spawner(reason) {
        return check_mob_spawn_rules(ty, level, reason, pos, random);
    }
    if biome_allows_surface_slime_spawns(level, pos) && pos.y() > 50 && pos.y() < 70 {
        let surface_slime_spawn_chance = level.surface_slime_spawn_chance_at(pos);
        if random.next_float() < surface_slime_spawn_chance
            && level.get_max_local_raw_brightness(pos) <= random.next_int_bounded(8)
        {
            return check_mob_spawn_rules(ty, level, reason, pos, random);
        }
    }
    let Some(seed) = level.world_gen_seed() else {
        return false;
    };
    let slime_chunk = mc_core::random::WorldgenRandom::seed_slime_chunk(
        pos.x() >> 4,
        pos.z() >> 4,
        seed,
        987234911,
    )
    .next_int_bounded(10)
        == 0;
    if random.next_int_bounded(10) == 0 && slime_chunk && pos.y() < 40 {
        return check_mob_spawn_rules(ty, level, reason, pos, random);
    }
    false
}

/// `level.getBiome(pos).is(BiomeTags.ALLOWS_SURFACE_SLIME_SPAWNS)`.
fn biome_allows_surface_slime_spawns(level: &dyn ServerLevelAccessor, pos: BlockPos) -> bool {
    let Some(biome) = level.get_biome_id(pos) else {
        return false;
    };
    let Some(registry) = level.registry_access().registry(keys::BIOME) else {
        return false;
    };
    registry.is_in_tag(
        biome.0,
        &Identifier::parse("minecraft:allows_surface_slime_spawns").expect("valid id"),
    )
}

mc_game_core::register_spawn_placement!(EntityType::SLIME, check_slime_spawn_rules);

#[distributed_slice(ENTITY_FACTORIES)]
static SLIME: EntityRegistration = EntityRegistration {
    entity_type: EntityType::SLIME,
    create: |ty, id, _registries| Box::new(Slime::new(ty, id)),
};
