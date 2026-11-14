//! `net.minecraft.world.entity.boss.enderdragon.EnderDragon`: the body of the dragon, its eight hitboxes
//! ([`EnderDragonPart`]) and what an attack on a hitbox does to it.
//!
//! The dragon owns the ids of its parts (taken right after its own), the level holds the parts while it tracks the dragon
//! (`ServerLevel.dragonParts`) and the dragon moves them every tick.
//!
//! Not implemented: the phase behaviour (`DragonPhaseInstance`: the flight to the phase's target, `onHurt`, `onCrystalDestroyed`,
//! `begin`/`end`); the wing knockback and the head/neck contact damage of `aiStep`; the `EnderDragonFight` (`updateDragon`,
//! `setDragonKilled`, `aliveCrystals`, `onCrystalDestroyed`); `handleKillingBlow` (the dying phase), `tickDeath` (the death
//! animation, experience and removal), `kill(level)`; the node path finding (`findClosestNode`, `findPath`); the head look vector.

use super::flight_history::DragonFlightHistory;
use super::part::{BODY, EnderDragonPart, HEAD, NECK, TAIL_1, WING_1, WING_2};
use super::phase::EnderDragonPhase;
use crate::define_accessor;
use crate::server::level::ServerLevel;
use crate::world::damagesource::{DamageSource, DamageTypeTags};
use crate::world::entity::boss::end_crystal::EndCrystal;
use crate::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use crate::world::entity::living_entity::damage;
use crate::world::entity::mob::{self, MobData, MobKind};
use crate::world::entity::scenario_dump::{SCENARIO_DUMP_EXTRA, ScenarioDumpExtra};
use crate::world::entity::{Entity, EntityId};
use crate::world::level::block::BlockStateExt as _;
use crate::world::level::{BlockGetter, LevelAccessor};
use linkme::distributed_slice;
use mc_core::math::mth;
use mc_core::phys::{AABB, Vec3};
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::BlockState;
use mc_data::{EntityType, SoundEvent};
use mc_nbt::CompoundTag;
use mc_protocol::packets::play::SoundSource;

define_accessor!(pub DATA_PHASE: Int = EnderDragon);

/// `(float)(Math.PI / 180.0)`.
const DEG_TO_RAD: f32 = 0.017453292;
/// `SITTING_ALLOWED_DAMAGE_PERCENTAGE`.
const SITTING_ALLOWED_DAMAGE_PERCENTAGE: f32 = 0.25;

/// `EnderDragon`.
pub struct EnderDragon {
    pub mob: MobData,
    /// The parts while the level does not track the dragon (it holds them otherwise).
    pending_parts: Vec<Box<dyn Entity>>,
    /// The ids of `subEntities` (head, neck, body, three tail parts, two wings), allocated right after the dragon's.
    part_ids: Vec<EntityId>,
    /// `flightHistory`.
    pub flight_history: DragonFlightHistory,
    /// `oFlapTime`, `flapTime`.
    pub o_flap_time: f32,
    pub flap_time: f32,
    /// `inWall`.
    pub in_wall: bool,
    /// `dragonDeathTime`.
    pub dragon_death_time: i32,
    /// `yRotA`.
    pub y_rot_a: f32,
    /// `nearestCrystal`.
    nearest_crystal: Option<EntityId>,
    /// The current phase of the phase manager.
    phase: EnderDragonPhase,
    /// `sittingDamageReceived`.
    sitting_damage_received: f32,
}

impl EnderDragon {
    /// `new EnderDragon(type, level)`: the parts are created here (each draws an entity seed and UUID like any entity); their
    /// ids come from [`Entity::allocate_sub_entity_ids`].
    pub fn new(ty: EntityType, id: EntityId) -> Self {
        let mut mob = MobData::new(ty, id, MobKind::Mob, |b| {
            b.define(DATA_PHASE, EnderDragonPhase::Hovering.id());
        });
        mob.enemy = true;
        let parts = (0..super::part::DRAGON_PARTS.len())
            .map(|index| Box::new(EnderDragonPart::new(ty, 0, id, index)) as Box<dyn Entity>)
            .collect();
        let max_health = mob.living.get_max_health();
        mob.living.set_health(max_health);
        mob.living.entity.no_physics = true;
        Self {
            mob,
            pending_parts: parts,
            part_ids: Vec::new(),
            flight_history: DragonFlightHistory::default(),
            o_flap_time: 0.0,
            flap_time: 0.0,
            in_wall: false,
            dragon_death_time: 0,
            y_rot_a: 0.0,
            nearest_crystal: None,
            phase: EnderDragonPhase::Hovering,
            sitting_damage_received: 0.0,
        }
    }

