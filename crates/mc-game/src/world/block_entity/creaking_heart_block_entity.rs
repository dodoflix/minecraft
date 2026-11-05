//! `CreakingHeartBlockEntity`: the protector creaking (UUID, resolved through the level), the server tick (state update,
//! spawning, particles, analog output) and the NBT.
//!
//! vanilla's `Either<Creaking, UUID> creakingInfo` is `creaking_info` (the UUID) plus `creaking_resolved` (`isLeft`): the
//! creaking entity itself is always found through the level (it may be out of its slot while it ticks, see
//! [`ServerLevel::entity_id_by_uuid`]).

use super::{BLOCK_ENTITY_TYPES, BlockEntity, BlockEntityData, BlockEntityRegistration, read_codec};
use crate::server::level::ServerLevel;
use crate::world::damagesource::DamageSource;
use crate::world::entity::monster::creaking::Creaking;
use crate::world::entity::spawn_util::{self, Strategy};
use crate::world::entity::{Entity, EntityId};
use crate::world::level::block::BlockStateExt;
use crate::world::level::block::impls::creaking_heart_block::has_required_logs;
use crate::world::level::block::impls::multiface_block;
use crate::world::level::{BlockGetter, LevelAccessor as _};
use linkme::distributed_slice;
use mc_core::direction::Direction;
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_core::pos::{BlockPos, TraversalNodeStatus};
use mc_core::random::RandomSource;
use mc_data::block::{Block, BlockState};
use mc_data::enums::{CreakingHeartState, EntitySpawnReason};
use mc_data::{BlockEntityType, GameEvent, SoundEvent, props};
use mc_nbt::CompoundTag;
use mc_protocol::components::uuid_codec;
use mc_protocol::packets::play::SoundSource;
use mc_protocol::packets::play::ParticleOptions;
use mc_protocol::packets::play::level::ClientboundBlockEntityDataPacket;
use uuid::Uuid;

const PLAYER_DETECTION_RANGE: f64 = 32.0;
/// `CREAKING_ROAMING_RADIUS`.
pub const CREAKING_ROAMING_RADIUS: i32 = 32;
const DISTANCE_CREAKING_TOO_FAR: f64 = 34.0;
const SPAWN_RANGE_XZ: i32 = 16;
const SPAWN_RANGE_Y: i32 = 8;
const ATTEMPTS_PER_SPAWN: i32 = 5;
const UPDATE_TICKS: i32 = 20;
const UPDATE_TICKS_VARIANCE: i32 = 5;
const HURT_CALL_TOTAL_TICKS: i32 = 100;
const MAX_DEPTH: i32 = 2;
const MAX_COUNT: i32 = 64;
const TICKS_GRACE_PERIOD: i64 = 30;
/// `Creaking.CREAKING_ORANGE` / `CREAKING_GRAY`.
const CREAKING_ORANGE: i32 = 16545810;
const CREAKING_GRAY: i32 = 6250335;

pub struct CreakingHeartBlockEntity {
    base: BlockEntityData,
    /// `creakingInfo` (`null` = `None`).
    creaking_info: Option<Uuid>,
    /// `creakingInfo.left().isPresent()`: the creaking entity was resolved.
    creaking_resolved: bool,
    ticks_existed: i64,
    ticker: i32,
    emitter: i32,
    emitter_target: Option<Vec3>,
    output_signal: i32,
}

impl CreakingHeartBlockEntity {
    pub fn new(pos: BlockPos, state: BlockState) -> Self {
        Self {
            base: BlockEntityData::new(BlockEntityType::CREAKING_HEART, pos, state),
            creaking_info: None,
            creaking_resolved: false,
            ticks_existed: 0,
            ticker: 0,
            emitter: 0,
            emitter_target: None,
            output_signal: 0,
        }
    }

    /// `getAnalogOutputSignal()`.
    pub fn get_analog_output_signal(&self) -> i32 {
        self.output_signal
    }

