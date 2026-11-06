//! `Mob.finalizeSpawn`, `AgeableMob.finalizeSpawn` + `AgeableMobGroupData`, `Mob.checkDespawn`, spawn checks.

use super::MobData;
use crate::server::level::ServerLevel;
use crate::world::difficulty_instance::DifficultyInstance;
use crate::world::entity::Entity;
use crate::world::entity::attributes::{AttributeModifier, AttributeModifierOperation};
use crate::world::entity::living_entity::util::{l, lm};
use crate::world::entity::spawn_group_data::SpawnGroupData;
use crate::world::level::server_level_accessor::ServerLevelAccessor;
use mc_core::ident::Identifier;
use mc_core::random::RandomSource;
use mc_data::Attribute;
use mc_data::enums::EntitySpawnReason;
use mc_world::level_storage::Difficulty;
use std::sync::LazyLock;

/// `RANDOM_SPAWN_BONUS_ID`.
static RANDOM_SPAWN_BONUS_ID: LazyLock<Identifier> =
    LazyLock::new(|| Identifier::new("minecraft", "random_spawn_bonus").expect("valid id"));

/// `AgeableMob.AgeableMobGroupData`.
pub struct AgeableMobGroupData {
    group_size: i32,
    should_spawn_baby: bool,
    baby_spawn_chance: f32,
}

impl AgeableMobGroupData {
    pub fn new(should_spawn_baby: bool, baby_spawn_chance: f32) -> Self {
        Self {
            group_size: 0,
            should_spawn_baby,
            baby_spawn_chance,
        }
    }

    /// `AgeableMobGroupData(shouldSpawnBaby)` (chance 0.05).
    pub fn with_baby(should_spawn_baby: bool) -> Self {
        Self::new(should_spawn_baby, 0.05)
    }

    /// `AgeableMobGroupData(babySpawnChance)`.
    pub fn with_chance(baby_spawn_chance: f32) -> Self {
        Self::new(true, baby_spawn_chance)
    }

    pub fn get_group_size(&self) -> i32 {
        self.group_size
    }

    pub fn increase_group_size_by_one(&mut self) {
        self.group_size += 1;
    }

    pub fn is_should_spawn_baby(&self) -> bool {
        self.should_spawn_baby
    }

    pub fn get_baby_spawn_chance(&self) -> f32 {
        self.baby_spawn_chance
    }
}

impl SpawnGroupData for AgeableMobGroupData {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
    fn as_ageable_mut(&mut self) -> Option<&mut AgeableMobGroupData> {
        Some(self)
    }
}

/// `Mob.finalizeSpawn(level, difficulty, spawnReason, groupData)`, preceded by the `AgeableMob` group handling when `e`
/// is an ageable mob: the `super.finalizeSpawn` of every concrete mob class.
pub fn finalize_spawn(
    e: &mut dyn Entity,
    level: &mut dyn ServerLevelAccessor,
    difficulty: DifficultyInstance,
    reason: EntitySpawnReason,
    group_data: Option<Box<dyn SpawnGroupData>>,
) -> Option<Box<dyn SpawnGroupData>> {
    let group_data = if e.mob().is_some_and(|m| m.ageable.is_some()) {
        super::ageable::finalize_spawn(e, level, difficulty, reason, group_data)
    } else {
        group_data
    };
    mob_finalize_spawn(e, level, difficulty, reason, group_data)
}

/// `Mob.finalizeSpawn` alone.
pub fn mob_finalize_spawn(
    e: &mut dyn Entity,
    level: &mut dyn ServerLevelAccessor,
    _difficulty: DifficultyInstance,
    _reason: EntitySpawnReason,
    group_data: Option<Box<dyn SpawnGroupData>>,
) -> Option<Box<dyn SpawnGroupData>> {
    let random = level.random();
    let follow_range = lm(e)
        .attributes
        .get_instance(Attribute::FOLLOW_RANGE)
        .expect("follow_range attribute");
    if !follow_range.has_modifier(&RANDOM_SPAWN_BONUS_ID) {
        follow_range.add_permanent_modifier(AttributeModifier {
            id: RANDOM_SPAWN_BONUS_ID.clone(),
            amount: random.triangle(0.0, 0.11485000000000001),
            operation: AttributeModifierOperation::AddMultipliedBase,
        });
    }
    let left_handed = random.next_float() < 0.05;
    e.mob_mut().expect("mob").set_left_handed(left_handed);
    group_data
}

/// `Mob.checkDespawn()`.
pub fn check_despawn(e: &mut dyn Entity, level: &mut ServerLevel) {
    if level.difficulty() == Difficulty::Peaceful && !e.get_type().data().allowed_in_peaceful {
        e.discard();
        return;
    }
    let is_persistent = e.is_persistence_required() || e.requires_custom_persistence();
    let pos = e.position();
    let player = level.get_nearest_player(pos.x, pos.y, pos.z, -1.0, |p| !p.is_spectator());
    if let Some(player) = player {
        let dist_sqr = level
            .get_entity(player)
            .map_or(0.0, |p| p.base().distance_to_sqr_vec(pos));
        let category = e.get_type().data().category.data();
        let despawn_distance = category.despawn_distance;
        let despawn_distance_sqr = despawn_distance * despawn_distance;
        if !is_persistent
            && dist_sqr > f64::from(despawn_distance_sqr)
            && e.remove_when_far_away(dist_sqr)
        {
            e.discard();
        }
        let no_despawn_distance = category.no_despawn_distance;
        let no_despawn_distance_sqr = no_despawn_distance * no_despawn_distance;
        if !is_persistent
            && l(e).no_action_time > 600
            && e.base_mut().random.next_int_bounded(800) == 0
            && dist_sqr > f64::from(no_despawn_distance_sqr)
            && e.remove_when_far_away(dist_sqr)
        {
            e.discard();
        } else if dist_sqr < f64::from(no_despawn_distance_sqr) {
            lm(e).no_action_time = 0;
        }
    }
}

/// `Mob.checkSpawnObstruction(level)`.
pub fn check_spawn_obstruction(e: &dyn Entity, level: &dyn ServerLevelAccessor) -> bool {
    !level.contains_any_liquid(&e.base().bounding_box()) && level.is_unobstructed_entity(e)
}

/// `PathfinderMob.checkSpawnRules(level, spawnReason)`: `getWalkTargetValue(blockPosition(), level) >= 0`.
pub fn pathfinder_check_spawn_rules(e: &dyn Entity, level: &dyn ServerLevelAccessor) -> bool {
    e.get_walk_target_value(e.block_position(), level) >= 0.0
}

/// `Mob.getMaxFallDistance` etc. live in [`super::tick`]; this helper reads whether `e` keeps its persistence flag.
pub fn is_persistence_required(m: &MobData) -> bool {
    m.persistence_required
}