    /// `getPhaseManager().getCurrentPhase().getPhase()`.
    pub fn phase(&self) -> EnderDragonPhase {
        self.phase
    }

    /// `getPhaseManager().setPhase(phase)`: a new phase is published through the synched data.
    pub fn set_phase(&mut self, phase: EnderDragonPhase) {
        if phase != self.phase {
            self.phase = phase;
            self.mob
                .living
                .entity
                .entity_data
                .set(DATA_PHASE, phase.id());
        }
    }

    /// The ids of the parts, in `subEntities` order.
    pub fn part_ids(&self) -> &[EntityId] {
        &self.part_ids
    }

    /// `hurt(level, part, source, damage)` for the part at `part` (an index of [`super::part::DRAGON_PARTS`]).
    pub fn hurt_part(
        &mut self,
        level: &mut ServerLevel,
        part: usize,
        source: &DamageSource,
        mut damage: f32,
    ) -> bool {
        if self.phase == EnderDragonPhase::Dying {
            return false;
        }
        // Not implemented: `getCurrentPhase().onHurt(source, damage)` (the sitting phases stop arrows)
        if part != HEAD && part != NECK {
            damage = damage / 4.0 + damage.min(1.0);
        }
        if damage < 0.01 {
            return false;
        }
        if source.causing_is_player()
            || source.is_tag(level, DamageTypeTags::ALWAYS_HURTS_ENDER_DRAGONS)
        {
            let health_before = self.get_health();
            self.really_hurt(level, source, damage);
            if self.phase.is_sitting() {
                self.sitting_damage_received =
                    self.sitting_damage_received + health_before - self.get_health();
                let max_health = self.mob.living.get_max_health();
                if self.sitting_damage_received > SITTING_ALLOWED_DAMAGE_PERCENTAGE * max_health {
                    self.sitting_damage_received = 0.0;
                    self.set_phase(EnderDragonPhase::Takeoff);
                }
            }
        }
        true
    }

    /// `reallyHurt(level, source, damage)`: `LivingEntity.hurtServer`.
    fn really_hurt(&mut self, level: &mut ServerLevel, source: &DamageSource, damage: f32) {
        damage::hurt_server(self, level, source, damage);
    }

    /// `tickPart(part, x, y, z)`.
    fn tick_part(&self, level: &mut ServerLevel, part: usize, x: f64, y: f64, z: f64) {
        let position = self.base().position();
        if let Some(part) = level.dragon_part_mut(self.part_ids[part]) {
            part.set_pos(position.x + x, position.y + y, position.z + z);
        }
    }

    /// `getHeadYOffset()`.
    fn get_head_y_offset(&self) -> f32 {
        if self.phase.is_sitting() {
            -1.0
        } else {
            let p0 = self.flight_history.get(5);
            let p1 = self.flight_history.get(0);
            (p0.y - p1.y) as f32
        }
    }

    /// `checkCrystals()`.
    fn check_crystals(&mut self, level: &mut ServerLevel) {
        if let Some(crystal) = self.nearest_crystal {
            if level.get_entity(crystal).is_none_or(|c| c.is_removed()) {
                self.nearest_crystal = None;
            } else if self.base().tick_count % 10 == 0
                && self.get_health() < self.mob.living.get_max_health()
            {
                let health = self.get_health();
                self.mob.living.set_health(health + 1.0);
            }
        }
        if self.base_mut().random.next_int_bounded(10) == 0 {
            let area = self.base().bounding_box().inflate_scalar(32.0);
            let crystals =
                level.get_entities_of_class(&area, |e| e.as_any().is::<EndCrystal>(), |_| true);
            let mut nearest = None;
            let mut distance = f64::MAX;
            for id in crystals {
                let Some(crystal) = level.get_entity(id) else {
                    continue;
                };
                let dist = crystal.base().distance_to_sqr_vec(self.base().position());
                if dist < distance {
                    distance = dist;
                    nearest = Some(id);
                }
            }
            self.nearest_crystal = nearest;
        }
    }