    /// `serverTick(level, pos, state, entity)` (the block entity is out of its chunk while it runs).
    fn tick_server(&mut self, level: &mut ServerLevel) {
        let pos = self.get_block_pos();
        let state = self.get_block_state();
        self.ticks_existed += 1;
        let computed_output_signal = self.compute_analog_output_signal(level);
        if self.output_signal != computed_output_signal {
            self.output_signal = computed_output_signal;
            level.update_neighbour_for_output_signal(pos, Block::CREAKING_HEART);
        }
        if self.emitter > 0 {
            if self.emitter > 50 {
                self.emit_particles(level, 1, true);
                self.emit_particles(level, 1, false);
            }
            if self.emitter % 10 == 0 && self.emitter_target.is_some() {
                if let Some(id) = self.get_creaking_protector(level)
                    && let Some(creaking) = level.get_entity(id)
                {
                    self.emitter_target = Some(creaking.get_bounding_box().get_center());
                }
                let heart_position = Vec3::at_center_of(&pos);
                let progress = 0.2 + 0.8 * (100 - self.emitter) as f32 / 100.0;
                let target = self.emitter_target.expect("checked above");
                let sound_location = heart_position.subtract(target).scale(f64::from(progress)).add(target);
                let sound_pos = BlockPos::containing_position(&sound_location);
                let volume = self.emitter as f32 / 2.0 / 100.0 + 0.5;
                level.play_sound(None, sound_pos, SoundEvent::BLOCK_CREAKING_HEART_HURT, SoundSource::Blocks, volume, 1.0);
            }
            self.emitter -= 1;
        }
        let ticker = self.ticker;
        self.ticker -= 1;
        if ticker < 0 {
            self.ticker = level.random().next_int_bounded(UPDATE_TICKS_VARIANCE) + UPDATE_TICKS;
            let updated_state = self.update_creaking_state(level, state, pos);
            if updated_state != state {
                level.set_block_and_update(pos, updated_state);
                if updated_state.get(props::CREAKING_HEART_STATE) == CreakingHeartState::Uprooted {
                    return;
                }
            }
            if self.creaking_info.is_none() {
                if updated_state.get(props::CREAKING_HEART_STATE) == CreakingHeartState::Awake
                    && level.is_spawning_monsters()
                    && level.difficulty() != mc_world::level_storage::Difficulty::Peaceful
                    && level.get_nearest_player(f64::from(pos.x()), f64::from(pos.y()), f64::from(pos.z()), PLAYER_DETECTION_RANGE, |p| !p.is_spectator()).is_some()
                    && let Some(creaking) = Self::spawn_protector(level, pos)
                {
                    self.set_creaking_info_entity(level, creaking);
                    level.with_entity(creaking, |level, e| {
                        if let Some(creaking) = e.as_any_mut().downcast_mut::<Creaking>() {
                            creaking.make_sound(level, SoundEvent::ENTITY_CREAKING_SPAWN);
                        }
                    });
                    level.play_sound(None, pos, SoundEvent::BLOCK_CREAKING_HEART_SPAWN, SoundSource::Blocks, 1.0, 1.0);
                }
            } else if let Some(creaking) = self.get_creaking_protector(level) {
                let still_active = level.attribute_bool(crate::world::attribute::attributes::CREAKING_ACTIVE, pos);
                let (persistent, stuck) = level
                    .with_entity(creaking, |level, e| {
                        e.as_any_mut().downcast_mut::<Creaking>().map(|c| (c.is_persistence_required(), c.player_is_stuck_in_you(level)))
                    })
                    .flatten()
                    .unwrap_or((false, false));
                if !still_active && !persistent || self.distance_to_creaking(level) > DISTANCE_CREAKING_TOO_FAR || stuck {
                    self.remove_protector(level, None);
                }
            }
        }
    }

