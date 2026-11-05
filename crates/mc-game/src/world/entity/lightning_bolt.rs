//! `net.minecraft.world.entity.LightningBolt` (server side: fire, rod power, copper cleaning, damage; the flash and the
//! sounds are client work triggered by the add-entity packet) and the `ServerLevel.tickThunder` spawn.

use super::factory::{ENTITY_FACTORIES, EntityRegistration};
use super::{Entity, EntityData, EntityId, entity};
use crate::server::level::ServerLevel;
use crate::world::level::block::BlockStateExt;
use crate::world::level::block::helpers::is_instance_of;
use crate::world::level::block::impls::lightning_rod_block::LightningRodBlock;
use crate::world::level::block::weathering_copper;
use crate::world::level::{BlockGetter, LevelAccessor};
use linkme::distributed_slice;
use mc_core::phys::{AABB, Vec3};
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::{EntityType, GameEvent};
use mc_world::level_storage::Difficulty;
use std::any::Any;
use std::collections::HashSet;
use std::sync::OnceLock;

const START_LIFE: i32 = 2;
const DAMAGE_RADIUS: f64 = 3.0;
const DETECTION_RADIUS: f64 = 15.0;

pub struct LightningBolt {
    base: EntityData,
    life: i32,
    pub seed: i64,
    flashes: i32,
    visual_only: bool,
    /// `cause` (the `ServerPlayer` that channeled it).
    cause: Option<EntityId>,
    hit_entities: HashSet<EntityId>,
    blocks_set_on_fire: i32,
}

impl LightningBolt {
    /// `LightningBolt(type, level)`: RNG order = UUID, `seed`, `flashes`.
    pub fn new(ty: EntityType, id: EntityId) -> Self {
        let mut base = EntityData::new(ty, id, entity::TOTAL_AIR_SUPPLY, |_| {});
        let seed = base.random.next_long();
        let flashes = base.random.next_int_bounded(3) + 1;
        Self { base, life: START_LIFE, seed, flashes, visual_only: false, cause: None, hit_entities: HashSet::new(), blocks_set_on_fire: 0 }
    }

    pub fn set_visual_only(&mut self, visual_only: bool) {
        self.visual_only = visual_only;
    }

    pub fn get_cause(&self) -> Option<EntityId> {
        self.cause
    }

    pub fn set_cause(&mut self, cause: Option<EntityId>) {
        self.cause = cause;
    }

    pub fn get_blocks_set_on_fire(&self) -> i32 {
        self.blocks_set_on_fire
    }

    /// `getHitEntities()` filtered to the living-in-level ones.
    pub fn get_hit_entities(&self, level: &ServerLevel) -> Vec<EntityId> {
        self.hit_entities.iter().copied().filter(|id| level.get_entity(*id).is_some_and(|e| e.is_alive())).collect()
    }

    /// `getStrikePosition()`.
    fn get_strike_position(&self) -> BlockPos {
        let p = self.base.position();
        BlockPos::containing(p.x, p.y - 1.0E-6, p.z)
    }

    /// `powerLightningRod()`.
    fn power_lightning_rod(&self, level: &mut ServerLevel) {
        let pos = self.get_strike_position();
        let state = level.get_block_state(pos);
        if is_instance_of(state, "LightningRodBlock") {
            LightningRodBlock::on_lightning_strike(state, level, pos);
        }
    }

    /// `spawnFire(additionalSources)`.
    fn spawn_fire(&mut self, level: &mut ServerLevel, additional_sources: i32) {
        if self.visual_only {
            return;
        }
        let pos = self.base.block_position();
        if !level.can_spread_fire_around(pos) {
            return;
        }
        let mut fire = crate::world::level::block::impls::base_fire_block::get_state(&*level, pos);
        if level.get_block_state(pos).is_air() && fire.can_survive(&*level, pos) {
            level.set_block_and_update(pos, fire);
            self.blocks_set_on_fire += 1;
        }
        for _ in 0..additional_sources {
            let dx = self.base.random.next_int_bounded(3) - 1;
            let dy = self.base.random.next_int_bounded(3) - 1;
            let dz = self.base.random.next_int_bounded(3) - 1;
            let nearby = pos.offset(dx, dy, dz);
            fire = crate::world::level::block::impls::base_fire_block::get_state(&*level, nearby);
            if level.get_block_state(nearby).is_air() && fire.can_survive(&*level, nearby) {
                level.set_block_and_update(nearby, fire);
                self.blocks_set_on_fire += 1;
            }
        }
    }
}

/// `clearCopperOnLightningStrike(level, struckPos)`.
pub fn clear_copper_on_lightning_strike(level: &mut ServerLevel, struck_pos: BlockPos) {
    let struck_state = level.get_block_state(struck_pos);
    let is_waxed = weathering_copper::is_waxed(struck_state.block());
    let is_weathering_copper = weathering_copper::is_weathering_copper(struck_state.block());
    if is_weathering_copper || is_waxed {
        if is_weathering_copper {
            let first = weathering_copper::get_first(level.get_block_state(struck_pos));
            level.set_block_and_update(struck_pos, first);
        }
        let strikes_count = LevelAccessor::random(level).next_int_bounded(3) + 3;
        for _ in 0..strikes_count {
            let step_count = LevelAccessor::random(level).next_int_bounded(8) + 1;
            random_walk_cleaning_copper(level, struck_pos, step_count);
        }
    }
}