    /// `checkWalls(level, bb)`: breaks the blocks of the box that are neither transparent nor immune to the dragon (when mobs
    /// may grief) and reports whether a block stopped it.
    fn check_walls(&mut self, level: &mut ServerLevel, bb: &AABB) -> bool {
        let x0 = mth::floor_f64(bb.min_x);
        let y0 = mth::floor_f64(bb.min_y);
        let z0 = mth::floor_f64(bb.min_z);
        let x1 = mth::floor_f64(bb.max_x);
        let y1 = mth::floor_f64(bb.max_y);
        let z1 = mth::floor_f64(bb.max_z);
        let mut hit_wall = false;
        let mut destroyed_block = false;
        for x in x0..=x1 {
            for y in y0..=y1 {
                for z in z0..=z1 {
                    let pos = BlockPos::new(x, y, z);
                    let state: BlockState = level.get_block_state(pos);
                    if !state.is_air() && !state.is_tag("dragon_transparent") {
                        if level.mob_griefing() && !state.is_tag("dragon_immune") {
                            destroyed_block = level.remove_block(pos, false) || destroyed_block;
                        } else {
                            hit_wall = true;
                        }
                    }
                }
            }
        }
        if destroyed_block {
            let random = &mut self.base_mut().random;
            let random_pos = BlockPos::new(
                x0 + random.next_int_bounded(x1 - x0 + 1),
                y0 + random.next_int_bounded(y1 - y0 + 1),
                z0 + random.next_int_bounded(z1 - z0 + 1),
            );
            level.level_event(None, 2008, random_pos, 0);
        }
        hit_wall
    }

    /// The part placement of `aiStep` (everything after the flight): the body, wings, neck, head and tail follow the dragon, then
    /// `xo`/`yo`/`zo` of every part is the position it had before.
    fn place_parts(&mut self, level: &mut ServerLevel) {
        let old_pos: Vec<Vec3> = self
            .part_ids
            .iter()
            .map(|id| level.dragon_part(*id).map_or(Vec3::ZERO, |p| p.position()))
            .collect();
        let tilt = (self.flight_history.get(5).y - self.flight_history.get(10).y) as f32
            * 10.0
            * DEG_TO_RAD;
        let cc_tilt = mth::cos(f64::from(tilt));
        let ss_tilt = mth::sin(f64::from(tilt));
        let y_rot = self.base().y_rot();
        let rot1 = y_rot * DEG_TO_RAD;
        let ss1 = mth::sin(f64::from(rot1));
        let cc1 = mth::cos(f64::from(rot1));
        self.tick_part(
            level,
            BODY,
            f64::from(ss1 * 0.5),
            0.0,
            f64::from(-cc1 * 0.5),
        );
        self.tick_part(
            level,
            WING_1,
            f64::from(cc1 * 4.5),
            2.0,
            f64::from(ss1 * 4.5),
        );
        self.tick_part(
            level,
            WING_2,
            f64::from(cc1 * -4.5),
            2.0,
            f64::from(ss1 * -4.5),
        );
        // Not implemented: the knockback of the wings and the contact damage of the head and neck (`!wasHurtRecently()`)
        let rot2 = y_rot * DEG_TO_RAD - self.y_rot_a * 0.01;
        let ss2 = mth::sin(f64::from(rot2));
        let cc2 = mth::cos(f64::from(rot2));
        let y_offset = self.get_head_y_offset();
        self.tick_part(
            level,
            HEAD,
            f64::from(ss2 * 6.5 * cc_tilt),
            f64::from(y_offset + ss_tilt * 6.5),
            f64::from(-cc2 * 6.5 * cc_tilt),
        );
        self.tick_part(
            level,
            NECK,
            f64::from(ss2 * 5.5 * cc_tilt),
            f64::from(y_offset + ss_tilt * 5.5),
            f64::from(-cc2 * 5.5 * cc_tilt),
        );
        let p1 = self.flight_history.get(5);
        for i in 0..3 {
            let p0 = self.flight_history.get(12 + i * 2);
            let wrapped = mth::wrap_degrees_f64(f64::from(p0.y_rot - p1.y_rot)) as f32;
            let rot = y_rot * DEG_TO_RAD + wrapped * DEG_TO_RAD;
            let ss = mth::sin(f64::from(rot));
            let cc = mth::cos(f64::from(rot));
            let dd = (i + 1) as f32 * 2.0;
            self.tick_part(
                level,
                TAIL_1 + i as usize,
                f64::from(-(ss1 * 1.5 + ss * dd) * cc_tilt),
                p0.y - p1.y - f64::from((dd + 1.5) * ss_tilt) + 1.5,
                f64::from((cc1 * 1.5 + cc * dd) * cc_tilt),
            );
        }
        let head_box = level
            .dragon_part(self.part_ids[HEAD])
            .map(|p| p.get_bounding_box());
        let neck_box = level
            .dragon_part(self.part_ids[NECK])
            .map(|p| p.get_bounding_box());
        let body_box = level
            .dragon_part(self.part_ids[BODY])
            .map(|p| p.get_bounding_box());
        let mut in_wall = false;
        for bb in [head_box, neck_box, body_box].into_iter().flatten() {
            in_wall |= self.check_walls(level, &bb);
        }
        self.in_wall = in_wall;
        // Not implemented: `EnderDragonFight.updateDragon(this)`
        for (id, old) in self.part_ids.iter().zip(old_pos) {
            if let Some(part) = level.dragon_part_mut(*id) {
                part.base_mut().set_old_pos(old);
            }
        }
    }
}