    /// `updateCreakingState(level, state, pos, entity)`.
    fn update_creaking_state(&self, level: &ServerLevel, state: BlockState, pos: BlockPos) -> BlockState {
        if !has_required_logs(state, level, pos) && self.creaking_info.is_none() {
            state.set(props::CREAKING_HEART_STATE, CreakingHeartState::Uprooted)
        } else {
            let heart_state =
                if level.attribute_bool(crate::world::attribute::attributes::CREAKING_ACTIVE, pos) { CreakingHeartState::Awake } else { CreakingHeartState::Dormant };
            state.set(props::CREAKING_HEART_STATE, heart_state)
        }
    }

    /// `distanceToCreaking()`.
    fn distance_to_creaking(&mut self, level: &ServerLevel) -> f64 {
        let bottom_center = Vec3::at_bottom_center_of(&self.get_block_pos());
        self.get_creaking_protector(level).and_then(|id| level.get_entity(id)).map_or(0.0, |c| c.base().distance_to_sqr_vec(bottom_center).sqrt())
    }

    /// `clearCreakingInfo()`.
    fn clear_creaking_info(&mut self) {
        self.creaking_info = None;
        self.creaking_resolved = false;
    }

    /// `setCreakingInfo(creaking)`.
    pub fn set_creaking_info_entity(&mut self, level: &ServerLevel, creaking: EntityId) {
        self.creaking_info = level.entity_uuid_by_id(creaking);
        self.creaking_resolved = self.creaking_info.is_some();
    }

    /// `setCreakingInfo(uuid)`.
    pub fn set_creaking_info(&mut self, uuid: Uuid) {
        self.creaking_info = Some(uuid);
        self.creaking_resolved = false;
        self.ticks_existed = 0;
    }

    /// `getCreakingProtector()`.
    fn get_creaking_protector(&mut self, level: &ServerLevel) -> Option<EntityId> {
        let uuid = self.creaking_info?;
        let found = level.entity_id_by_uuid(uuid);
        if self.creaking_resolved {
            if found.is_some() {
                return found;
            }
            // `this.setCreakingInfo(creaking.getUUID())` of a removed creaking
            self.set_creaking_info(uuid);
        }
        match found.filter(|id| level.get_entity(*id).is_none_or(|e| e.as_any().is::<Creaking>())) {
            Some(id) => {
                self.creaking_resolved = true;
                Some(id)
            }
            None => {
                if self.ticks_existed >= TICKS_GRACE_PERIOD {
                    self.clear_creaking_info();
                }
                None
            }
        }
    }

    /// `spawnProtector(level, entity)`.
    fn spawn_protector(level: &mut ServerLevel, pos: BlockPos) -> Option<EntityId> {
        let creaking = spawn_util::try_spawn_mob(
            level,
            mc_data::EntityType::CREAKING,
            EntitySpawnReason::Spawner,
            pos,
            ATTEMPTS_PER_SPAWN,
            SPAWN_RANGE_XZ,
            SPAWN_RANGE_Y,
            Strategy::OnTopOfColliderNoLeaves,
            true,
        )?;
        let position = level.get_entity(creaking)?.position();
        level.game_event_from_entity(Some(creaking), GameEvent::ENTITY_PLACE, position);
        level.broadcast_entity_event(creaking, 60);
        level.with_entity(creaking, |_, e| {
            if let Some(creaking) = e.as_any_mut().downcast_mut::<Creaking>() {
                creaking.set_transient(pos);
            }
        });
        Some(creaking)
    }

