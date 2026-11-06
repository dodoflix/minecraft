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
        Self::with_random(ty, id, mc_core::random::create_new())
    }

    /// `LightningBolt(type, level)` with the entity's random source given (reproducible `seed`/`flashes`).
    pub fn with_random(
        ty: EntityType,
        id: EntityId,
        random: mc_core::random::LegacyRandomSource,
    ) -> Self {
        let mut base = EntityData::new(ty, id, entity::TOTAL_AIR_SUPPLY, |_| {});
        base.random = random;
        base.uuid = mc_core::math::mth::create_insecure_uuid(&mut base.random);
        let seed = base.random.next_long();
        let flashes = base.random.next_int_bounded(3) + 1;
        Self {
            base,
            life: START_LIFE,
            seed,
            flashes,
            visual_only: false,
            cause: None,
            hit_entities: HashSet::new(),
            blocks_set_on_fire: 0,
        }
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

    /// `CriteriaTriggers.LIGHTNING_STRIKE.trigger(player, this, viewers)` for the players within 256 blocks, with the living
    /// entities within 15 blocks that the bolt did not hit as `viewers`.
    fn trigger_lightning_strike(&self, level: &mut ServerLevel) {
        use crate::advancements::triggers::CriteriaTriggers;
        use crate::world::entity::ServerPlayer;
        let p = self.base.position();
        let area = AABB::new(
            p.x - DETECTION_RADIUS,
            p.y - DETECTION_RADIUS,
            p.z - DETECTION_RADIUS,
            p.x + DETECTION_RADIUS,
            p.y + 6.0 + DETECTION_RADIUS,
            p.z + DETECTION_RADIUS,
        );
        let viewer_ids = level.get_entities(Some(self.base.id), &area, |e| {
            e.is_alive() && !self.hit_entities.contains(&e.id())
        });
        for player_id in level.players().to_vec() {
            let matches = {
                let Some(player) = level
                    .get_entity(player_id)
                    .and_then(|e| e.as_any().downcast_ref::<ServerPlayer>())
                else {
                    continue;
                };
                // `distanceTo(entity) < 256.0F` (float arithmetic)
                let (dx, dy, dz) = (
                    (p.x - player.base().x()) as f32,
                    (p.y - player.base().y()) as f32,
                    (p.z - player.base().z()) as f32,
                );
                if (dx * dx + dy * dy + dz * dz).sqrt() >= 256.0 {
                    continue;
                }
                let viewers: Vec<&dyn Entity> = viewer_ids
                    .iter()
                    .filter_map(|&id| level.get_entity(id))
                    .collect();
                CriteriaTriggers::LIGHTNING_STRIKE.matches(&*level, player, self, &viewers)
            };
            level.with_player(player_id, |level, player| matches.award(level, player));
        }
    }

    pub fn get_blocks_set_on_fire(&self) -> i32 {
        self.blocks_set_on_fire
    }

    pub fn life(&self) -> i32 {
        self.life
    }

    pub fn flashes(&self) -> i32 {
        self.flashes
    }

    /// `getHitEntities()` filtered to the living-in-level ones.
    pub fn get_hit_entities(&self, level: &ServerLevel) -> Vec<EntityId> {
        self.hit_entities
            .iter()
            .copied()
            .filter(|id| level.get_entity(*id).is_some_and(|e| e.is_alive()))
            .collect()
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
fn random_walk_cleaning_copper(
    level: &mut ServerLevel,
    original_strike_pos: BlockPos,
    step_count: i32,
) {
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
        spread_radius == -1
            || self
                .chunk_source
                .chunk_map
                .any_player_close_enough_to(pos, spread_radius)
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
                self.trigger_lightning_strike(level);
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
            let area = AABB::new(
                p.x - DAMAGE_RADIUS,
                p.y - DAMAGE_RADIUS,
                p.z - DAMAGE_RADIUS,
                p.x + DAMAGE_RADIUS,
                p.y + 6.0 + DAMAGE_RADIUS,
                p.z + DAMAGE_RADIUS,
            );
            let entities = level.get_entities(Some(self.base.id), &area, |e| e.is_alive());
            let bolt_id = self.base.id;
            for &id in &entities {
                level.with_entity(id, |level, e| e.thunder_hit_by(level, bolt_id));
            }
            self.hit_entities.extend(entities.iter().copied());
            if let Some(cause) = self.cause {
                let matches = {
                    let victims: Vec<&dyn Entity> = entities
                        .iter()
                        .filter_map(|&id| level.get_entity(id))
                        .collect();
                    level
                        .get_entity(cause)
                        .and_then(|e| {
                            e.as_any()
                                .downcast_ref::<crate::world::entity::ServerPlayer>()
                        })
                        .map(|player| {
                            crate::advancements::triggers::CriteriaTriggers::CHANNELED_LIGHTNING
                                .matches(&*level, player, &victims)
                        })
                };
                if let Some(matches) = matches {
                    level.with_player(cause, |level, player| matches.award(level, player));
                }
            }
        }
    }

    fn hurt_server(
        &mut self,
        _level: &mut ServerLevel,
        _source: &crate::world::damagesource::DamageSource,
        _amount: f32,
    ) -> bool {
        false
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static LIGHTNING_BOLT_FACTORY: EntityRegistration = EntityRegistration {
    entity_type: EntityType::LIGHTNING_BOLT,
    create: |ty, id, _registries| Box::new(LightningBolt::new(ty, id)),
};

/// The spawn half of `ServerLevel.tickThunder` (`LevelTickState::lightning_spawn`). Not implemented: for a trap
/// (`is_trap`) the bolt is only made visual; the trap skeleton horse vanilla spawns with it is not.
pub fn spawn_from_thunder(level: &mut ServerLevel, pos: BlockPos, is_trap: bool) {
    let mut bolt = LightningBolt::new(EntityType::LIGHTNING_BOLT, level.next_entity_id());
    let at = Vec3::at_bottom_center_of(&pos);
    bolt.snap_to_pos(at.x, at.y, at.z);
    bolt.set_visual_only(is_trap);
    level.add_fresh_entity(Box::new(bolt));
}