impl Entity for EnderDragon {
    crate::mob_entity_accessors!(mob);

    fn allocate_sub_entity_ids(&mut self, next_id: &mut dyn FnMut() -> EntityId) {
        self.part_ids = self
            .pending_parts
            .iter_mut()
            .map(|part| {
                part.base_mut().id = next_id();
                part.base().id
            })
            .collect();
    }

    fn sub_entity_ids(&self) -> &[EntityId] {
        &self.part_ids
    }

    fn take_sub_entities(&mut self) -> Vec<Box<dyn Entity>> {
        std::mem::take(&mut self.pending_parts)
    }

    fn restore_sub_entities(&mut self, parts: Vec<Box<dyn Entity>>) {
        self.pending_parts = parts;
    }

    /// `aiStep()`: the dragon does not run `Mob.aiStep`.
    fn ai_step(&mut self, level: &mut ServerLevel) {
        self.process_flapping_movement(level);
        self.o_flap_time = self.flap_time;
        if self.is_dead_or_dying() {
            // the offsets of the explosion particle (`addParticle` itself is client only)
            for _ in 0..3 {
                self.base_mut().random.next_float();
            }
            return;
        }
        self.check_crystals(level);
        let movement = self.base().delta_movement();
        let mut flap_speed = 0.2 / (movement.horizontal_distance() as f32 * 10.0 + 1.0);
        flap_speed *= f64::powf(2.0, movement.y) as f32;
        if self.phase.is_sitting() {
            self.flap_time += 0.1;
        } else if self.in_wall {
            self.flap_time += flap_speed * 0.5;
        } else {
            self.flap_time += flap_speed;
        }
        let wrapped = mth::wrap_degrees_f32(self.base().y_rot());
        self.base_mut().set_y_rot(wrapped);
        if self.mob.is_no_ai() {
            // the parts stay where they are: they are placed only by the AI branch
            self.flap_time = 0.5;
        } else {
            let (y, y_rot) = (self.base().y(), self.base().y_rot());
            self.flight_history.record(y, y_rot);
            // Not implemented: the phase tick and the flight to the phase's target location
            self.apply_effects_from_blocks(level);
            self.mob.living.y_body_rot = self.base().y_rot();
            self.place_parts(level);
        }
    }

    fn is_flapping(&self) -> bool {
        let flap = mth::cos(f64::from(
            self.flap_time * (std::f64::consts::PI * 2.0) as f32,
        ));
        let old_flap = mth::cos(f64::from(
            self.o_flap_time * (std::f64::consts::PI * 2.0) as f32,
        ));
        old_flap <= -0.3 && flap >= -0.3
    }

    /// `hurtServer(level, source, damage)`: an attack on the body.
    fn hurt_server(&mut self, level: &mut ServerLevel, source: &DamageSource, amount: f32) -> bool {
        self.hurt_part(level, BODY, source, amount)
    }

    fn check_despawn(&mut self, _level: &mut ServerLevel) {}

    /// `knockback(..)`: a sitting dragon is not pushed.
    fn knockback(
        &mut self,
        power: f64,
        xd: f64,
        zd: f64,
        source: &DamageSource,
        damage: f32,
        comes_from_effect: bool,
    ) {
        if !self.phase.is_sitting() {
            damage::knockback(self, power, xd, zd, source, damage, comes_from_effect);
        }
    }