    /// `creakingHurt()`: `creaking` is the protector that was just hurt (it is out of the level while it runs).
    pub fn creaking_hurt(&mut self, level: &mut ServerLevel, creaking: &dyn Entity) {
        if self.emitter <= 0 {
            self.emit_particles_for(level, creaking.get_bounding_box(), 20, false);
            if self.get_block_state().get(props::CREAKING_HEART_STATE) == CreakingHeartState::Awake {
                let number_of_clumps = mth::random_between_inclusive(level.random(), 2, 3);
                for _ in 0..number_of_clumps {
                    if let Some(block_pos) = self.spread_resin(level) {
                        level.play_sound(None, block_pos, SoundEvent::BLOCK_RESIN_PLACE, SoundSource::Blocks, 1.0, 1.0);
                        let state = self.get_block_state();
                        level.game_event_at_block(GameEvent::BLOCK_PLACE, block_pos, &crate::world::level::gameevent::GameEventContext::of_state(Some(state)));
                    }
                }
            }
            self.emitter = HURT_CALL_TOTAL_TICKS;
            self.emitter_target = Some(creaking.get_bounding_box().get_center());
        }
    }

    /// `spreadResin(level)`.
    fn spread_resin(&self, level: &mut ServerLevel) -> Option<BlockPos> {
        let mut placed_resin: Option<BlockPos> = None;
        let start = self.get_block_pos();
        // both lambdas of vanilla's traversal use the level's random source and block access
        let level_cell = std::cell::RefCell::new(level);
        BlockPos::breadth_first_traversal(
            start,
            MAX_DEPTH,
            MAX_COUNT,
            |pos, acceptor| {
                let mut level = level_cell.borrow_mut();
                for dir in mc_core::direction::shuffled_copy(&Direction::ALL, level.random()) {
                    let neighbour_pos = pos.relative(dir);
                    if level.get_block_state(neighbour_pos).is_tag("pale_oak_logs") {
                        acceptor(neighbour_pos);
                    }
                }
            },
            |pos| {
                let mut level = level_cell.borrow_mut();
                if !level.get_block_state(pos).is_tag("pale_oak_logs") {
                    return TraversalNodeStatus::Accept;
                }
                for dir in mc_core::direction::shuffled_copy(&Direction::ALL, level.random()) {
                    let neighbour_pos = pos.relative(dir);
                    let mut neighbour_state = level.get_block_state(neighbour_pos);
                    let opposite = dir.get_opposite();
                    if neighbour_state.is_air() {
                        neighbour_state = Block::RESIN_CLUMP.default_state();
                    } else if neighbour_state.block() == Block::WATER && neighbour_state.get_fluid_state().is_source() {
                        neighbour_state = Block::RESIN_CLUMP.default_state().set(props::WATERLOGGED, true);
                    }
                    if neighbour_state.block() == Block::RESIN_CLUMP && !multiface_block::has_face(neighbour_state, opposite) {
                        level.set_block_and_update(neighbour_pos, neighbour_state.set(multiface_block::get_face_property(opposite), true));
                        placed_resin = Some(neighbour_pos);
                        return TraversalNodeStatus::Stop;
                    }
                }
                TraversalNodeStatus::Accept
            },
        );
        placed_resin
    }

    /// `emitParticles(serverLevel, count, towardsCreaking)`.
    fn emit_particles(&mut self, level: &mut ServerLevel, count: i32, towards_creaking: bool) {
        if let Some(creaking) = self.get_creaking_protector(level).and_then(|id| level.get_entity(id)) {
            let bounding_box = creaking.get_bounding_box();
            self.emit_particles_for(level, bounding_box, count, towards_creaking);
        }
    }

    fn emit_particles_for(&self, level: &mut ServerLevel, bounding_box: mc_core::phys::AABB, count: i32, towards_creaking: bool) {
        let color = if towards_creaking { CREAKING_ORANGE } else { CREAKING_GRAY };
        let pos = self.get_block_pos();
        for _ in 0..count {
            let random = level.random();
            let min = bounding_box.get_min_position();
            let mut source = min.add_xyz(random.next_double() * bounding_box.get_xsize(), random.next_double() * bounding_box.get_ysize(), random.next_double() * bounding_box.get_zsize());
            let mut destination = Vec3::at_lower_corner_of(&pos).add_xyz(random.next_double(), random.next_double(), random.next_double());
            if towards_creaking {
                std::mem::swap(&mut source, &mut destination);
            }
            let duration = random.next_int_bounded(40) + 10;
            level.send_particles_full(
                ParticleOptions::Trail { target: destination, color, duration },
                true,
                true,
                source.x,
                source.y,
                source.z,
                1,
                0.0,
                0.0,
                0.0,
                0.0,
                0.0,
                0.0,
                crate::server::level::RandomizationType::Default,
            );
        }
    }