/// `randomWalkCleaningCopper(level, originalStrikePos, workPos, stepCount)`.
fn random_walk_cleaning_copper(level: &mut ServerLevel, original_strike_pos: BlockPos, step_count: i32) {
    let mut work_pos = original_strike_pos;
    for _ in 0..step_count {
        match random_step_cleaning_copper(level, work_pos) {
            Some(step) => work_pos = step,
            None => break,
        }
    }
}

/// `randomStepCleaningCopper(level, pos)`: `BlockPos.randomInCube(random, 10, pos, 1)` drawn one candidate at a time.
fn random_step_cleaning_copper(level: &mut ServerLevel, pos: BlockPos) -> Option<BlockPos> {
    for _ in 0..10 {
        let random = LevelAccessor::random(level);
        let x = pos.x() - 1 + random.next_int_bounded(3);
        let y = pos.y() - 1 + random.next_int_bounded(3);
        let z = pos.z() - 1 + random.next_int_bounded(3);
        let candidate = BlockPos::new(x, y, z);
        let state = level.get_block_state(candidate);
        if weathering_copper::is_weathering_copper(state.block()) {
            if let Some(previous) = weathering_copper::get_previous(state) {
                level.set_block_and_update(candidate, previous);
            }
            level.level_event(None, 3002, candidate, -1);
            return Some(candidate);
        }
    }
    None
}

impl ServerLevel {
    /// `canSpreadFireAround(pos)`.
    pub fn can_spread_fire_around(&self, pos: BlockPos) -> bool {
        let spread_radius = self.fire_spread_radius_around_player();
        spread_radius == -1 || self.chunk_source.chunk_map.any_player_close_enough_to(pos, spread_radius)
    }
}

impl Entity for LightningBolt {
    fn base(&self) -> &EntityData {
        &self.base
    }
    fn base_mut(&mut self) -> &mut EntityData {
        &mut self.base
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn as_dyn(&self) -> &dyn Entity {
        self
    }
    fn as_dyn_mut(&mut self) -> &mut dyn Entity {
        self
    }

    fn tick(&mut self, level: &mut ServerLevel) {
        entity::base_tick(self, level);
        if self.life == START_LIFE {
            let difficulty = level.difficulty();
            if difficulty == Difficulty::Normal || difficulty == Difficulty::Hard {
                self.spawn_fire(level, 4);
            }
            self.power_lightning_rod(level);
            clear_copper_on_lightning_strike(level, self.get_strike_position());
            let position = self.base.position();
            level.game_event_from_entity(Some(self.base.id), GameEvent::LIGHTNING_STRIKE, position);
        }
        self.life -= 1;
        if self.life < 0 {
            if self.flashes == 0 {
                // `CriteriaTriggers.LIGHTNING_STRIKE.trigger(player, this, viewers)` for the players within 256 blocks:
                // advancements are not ported yet (no criterion triggers exist).
                self.discard();
            } else if self.life < -self.base.random.next_int_bounded(10) {
                self.flashes -= 1;
                self.life = 1;
                self.seed = self.base.random.next_long();
                self.spawn_fire(level, 0);
            }
        }
        if self.life >= 0 && !self.visual_only {
            let p = self.base.position();
            let area = AABB::new(p.x - DAMAGE_RADIUS, p.y - DAMAGE_RADIUS, p.z - DAMAGE_RADIUS, p.x + DAMAGE_RADIUS, p.y + 6.0 + DAMAGE_RADIUS, p.z + DAMAGE_RADIUS);
            let entities = level.get_entities(Some(self.base.id), &area, |e| e.is_alive());
            for &id in &entities {
                level.with_entity(id, |level, e| e.thunder_hit(level, self));
            }
            self.hit_entities.extend(entities);
            // `CriteriaTriggers.CHANNELED_LIGHTNING.trigger(cause, entities)`: advancements are not ported yet.
        }
    }

    fn hurt_server(&mut self, _level: &mut ServerLevel, _source: &crate::world::damagesource::DamageSource, _amount: f32) -> bool {
        false
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static LIGHTNING_BOLT_FACTORY: EntityRegistration = EntityRegistration { entity_type: EntityType::LIGHTNING_BOLT, create: |ty, level| Box::new(LightningBolt::new(ty, level.next_entity_id())) };

/// Spawns the skeleton horse of a lightning trap at `pos` (`SkeletonHorse.setTrap(true); setAge(0)`): installed by the
/// mob classes (the horse is not an entity class of this slice).
pub type TrapHorseSpawner = fn(&mut ServerLevel, BlockPos);

static TRAP_HORSE_SPAWNER: OnceLock<TrapHorseSpawner> = OnceLock::new();

/// Registers the skeleton-trap spawner (once, at startup).
pub fn set_trap_horse_spawner(spawner: TrapHorseSpawner) {
    let _ = TRAP_HORSE_SPAWNER.set(spawner);
}

/// The spawn half of `ServerLevel.tickThunder` (`LevelTickState::lightning_spawn`).
pub fn spawn_from_thunder(level: &mut ServerLevel, pos: BlockPos, is_trap: bool) {
    if is_trap && let Some(spawn) = TRAP_HORSE_SPAWNER.get() {
        spawn(level, pos);
    }
    let mut bolt = LightningBolt::new(EntityType::LIGHTNING_BOLT, level.next_entity_id());
    let at = Vec3::at_bottom_center_of(&pos);
    bolt.snap_to_pos(at.x, at.y, at.z);
    bolt.set_visual_only(is_trap);
    level.add_fresh_entity(Box::new(bolt));
}