    fn is_pickable(&self) -> bool {
        false
    }

    fn get_sound_source(&self) -> SoundSource {
        SoundSource::Hostile
    }

    fn get_ambient_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_ENDER_DRAGON_AMBIENT)
    }

    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_ENDER_DRAGON_HURT)
    }

    fn get_sound_volume(&self) -> f32 {
        5.0
    }

    fn add_effect(
        &mut self,
        _level: &mut ServerLevel,
        _effect: crate::world::entity::effect::MobEffectInstance,
        _source: Option<EntityId>,
    ) -> bool {
        false
    }

    fn can_ride(&self, _vehicle: &dyn Entity) -> bool {
        false
    }

    fn can_use_portal(&self, _ignore_passenger: bool) -> bool {
        false
    }

    fn can_attack(&self, _level: &ServerLevel, target: &dyn Entity) -> bool {
        target.can_be_seen_as_enemy()
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        mob::nbt::add_additional_save_data(self, out);
        out.put_int("DragonPhase", self.phase.id());
        out.put_int("DragonDeathTime", self.dragon_death_time);
        out.put_float("sitting_damage_received", self.sitting_damage_received);
    }

    fn read_additional_save_data(&mut self, level: Option<&ServerLevel>, input: &CompoundTag) {
        mob::nbt::read_additional_save_data(self, level, input);
        if let Some(phase) = input.get_int("DragonPhase") {
            self.set_phase(EnderDragonPhase::by_id(phase));
        }
        self.dragon_death_time = input.get_int_or("DragonDeathTime", 0);
        self.sitting_damage_received = input.get_float_or("sitting_damage_received", 0.0);
    }
}

/// The scenario dump fields of an ender dragon: `dphase flap inwall dtime sitdmg`.
#[distributed_slice(SCENARIO_DUMP_EXTRA)]
static ENDER_DRAGON_DUMP: ScenarioDumpExtra = ScenarioDumpExtra {
    order: 22,
    applies: |e| e.as_any().is::<EnderDragon>(),
    fields: |e, _level, _ctx| {
        let dragon = e.as_any().downcast_ref::<EnderDragon>().expect("applies");
        vec![
            ("dphase".into(), dragon.phase.id().to_string()),
            (
                "flap".into(),
                format!("{}/{}", dragon.o_flap_time, dragon.flap_time),
            ),
            ("inwall".into(), u8::from(dragon.in_wall).to_string()),
            ("dtime".into(), dragon.dragon_death_time.to_string()),
            ("sitdmg".into(), dragon.sitting_damage_received.to_string()),
        ]
    },
};