    /// `removeProtector(damageSource)`.
    pub fn remove_protector(&mut self, level: &mut ServerLevel, damage_source: Option<&DamageSource>) {
        if let Some(creaking_id) = self.get_creaking_protector(level) {
            level.with_entity(creaking_id, |level, e| {
                if let Some(creaking) = e.as_any_mut().downcast_mut::<Creaking>() {
                    match damage_source {
                        None => creaking.tear_down(level),
                        Some(source) => {
                            creaking.creaking_death_effects(level, source);
                            creaking.set_tearing_down();
                            creaking.set_health(0.0);
                        }
                    }
                }
            });
            self.clear_creaking_info();
        }
    }

    /// `isProtector(creaking)`.
    pub fn is_protector(&mut self, level: &ServerLevel, creaking: EntityId) -> bool {
        self.get_creaking_protector(level) == Some(creaking)
    }

    /// `computeAnalogOutputSignal()`.
    pub fn compute_analog_output_signal(&mut self, level: &ServerLevel) -> i32 {
        if self.creaking_info.is_some() && self.get_creaking_protector(level).is_some() {
            let distance = self.distance_to_creaking(level);
            let scaled_distance = distance.clamp(0.0, 32.0) / 32.0;
            15 - mth::floor_f64(scaled_distance * 15.0)
        } else {
            0
        }
    }
}

impl BlockEntity for CreakingHeartBlockEntity {
    block_entity_base!();

    fn load_additional(&mut self, tag: &CompoundTag) {
        match read_codec(tag, "creaking", &uuid_codec()) {
            Some(uuid) => self.set_creaking_info(uuid),
            None => self.clear_creaking_info(),
        }
    }

    fn save_additional(&self, tag: &mut CompoundTag) {
        if let Some(uuid) = &self.creaking_info
            && let Ok(encoded) = mc_dfu::encode_nbt(&uuid_codec(), uuid).into_result()
        {
            tag.put("creaking", encoded);
        }
    }

    /// `getTicker`: ticking unless the heart is uprooted.
    fn is_ticking(&self) -> bool {
        self.get_block_state().get(props::CREAKING_HEART_STATE) != CreakingHeartState::Uprooted
    }

    fn server_tick(&mut self, level: &mut ServerLevel) {
        self.tick_server(level);
    }

    /// `preRemoveSideEffects`: `removeProtector(null)`.
    fn pre_remove_side_effects(&mut self, level: &mut ServerLevel, _pos: BlockPos, _state: BlockState) {
        self.remove_protector(level, None);
    }

    /// `getUpdateTag`: `saveCustomOnly`.
    fn get_update_tag(&self) -> CompoundTag {
        let mut tag = CompoundTag::new();
        self.save_additional(&mut tag);
        tag
    }

    fn get_update_packet(&self) -> Option<ClientboundBlockEntityDataPacket> {
        Some(ClientboundBlockEntityDataPacket { pos: self.get_block_pos(), r#type: self.get_type(), tag: self.get_update_tag() })
    }
}

#[distributed_slice(BLOCK_ENTITY_TYPES)]
static CREAKING_HEART: BlockEntityRegistration =
    BlockEntityRegistration { ty: BlockEntityType::CREAKING_HEART, create: |pos, state| Box::new(CreakingHeartBlockEntity::new(pos, state)) };