#[distributed_slice(ENTITY_FACTORIES)]
static ENDER_DRAGON: EntityRegistration = EntityRegistration {
    entity_type: EntityType::ENDER_DRAGON,
    create: |ty, id, _registries| Box::new(EnderDragon::new(ty, id)),
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::entity::RemovalReason;
    use crate::world::entity::factory::{self, EntitySpawnRequest};
    use crate::world::level::entity::Visibility;
    use mc_core::pos::chunk_pos::ChunkPos;
    use mc_data::enums::EntitySpawnReason;

    fn level() -> ServerLevel {
        let mut level = ServerLevel::detached(
            mc_core::ident::Identifier::parse("minecraft:overworld").unwrap(),
        );
        level.use_private_entity_counter(100);
        level
            .entity_manager
            .update_chunk_status(ChunkPos::new(0, 0), Visibility::Ticking);
        level
    }

    fn summon(level: &mut ServerLevel, x: f64, y: f64, z: f64) -> EntityId {
        let request = EntitySpawnRequest {
            reason: EntitySpawnReason::Command,
            ignore_checks: true,
        };
        let mut dragon = factory::create(EntityType::ENDER_DRAGON, level, request).expect("dragon");
        dragon.snap_to_pos(x, y, z);
        let id = dragon.id();
        assert!(level.add_fresh_entity(dragon));
        id
    }

    fn origin_box() -> AABB {
        AABB::new(-1.0, -1.0, -1.0, 1.0, 1.0, 1.0)
    }

    #[test]
    fn the_parts_take_the_ids_after_the_dragon_and_are_not_entities_of_the_level() {
        let mut level = level();
        let id = summon(&mut level, 4.5, 70.0, 4.5);
        assert_eq!(id, 101);
        let part_ids = level.get_entity(id).unwrap().sub_entity_ids().to_vec();
        assert_eq!(part_ids, (102..=109).collect::<Vec<_>>());
        assert_eq!(level.next_entity_id(), 110);
        for part in part_ids {
            assert!(level.get_entity(part).is_none());
            assert_eq!(level.get_entity_or_part(part).map(|p| p.id()), Some(part));
            assert_eq!(level.dragon_part_parent(part), Some(id));
        }
        // a part of the dragon's type, sized by its table row, at the origin until the dragon places it
        let wing = level.dragon_part(108).unwrap();
        assert_eq!(wing.get_type(), EntityType::ENDER_DRAGON);
        assert_eq!(
            wing.get_bounding_box(),
            AABB::new(-2.0, 0.0, -2.0, 2.0, 2.0, 2.0)
        );
        assert!(wing.is_pickable());
    }

    #[test]
    fn a_box_query_returns_the_parts_after_the_entities() {
        let mut level = level();
        let id = summon(&mut level, 4.5, 70.0, 4.5);
        let near_origin = level.get_entities(None, &origin_box(), |_| true);
        assert_eq!(near_origin.len(), 8);
        assert!(near_origin.iter().all(|p| (102..=109).contains(p)));
        // a part is skipped for its own id and for its dragon's, whatever the selector says
        assert!(
            level
                .get_entities(Some(id), &origin_box(), |_| true)
                .is_empty()
        );
        let without_head = level.get_entities(Some(102), &origin_box(), |_| true);
        assert_eq!(without_head.len(), 7);
        assert!(!without_head.contains(&102));
        assert!(
            level
                .get_entities(None, &AABB::new(10.0, 10.0, 10.0, 11.0, 11.0, 11.0), |_| {
                    true
                })
                .is_empty()
        );
        // the selector sees a part as an entity of the dragon's type that is not living
        assert_eq!(
            level
                .get_entities(None, &origin_box(), |e| e.living().is_none()
                    && e.is_pickable())
                .len(),
            8
        );
    }

    #[test]
    fn a_class_query_that_finds_the_dragon_tests_its_parts_wherever_they_are() {
        let mut level = level();
        let id = summon(&mut level, 4.5, 70.0, 4.5);
        let around_dragon = AABB::new(4.0, 69.0, 4.0, 5.0, 71.0, 5.0);
        let of_type = level.get_entities_of_class(
            &around_dragon,
            |e| e.get_type() == EntityType::ENDER_DRAGON,
            |_| true,
        );
        assert_eq!(of_type.len(), 9);
        assert_eq!(of_type[0], id);
        let living =
            level.get_entities_of_class(&around_dragon, |e| e.living().is_some(), |_| true);
        assert_eq!(living, [id]);
        // the parts alone are not found by a box that misses the dragon
        assert!(
            level
                .get_entities_of_class(
                    &origin_box(),
                    |e| e.get_type() == EntityType::ENDER_DRAGON,
                    |_| true
                )
                .is_empty()
        );
    }

    #[test]
    fn parts_are_not_tracked_and_leave_with_their_dragon() {
        let mut level = level();
        let id = summon(&mut level, 4.5, 70.0, 4.5);
        assert!(level.entity_tracker.has_entity_with_id(id));
        assert!((102..=109).all(|p| !level.entity_tracker.has_entity_with_id(p)));
        level.with_entity(id, |level, dragon| {
            level.remove_held_entity(dragon, RemovalReason::Discarded)
        });
        assert!((102..=109).all(|p| level.get_entity_or_part(p).is_none()));
        assert!(level.get_entities(None, &origin_box(), |_| true).is_empty());
    }

    #[test]
    fn a_part_can_be_taken_out_and_is_put_back() {
        let mut level = level();
        summon(&mut level, 4.5, 70.0, 4.5);
        let moved = level.with_entity(104, |level, part| {
            part.set_pos(5.0, 6.0, 7.0);
            // out of its slot: queries skip it meanwhile
            assert!(level.dragon_part(104).is_none());
            part.position()
        });
        assert_eq!(moved, Some(Vec3::new(5.0, 6.0, 7.0)));
        assert_eq!(
            level.dragon_part(104).unwrap().position(),
            Vec3::new(5.0, 6.0, 7.0)
        );
        assert_eq!(
            level.dragon_part(104).unwrap().get_bounding_box().min_x,
            2.5
        );
    }
}
