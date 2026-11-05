//! `net.minecraft.world.entity.Entity`: data ([`EntityData`]) + overridable behavior ([`Entity`]).
//!
//! Vanilla's `Level level` field is gone: methods that need the world take `level: &mut ServerLevel` (the entity is
//! taken out of the level's storage while it runs, see `world/entity/mod.rs`). Relations to other entities
//! (`vehicle`, `passengers`) are ids resolved through the level.

use super::dimensions::{EntityAttachment, EntityAttachments, EntityDimensions};
use super::synched_data::{EntityDataAccessor, SynchedEntityData, SynchedEntityDataBuilder, serializer};
use super::tag_set::JavaStringSet;
use super::vec_delta_codec::VecDeltaCodec;
use super::{EntityId, MoverType, RemovalReason};
use crate::define_accessor;
use crate::server::level::ServerLevel;
use mc_core::math::mth;
use mc_core::phys::{AABB, Vec2, Vec3};
use mc_core::pos::chunk_pos::ChunkPos;
use mc_core::pos::{BlockPos, SectionPos};
use mc_core::random::{LegacyRandomSource, RandomSource};
use mc_data::EntityType;
use mc_data::enums::Pose;
use mc_nbt::{CompoundTag, ListTag, Tag};
use mc_text::Component;
use std::any::Any;
use std::collections::VecDeque;
use uuid::Uuid;

pub const TAG_ID: &str = "id";
pub const TAG_UUID: &str = "UUID";
pub const TAG_PASSENGERS: &str = "Passengers";
pub const TAG_DATA: &str = "data";
pub const TAG_POS: &str = "Pos";
pub const TAG_MOTION: &str = "Motion";
pub const TAG_ROTATION: &str = "Rotation";
pub const BOARDING_COOLDOWN: i32 = 60;
pub const TOTAL_AIR_SUPPLY: i32 = 300;
pub const MAX_ENTITY_TAG_COUNT: usize = 1024;
pub const BASE_TICKS_REQUIRED_TO_FREEZE: i32 = 140;
pub const FREEZE_HURT_FREQUENCY: i32 = 40;
pub const BASE_SAFE_FALL_DISTANCE: i32 = 3;
pub const DEFAULT_BB_WIDTH: f32 = 0.6;
pub const DEFAULT_BB_HEIGHT: f32 = 1.8;
pub const MAX_MOVEMENTS_HANDLED_PER_TICK: usize = 100;

/// `Entity.FLAG_*` bits of `DATA_SHARED_FLAGS_ID`.
pub const FLAG_ONFIRE: i32 = 0;
pub const FLAG_SHIFT_KEY_DOWN: i32 = 1;
pub const FLAG_SPRINTING: i32 = 3;
pub const FLAG_SWIMMING: i32 = 4;
pub const FLAG_INVISIBLE: i32 = 5;
pub const FLAG_GLOWING: i32 = 6;
pub const FLAG_FALL_FLYING: i32 = 7;

define_accessor!(pub DATA_SHARED_FLAGS_ID: Byte = Entity);
define_accessor!(pub DATA_AIR_SUPPLY_ID: Int = Entity);
define_accessor!(pub DATA_CUSTOM_NAME: OptionalComponent = Entity);
define_accessor!(pub DATA_CUSTOM_NAME_VISIBLE: Boolean = Entity);
define_accessor!(pub DATA_SILENT: Boolean = Entity);
define_accessor!(pub DATA_NO_GRAVITY: Boolean = Entity);
define_accessor!(pub DATA_POSE: Pose = Entity);
define_accessor!(pub DATA_TICKS_FROZEN: Int = Entity);

/// `Entity.Movement`: a recorded move of this tick (used by `applyEffectsFromBlocks`).
#[derive(Clone, Copy, Debug)]
pub struct Movement {
    pub from: Vec3,
    pub to: Vec3,
    pub axis_dependent_original_movement: Option<Vec3>,
}

/// `MoveSimulationType`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MoveSimulationType {
    ServerAndClient,
    AuthoritativeSide,
    AuthoritativeSideAndServer,
}

/// `Entity.MovementEmission`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MovementEmission {
    None,
    Sounds,
    Events,
    All,
}

impl MovementEmission {
    pub fn emits_anything(self) -> bool {
        self != MovementEmission::None
    }
    pub fn emits_events(self) -> bool {
        matches!(self, MovementEmission::Events | MovementEmission::All)
    }
    pub fn emits_sounds(self) -> bool {
        matches!(self, MovementEmission::Sounds | MovementEmission::All)
    }
}

/// Fields of vanilla `Entity`.
pub struct EntityData {
    pub entity_type: EntityType,
    pub requires_precise_position: bool,
    pub id: EntityId,
    pub blocks_building: bool,
    /// `passengers` (ids; first element is a player passenger if any, see `add_passenger`).
    pub passengers: Vec<EntityId>,
    pub boarding_cooldown: i32,
    pub vehicle: Option<EntityId>,
    pub xo: f64,
    pub yo: f64,
    pub zo: f64,
    position: Vec3,
    block_position: BlockPos,
    chunk_position: ChunkPos,
    delta_movement: Vec3,
    y_rot: f32,
    x_rot: f32,
    pub y_rot_o: f32,
    pub x_rot_o: f32,
    bb: AABB,
    on_ground: bool,
    pub horizontal_collision: bool,
    pub vertical_collision: bool,
    pub vertical_collision_below: bool,
    pub minor_horizontal_collision: bool,
    pub stuck_speed_multiplier: Vec3,
    removal_reason: Option<RemovalReason>,
    pub move_dist: f32,
    pub fly_dist: f32,
    pub fall_distance: f64,
    pub next_step: f32,
    pub x_old: f64,
    pub y_old: f64,
    pub z_old: f64,
    pub no_physics: bool,
    pub random: LegacyRandomSource,
    pub tick_count: i32,
    pub remaining_fire_ticks: i32,
    pub was_touching_water: bool,
    pub was_eye_in_water: bool,
    /// `EntityFluidInteraction` result: fluid height above the box bottom / eyes inside, for [water, lava] (currents not modelled).
    pub fluid_height: [f64; 2],
    pub eye_in_fluid: [bool; 2],
    pub invulnerable_time: i32,
    pub first_tick: bool,
    pub entity_data: SynchedEntityData,
    pub packet_position_codec: VecDeltaCodec,
    /// `getInterpolation().interpolationTracker()` (server side).
    pub interpolation_tracker: super::interpolation::InterpolationTracker,
    pub needs_sync: bool,
    pub sync_position: bool,
    pub sync_velocity: bool,
    pub portal_cooldown: i32,
    pub permanently_invulnerable: bool,
    pub uuid: Uuid,
    pub has_glowing_tag: bool,
    pub tags: JavaStringSet,
    pub piston_deltas: [f64; 3],
    pub piston_deltas_game_time: i64,
    pub dimensions: EntityDimensions,
    pub eye_height: f32,
    pub is_in_powder_snow: bool,
    pub was_in_powder_snow: bool,
    pub main_supporting_block_pos: Option<BlockPos>,
    pub on_ground_no_blocks: bool,
    pub has_visual_fire: bool,
    pub last_known_speed: Vec3,
    pub last_known_position: Option<Vec3>,
    pub movement_this_tick: VecDeque<Movement>,
    pub final_movements_this_tick: Vec<Movement>,
    pub custom_data: CompoundTag,
    /// `levelCallback.onMove()` pending: the block position changed since the section manager last looked.
    pub moved_since_sync: bool,
    /// Dimensions grew in `refresh_dimensions` and `fudgePositionAfterSizeChange` still has to run (it needs the level).
    pub pending_fudge_from: Option<EntityDimensions>,
    // ==== [EntityBase] additions (EntityData field) ====
    /// `fluidInteraction` (trackers + current accumulators; `fluid_height`/`eye_in_fluid` mirror it for water and lava).
    pub fluid_interaction: super::fluid_interaction::EntityFluidInteraction,
    /// `portalProcess`.
    pub portal_process: Option<super::portal::PortalProcessor>,
    pub crystal_sound_intensity: f32,
    pub last_crystal_sound_play_tick: i32,
    /// `inBlockState` cache of `getInBlockState()`.
    pub in_block_state: Option<mc_data::block::BlockState>,
    /// `insideEffectCollector`.
    pub inside_effect_collector: super::inside_block_effects::StepBasedCollector,
    // ==== [Damage] additions (EntityData field) ====
    // ==== [Effects] additions (EntityData field) ====
    // ==== [Level] additions (EntityData field) ====
}

impl EntityData {
    /// `Entity(type, level)`: the id comes from `ServerLevel::next_entity_id`. `max_air_supply` is the value of
    /// the virtual `getMaxAirSupply()`; `define_synched_data` adds the subclass accessors (`defineSynchedData`).
    pub fn new(entity_type: EntityType, id: EntityId, max_air_supply: i32, define_synched_data: impl FnOnce(&mut SynchedEntityDataBuilder)) -> Self {
        let mut random = mc_core::random::create_new();
        let uuid = mth::create_insecure_uuid(&mut random);
        let dimensions = EntityDimensions::of_type(entity_type);
        let mut builder = SynchedEntityDataBuilder::new();
        builder.define(DATA_SHARED_FLAGS_ID, 0);
        builder.define(DATA_AIR_SUPPLY_ID, max_air_supply);
        builder.define(DATA_CUSTOM_NAME_VISIBLE, false);
        builder.define(DATA_CUSTOM_NAME, None);
        builder.define(DATA_SILENT, false);
        builder.define(DATA_NO_GRAVITY, false);
        builder.define(DATA_POSE, Pose::Standing);
        builder.define(DATA_TICKS_FROZEN, 0);
        define_synched_data(&mut builder);
        let mut data = Self {
            entity_type,
            requires_precise_position: false,
            id,
            blocks_building: false,
            passengers: Vec::new(),
            boarding_cooldown: 0,
            vehicle: None,
            xo: 0.0,
            yo: 0.0,
            zo: 0.0,
            position: Vec3::ZERO,
            block_position: BlockPos::ZERO,
            chunk_position: ChunkPos::ZERO,
            delta_movement: Vec3::ZERO,
            y_rot: 0.0,
            x_rot: 0.0,
            y_rot_o: 0.0,
            x_rot_o: 0.0,
            bb: AABB::new(0.0, 0.0, 0.0, 0.0, 0.0, 0.0),
            on_ground: false,
            horizontal_collision: false,
            vertical_collision: false,
            vertical_collision_below: false,
            minor_horizontal_collision: false,
            stuck_speed_multiplier: Vec3::ZERO,
            removal_reason: None,
            move_dist: 0.0,
            fly_dist: 0.0,
            fall_distance: 0.0,
            next_step: 1.0,
            x_old: 0.0,
            y_old: 0.0,
            z_old: 0.0,
            no_physics: false,
            random,
            tick_count: 0,
            remaining_fire_ticks: 0,
            was_touching_water: false,
            was_eye_in_water: false,
            fluid_height: [0.0; 2],
            eye_in_fluid: [false; 2],
            invulnerable_time: 0,
            first_tick: true,
            entity_data: builder.build(),
            packet_position_codec: VecDeltaCodec::default(),
            interpolation_tracker: Default::default(),
            needs_sync: false,
            sync_position: false,
            sync_velocity: false,
            portal_cooldown: 0,
            permanently_invulnerable: false,
            uuid,
            has_glowing_tag: false,
            tags: JavaStringSet::new(),
            piston_deltas: [0.0; 3],
            piston_deltas_game_time: 0,
            eye_height: dimensions.eye_height,
            dimensions,
            is_in_powder_snow: false,
            was_in_powder_snow: false,
            main_supporting_block_pos: None,
            on_ground_no_blocks: false,
            has_visual_fire: false,
            last_known_speed: Vec3::ZERO,
            last_known_position: None,
            movement_this_tick: VecDeque::new(),
            final_movements_this_tick: Vec::new(),
            custom_data: CompoundTag::new(),
            moved_since_sync: false,
            pending_fudge_from: None,
            // ==== [EntityBase] additions (EntityData::new initializer) ====
            fluid_interaction: Default::default(),
            portal_process: None,
            crystal_sound_intensity: 0.0,
            last_crystal_sound_play_tick: 0,
            in_block_state: None,
            inside_effect_collector: Default::default(),
            // ==== [Damage] additions (EntityData::new initializer) ====
            // ==== [Effects] additions (EntityData::new initializer) ====
            // ==== [Level] additions (EntityData::new initializer) ====
        };
        data.set_pos_raw_and_box(0.0, 0.0, 0.0);
        data
    }

    // ---- position -------------------------------------------------------------------------------------------

    pub fn position(&self) -> Vec3 {
        self.position
    }
    pub fn block_position(&self) -> BlockPos {
        self.block_position
    }
    pub fn chunk_position(&self) -> ChunkPos {
        self.chunk_position
    }
    pub fn x(&self) -> f64 {
        self.position.x
    }
    pub fn y(&self) -> f64 {
        self.position.y
    }
    pub fn z(&self) -> f64 {
        self.position.z
    }
    pub fn get_block_x(&self) -> i32 {
        self.block_position.x()
    }
    pub fn get_block_y(&self) -> i32 {
        self.block_position.y()
    }
    pub fn get_block_z(&self) -> i32 {
        self.block_position.z()
    }
    pub fn y_rot(&self) -> f32 {
        self.y_rot
    }
    pub fn x_rot(&self) -> f32 {
        self.x_rot
    }
    pub fn bounding_box(&self) -> AABB {
        self.bb
    }
    pub fn set_bounding_box(&mut self, bb: AABB) {
        self.bb = bb;
    }
    pub fn on_ground(&self) -> bool {
        self.on_ground
    }
    /// `onGround = ...` without the supporting-block check (callers use `set_on_ground*`).
    pub fn set_on_ground_raw(&mut self, on_ground: bool) {
        self.on_ground = on_ground;
    }
    pub fn delta_movement(&self) -> Vec3 {
        self.delta_movement
    }
    pub fn removal_reason(&self) -> Option<RemovalReason> {
        self.removal_reason
    }
    pub fn is_removed(&self) -> bool {
        self.removal_reason.is_some()
    }

    /// `setDeltaMovement(Vec3)`: ignored unless finite.
    pub fn set_delta_movement(&mut self, delta_movement: Vec3) {
        if delta_movement.is_finite() {
            self.delta_movement = delta_movement;
        }
    }

    pub fn set_delta_movement_xyz(&mut self, xd: f64, yd: f64, zd: f64) {
        self.set_delta_movement(Vec3::new(xd, yd, zd));
    }

    pub fn add_delta_movement(&mut self, momentum: Vec3) {
        if momentum.is_finite() {
            self.set_delta_movement(self.delta_movement.add(momentum));
        }
    }

    /// `setPosRaw(x, y, z)` (final in vanilla).
    pub fn set_pos_raw(&mut self, x: f64, y: f64, z: f64) {
        if self.position.x != x || self.position.y != y || self.position.z != z {
            self.position = Vec3::new(x, y, z);
            let fx = mth::floor_f64(x);
            let fy = mth::floor_f64(y);
            let fz = mth::floor_f64(z);
            if fx != self.block_position.x() || fy != self.block_position.y() || fz != self.block_position.z() {
                self.block_position = BlockPos::new(fx, fy, fz);
                self.in_block_state = None;
                if SectionPos::block_to_section_coord_i32(fx) != self.chunk_position.x || SectionPos::block_to_section_coord_i32(fz) != self.chunk_position.z {
                    self.chunk_position = ChunkPos::containing(&self.block_position);
                }
                self.moved_since_sync = true;
            }
        }
    }

    /// `setPosRaw` + `setBoundingBox(makeBoundingBox())` with the default dimensions (`setPos(x,y,z)` of vanilla).
    pub fn set_pos_raw_and_box(&mut self, x: f64, y: f64, z: f64) {
        self.set_pos_raw(x, y, z);
        self.bb = self.dimensions.make_bounding_box_vec(self.position);
    }

    /// `setYRot` (ignores non-finite values).
    pub fn set_y_rot(&mut self, y_rot: f32) {
        if y_rot.is_finite() {
            self.y_rot = y_rot;
        }
    }

    /// `setXRot`: `Math.clamp(xRot % 360, -90, 90)`.
    pub fn set_x_rot(&mut self, x_rot: f32) {
        if x_rot.is_finite() {
            self.x_rot = (x_rot % 360.0).clamp(-90.0, 90.0);
        }
    }

    /// `setRot(yRot, xRot)`.
    pub fn set_rot(&mut self, y_rot: f32, x_rot: f32) {
        self.set_y_rot(y_rot % 360.0);
        self.set_x_rot(x_rot % 360.0);
    }

    pub fn set_old_pos(&mut self, position: Vec3) {
        self.xo = position.x;
        self.x_old = position.x;
        self.yo = position.y;
        self.y_old = position.y;
        self.zo = position.z;
        self.z_old = position.z;
    }

    pub fn set_old_rot_values(&mut self, y_rot: f32, x_rot: f32) {
        self.y_rot_o = y_rot;
        self.x_rot_o = x_rot;
    }

    /// `setOldPosAndRot()`.
    pub fn set_old_pos_and_rot(&mut self) {
        self.set_old_pos(self.position);
        self.set_old_rot_values(self.y_rot, self.x_rot);
    }

    pub fn old_position(&self) -> Vec3 {
        Vec3::new(self.x_old, self.y_old, self.z_old)
    }

    // ---- shared flags -------------------------------------------------------------------------------------------

    pub fn get_shared_flag(&self, flag: i32) -> bool {
        (self.entity_data.get(DATA_SHARED_FLAGS_ID) as i32 & (1 << flag)) != 0
    }

    /// `setSharedFlag` (the shared-flags accessor has no `onSyncedDataUpdated` reaction).
    pub fn set_shared_flag(&mut self, flag: i32, value: bool) {
        let current = self.entity_data.get(DATA_SHARED_FLAGS_ID);
        let new = if value { (current as i32 | (1 << flag)) as i8 } else { (current as i32 & !(1 << flag)) as i8 };
        self.entity_data.set(DATA_SHARED_FLAGS_ID, new);
    }

    pub fn get_air_supply(&self) -> i32 {
        self.entity_data.get(DATA_AIR_SUPPLY_ID)
    }

    pub fn set_air_supply(&mut self, supply: i32) {
        self.entity_data.set(DATA_AIR_SUPPLY_ID, supply);
    }

    pub fn get_ticks_frozen(&self) -> i32 {
        self.entity_data.get(DATA_TICKS_FROZEN)
    }

    pub fn set_ticks_frozen(&mut self, ticks: i32) {
        self.entity_data.set(DATA_TICKS_FROZEN, ticks);
    }

    pub fn get_custom_name(&self) -> Option<Component> {
        self.entity_data.get(DATA_CUSTOM_NAME)
    }

    pub fn set_custom_name(&mut self, name: Option<Component>) {
        self.entity_data.set(DATA_CUSTOM_NAME, name);
    }

    pub fn has_custom_name(&self) -> bool {
        self.entity_data.get_ref(DATA_CUSTOM_NAME).is_some()
    }

    pub fn is_custom_name_visible(&self) -> bool {
        self.entity_data.get(DATA_CUSTOM_NAME_VISIBLE)
    }

    pub fn set_custom_name_visible(&mut self, visible: bool) {
        self.entity_data.set(DATA_CUSTOM_NAME_VISIBLE, visible);
    }

    pub fn is_silent(&self) -> bool {
        self.entity_data.get(DATA_SILENT)
    }

    pub fn set_silent(&mut self, silent: bool) {
        self.entity_data.set(DATA_SILENT, silent);
    }

    pub fn is_no_gravity(&self) -> bool {
        self.entity_data.get(DATA_NO_GRAVITY)
    }

    pub fn set_no_gravity(&mut self, no_gravity: bool) {
        self.entity_data.set(DATA_NO_GRAVITY, no_gravity);
    }

    pub fn get_pose(&self) -> Pose {
        self.entity_data.get(DATA_POSE)
    }

    /// `setRemoved` core: records the reason (first one wins). Deregistration from the level happens when the
    /// manager syncs the entity.
    pub fn set_removal_reason(&mut self, reason: RemovalReason) {
        if self.removal_reason.is_none() {
            self.removal_reason = Some(reason);
        }
    }

    /// `unsetRemoved()`.
    pub fn unset_removed(&mut self) {
        self.removal_reason = None;
    }

    pub fn add_tag(&mut self, tag: &str) -> bool {
        if self.tags.len() >= MAX_ENTITY_TAG_COUNT { false } else { self.tags.insert(tag) }
    }

    pub fn remove_tag(&mut self, tag: &str) -> bool {
        self.tags.remove(tag)
    }

    pub fn get_bb_width(&self) -> f32 {
        self.dimensions.width
    }

    pub fn get_bb_height(&self) -> f32 {
        self.dimensions.height
    }

    /// `getFluidJumpThreshold()`.
    pub fn get_fluid_jump_threshold(&self) -> f64 {
        if f64::from(self.eye_height) < 0.4 { 0.0 } else { 0.4 }
    }

    pub fn get_eye_y(&self) -> f64 {
        self.position.y + f64::from(self.eye_height)
    }

    pub fn get_x_progress(&self, progress: f64) -> f64 {
        self.position.x + f64::from(self.get_bb_width()) * progress
    }

    pub fn get_y_progress(&self, progress: f64) -> f64 {
        self.position.y + f64::from(self.get_bb_height()) * progress
    }

    pub fn get_z_progress(&self, progress: f64) -> f64 {
        self.position.z + f64::from(self.get_bb_width()) * progress
    }

    pub fn get_random_x(&mut self, spread: f64) -> f64 {
        let r = self.random.next_double();
        self.get_x_progress((2.0 * r - 1.0) * spread)
    }

    pub fn get_random_y_spread(&mut self, spread: f64) -> f64 {
        let r = self.random.next_double();
        self.get_y_progress((2.0 * r - 1.0) * spread)
    }

    pub fn get_random_y(&mut self) -> f64 {
        let r = self.random.next_double();
        self.get_y_progress(r)
    }

    pub fn get_random_z(&mut self, spread: f64) -> f64 {
        let r = self.random.next_double();
        self.get_z_progress((2.0 * r - 1.0) * spread)
    }

    pub fn distance_to_sqr_vec(&self, pos: Vec3) -> f64 {
        let xd = self.position.x - pos.x;
        let yd = self.position.y - pos.y;
        let zd = self.position.z - pos.z;
        xd * xd + yd * yd + zd * zd
    }

    pub fn distance_to_sqr_xyz(&self, x: f64, y: f64, z: f64) -> f64 {
        let xd = self.position.x - x;
        let yd = self.position.y - y;
        let zd = self.position.z - z;
        xd * xd + yd * yd + zd * zd
    }

    pub fn get_eye_position(&self) -> Vec3 {
        Vec3::new(self.position.x, self.get_eye_y(), self.position.z)
    }

    pub fn is_passenger(&self) -> bool {
        self.vehicle.is_some()
    }

    pub fn is_vehicle(&self) -> bool {
        !self.passengers.is_empty()
    }
}

/// Lookup of other entities for save/passenger traversal: `(id) -> entity currently in storage`.
pub type EntityLookupFn<'a> = dyn Fn(EntityId) -> Option<&'a dyn Entity> + 'a;

/// Vanilla `Entity` and its overridable behavior. Default impls are the vanilla base behavior; overriding
/// implementations call the parent behavior through the free functions of this module.
pub trait Entity: Any {
    fn base(&self) -> &EntityData;
    fn base_mut(&mut self) -> &mut EntityData;
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
    fn as_dyn(&self) -> &dyn Entity;
    fn as_dyn_mut(&mut self) -> &mut dyn Entity;

    fn id(&self) -> EntityId {
        self.base().id
    }
    fn uuid(&self) -> Uuid {
        self.base().uuid
    }
    fn get_type(&self) -> EntityType {
        self.base().entity_type
    }
    fn is_removed(&self) -> bool {
        self.base().is_removed()
    }
    fn position(&self) -> Vec3 {
        self.base().position()
    }
    fn block_position(&self) -> BlockPos {
        self.base().block_position()
    }
    fn chunk_position(&self) -> ChunkPos {
        self.base().chunk_position()
    }
    fn get_bounding_box(&self) -> AABB {
        self.base().bounding_box()
    }

    // ==== [Explosions] additions (Entity trait methods) ====

    /// `Explosion.getIndirectSourceEntity(this)` when this entity is an explosion source: a living entity is its own
    /// indirect source, a projectile's is its living owner (`PrimedTnt` overrides with its owner).
    fn explosion_indirect_source(&self, level: &ServerLevel) -> Option<EntityId> {
        if let Some(projectile) = self.as_projectile() {
            return projectile.get_owner(level).filter(|o| o.living().is_some()).map(|o| o.id());
        }
        self.living().map(|_| self.id())
    }

    /// `ignoreExplosion(explosion)`.
    fn ignore_explosion(&self, _explosion: &dyn crate::world::level::explosion::Explosion) -> bool {
        false
    }

    /// `getBlockExplosionResistance(explosion, level, pos, block, fluid, resistance)`.
    fn get_block_explosion_resistance(
        &self,
        _explosion: &dyn crate::world::level::explosion::Explosion,
        _level: &dyn crate::world::level::BlockGetter,
        _pos: BlockPos,
        _block: mc_data::block::BlockState,
        _fluid: mc_data::fluid::FluidState,
        resistance: f32,
    ) -> f32 {
        resistance
    }

    /// `shouldBlockExplode(explosion, level, pos, state, power)`.
    fn should_block_explode(
        &self,
        _explosion: &dyn crate::world::level::explosion::Explosion,
        _level: &dyn crate::world::level::BlockGetter,
        _pos: BlockPos,
        _state: mc_data::block::BlockState,
        _power: f32,
    ) -> bool {
        true
    }

    /// This entity as a `Projectile` (`instanceof Projectile`).
    fn as_projectile(&self) -> Option<&dyn super::projectile::Projectile> {
        None
    }
    fn as_projectile_mut(&mut self) -> Option<&mut dyn super::projectile::Projectile> {
        None
    }

    /// `isAffectedByPotions()` (`LivingEntity` true; `ArmorStand` overrides false).
    fn is_affected_by_potions(&self) -> bool {
        self.living().is_some()
    }

    /// `deflection(projectile)`: `DEFLECTS_PROJECTILES` entities reverse projectiles.
    fn deflection(&self, _projectile: &dyn super::projectile::Projectile) -> super::projectile::ProjectileDeflection {
        if mc_data::tags::get::<EntityType>("deflects_projectiles").is_some_and(|t| t.contains(self.get_type())) {
            super::projectile::ProjectileDeflection::Reverse
        } else {
            super::projectile::ProjectileDeflection::None
        }
    }

    /// `getPickRadius()`.
    fn get_pick_radius(&self) -> f32 {
        0.0
    }

    /// `canBePickedFromInside()`.
    fn can_be_picked_from_inside(&self) -> bool {
        true
    }

    // ---- identity / classification -------------------------------------------------------------------------

    /// `LivingEntity` data (`asLivingEntity()`), `None` for non-living entities.
    fn living(&self) -> Option<&super::living::LivingEntityData> {
        None
    }
    fn living_mut(&mut self) -> Option<&mut super::living::LivingEntityData> {
        None
    }
    /// `Player` data, `None` for non-players.
    fn player(&self) -> Option<&super::player::PlayerData> {
        None
    }
    fn player_mut(&mut self) -> Option<&mut super::player::PlayerData> {
        None
    }

    /// The `Container` an entity-backed menu reads (`ContainerKey::Entity(id)`): horse/nautilus inventory, chest
    /// minecart/boat, ... (vanilla `entity instanceof Container` / `getInventory()`).
    fn as_container(&self) -> Option<&dyn crate::world::container::Container> {
        None
    }
    fn as_container_mut(&mut self) -> Option<&mut dyn crate::world::container::Container> {
        None
    }

    /// `instanceof Player`.
    fn is_player(&self) -> bool {
        false
    }
    /// `isSpectator()`.
    fn is_spectator(&self) -> bool {
        false
    }
    /// `EntityAccess.isAlwaysTicking()`: players tick regardless of chunk status.
    fn is_always_ticking(&self) -> bool {
        false
    }
    /// `isAlive()` (`LivingEntity`: also needs health above zero).
    fn is_alive(&self) -> bool {
        !self.is_removed() && self.living().is_none_or(|l| l.get_health() > 0.0)
    }
    /// `Entity.shouldBeSaved()` without the passenger part (the section manager adds it).
    fn should_be_saved_ignoring_riding(&self) -> bool {
        self.base().removal_reason().is_none_or(|r| r.should_save())
    }

    // ---- movement hooks (used by `movement::move_entity`) ----------------------------------------------------

    /// `maxUpStep()` (`LivingEntity`: the `STEP_HEIGHT` attribute).
    fn max_up_step(&self) -> f32 {
        self.living().map_or(0.0, |l| l.get_attribute_value(mc_data::Attribute::STEP_HEIGHT) as f32)
    }
    /// `isDescending()` (= `isShiftKeyDown()`).
    fn is_descending(&self) -> bool {
        self.is_shift_key_down()
    }
    /// The main-hand item for `EntityCollisionContext.heldItem` (living entities only).
    fn held_item_for_collision(&self) -> Option<mc_data::Item> {
        None
    }
    /// `LivingEntity.canStandOnFluid(fluid)` for the collision context.
    fn can_stand_on_fluid_fn(&self) -> Option<crate::world::level::collision::CanStandOnFluid> {
        None
    }
    /// `isClientAuthoritative()`: the client simulates this entity's movement (players, entities they control).
    fn is_client_authoritative(&self) -> bool {
        false
    }
    /// `getMoveSimulationType()`.
    fn move_simulation_type(&self) -> MoveSimulationType {
        MoveSimulationType::AuthoritativeSide
    }
    fn move_simulation_type_is_server_and_client(&self) -> bool {
        self.move_simulation_type() == MoveSimulationType::ServerAndClient
    }
    /// `isLocalInstanceAuthoritative()` on the server.
    fn is_local_instance_authoritative(&self) -> bool {
        !self.is_client_authoritative()
    }
    /// `canSimulateMovement()` on the server.
    fn can_simulate_movement(&self) -> bool {
        match self.move_simulation_type() {
            MoveSimulationType::ServerAndClient | MoveSimulationType::AuthoritativeSideAndServer => true,
            MoveSimulationType::AuthoritativeSide => self.is_local_instance_authoritative(),
        }
    }
    /// `isHorizontalCollisionMinor(movement)`.
    fn is_horizontal_collision_minor(&self, _movement: Vec3) -> bool {
        false
    }
    /// `getMovementEmission()`.
    fn get_movement_emission(&self) -> MovementEmission {
        MovementEmission::All
    }
    /// `isStateClimbable(state)` (`BlockTags.CLIMBABLE` or powder snow).
    fn is_state_climbable(&self, state: mc_data::block::BlockState) -> bool {
        state.block().is_in_tag("climbable") || state.block() == mc_data::block::Block::POWDER_SNOW
    }
    /// `isSuppressingBounce()` (= `isShiftKeyDown()`).
    fn is_suppressing_bounce(&self) -> bool {
        self.is_shift_key_down()
    }
    /// `getEntityBounciness()`.
    fn get_entity_bounciness(&self) -> f64 {
        0.0
    }
    /// `getEffectiveGravity()` (`LivingEntity`: `SLOW_FALLING` caps the gravity of a falling entity at 0.01).
    fn get_effective_gravity(&self) -> f64 {
        if self.living().is_some() { super::living_entity::travel::living_effective_gravity(self.as_dyn()) } else { self.get_gravity() }
    }
    /// `getGravity()` (`getDefaultGravity()` unless `noGravity`; living entities read the `GRAVITY` attribute).
    fn get_gravity(&self) -> f64 {
        if self.base().is_no_gravity() { 0.0 } else { self.get_default_gravity() }
    }
    /// `getDefaultGravity()` (`LivingEntity`: the `GRAVITY` attribute).
    fn get_default_gravity(&self) -> f64 {
        self.living().map_or(0.0, |l| l.get_attribute_value(mc_data::Attribute::GRAVITY))
    }
    /// `getAirDrag()` (`LivingEntity`: modified by the `AIR_DRAG_MODIFIER` attribute).
    fn get_air_drag(&self) -> f32 {
        match self.living() {
            Some(l) => super::living_entity::travel::compute_modified_friction(
                if self.omnidirectional_air_mover() { 0.91 } else { 0.98 },
                l.get_attribute_value(mc_data::Attribute::AIR_DRAG_MODIFIER) as f32,
            ),
            None => 0.98,
        }
    }
    /// Players do not walk off edges while sneaking (`Player.maybeBackOffFromEdge`: `!flying && isStayingOnGroundSurface`).
    fn stays_on_ground_surface_at_edges(&self) -> bool {
        false
    }

    /// `move(moverType, delta)`.
    fn r#move(&mut self, level: &mut ServerLevel, mover: MoverType, delta: Vec3) {
        super::movement::move_entity(self.as_dyn_mut(), level, mover, delta);
    }

    /// `setOnGroundWithMovement(onGround, horizontalCollision, movement)`.
    fn set_on_ground_with_movement(&mut self, level: &ServerLevel, on_ground: bool, horizontal_collision: bool, movement: Vec3) {
        super::movement::set_on_ground_with_movement(self.as_dyn_mut(), level, on_ground, horizontal_collision, movement);
    }

    /// `doCheckFallDamage(xa, ya, za, onGround)`.
    fn do_check_fall_damage(&mut self, level: &mut ServerLevel, xa: f64, ya: f64, za: f64, on_ground: bool) {
        use crate::world::level::BlockGetter;
        if !super::movement::touching_unloaded_chunk(self.as_dyn(), &*level) {
            super::movement::check_supporting_block(self.as_dyn_mut(), &*level, on_ground, Some(Vec3::new(xa, ya, za)));
            let pos = super::movement::get_on_pos_legacy(self.as_dyn(), &*level);
            let state = level.get_block_state(pos);
            self.check_fall_damage(level, ya, on_ground, state, pos);
        }
    }

    /// `resetFallDistance()`.
    fn reset_fall_distance(&mut self) {
        self.base_mut().fall_distance = 0.0;
    }

    /// `teleportSetPosition(destination, relatives)`.
    fn teleport_set_position(&mut self, destination: mc_protocol::packets::play::PositionMoveRotation, relatives: mc_protocol::packets::play::Relatives) {
        let current = position_move_rotation_of(self.as_dyn());
        self.teleport_set_position_from(current, destination, relatives);
    }

    /// `teleportSetPosition(current, destination, relatives)`.
    fn teleport_set_position_from(
        &mut self,
        current: mc_protocol::packets::play::PositionMoveRotation,
        destination: mc_protocol::packets::play::PositionMoveRotation,
        relatives: mc_protocol::packets::play::Relatives,
    ) {
        let abs = calculate_absolute(current, destination, relatives);
        let b = self.base_mut();
        b.set_pos_raw(abs.position.x, abs.position.y, abs.position.z);
        b.set_y_rot(abs.y_rot);
        self.set_y_head_rot(abs.y_rot);
        self.base_mut().set_x_rot(abs.x_rot);
        self.reapply_position();
        let b = self.base_mut();
        b.set_old_pos_and_rot();
        b.set_delta_movement(abs.delta_movement);
        b.movement_this_tick.clear();
    }

    // ---- tracking ------------------------------------------------------------------------------------------

    /// `trackingPosition()`.
    fn tracking_position(&self) -> Vec3 {
        self.position()
    }

    /// Which `ServerEntity` special cases apply (`AbstractArrow`, `ItemEntity`).
    fn tracking_kind(&self) -> super::TrackingKind {
        super::TrackingKind::Normal
    }

    /// `AbstractHurtingProjectile.accelerationPower` (sent in a bundle with the motion packet).
    fn hurting_projectile_power(&self) -> Option<f64> {
        None
    }

    /// `getAddEntityPacket(serverEntity)`.
    fn get_add_entity_packet(&self, server_entity: &crate::server::level::ServerEntity) -> mc_protocol::packets::play::ClientboundAddEntityPacket {
        server_entity.add_entity_packet(self.id(), self.uuid(), self.get_type(), 0)
    }

    /// `getItemBySlot(slot)` (empty for non-living entities).
    fn get_item_by_slot(&self, slot: super::equipment::EquipmentSlot) -> crate::world::item::ItemStack {
        if let Some(p) = self.player() {
            return p.inventory.equipment_get(slot).clone();
        }
        self.living().map(|l| l.equipment.get(slot).clone()).unwrap_or_default()
    }

    /// The `ClientboundSetEquipmentPacket` of a pairing (`None` when nothing is equipped / not living).
    fn equipment_packet(&self) -> Option<mc_protocol::packets::play::ClientboundSetEquipmentPacket> {
        self.living()?;
        let slots: Vec<_> = super::equipment::EquipmentSlot::VALUES
            .into_iter()
            .filter_map(|slot| {
                let stack = self.get_item_by_slot(slot);
                (!stack.is_empty()).then(|| (protocol_slot(slot), stack))
            })
            .collect();
        (!slots.is_empty()).then(|| mc_protocol::packets::play::ClientboundSetEquipmentPacket { entity: self.id(), slots })
    }

    /// `playerTouch(player)` (pickup). The entity is taken out of the level while it runs.
    fn player_touch(&mut self, _level: &mut ServerLevel, _player: &mut super::ServerPlayer) {}

    /// Offset of `getBlockPosBelowThatAffectsMyMovement()` (0.500001; items use 0.999999).
    fn block_pos_below_offset(&self) -> f32 {
        0.500001
    }

    /// `broadcastToPlayer(player)`.
    fn broadcast_to_player(&self, _player_id: EntityId) -> bool {
        true
    }

    /// `instanceof EnderDragonPart` (never tracked individually).
    fn is_ender_dragon_part(&self) -> bool {
        false
    }

    /// `Leashable.getLeashHolder()` id when the entity is leashed.
    fn leash_holder(&self) -> Option<EntityId> {
        super::mob::leashable::get_leash_holder(self.as_dyn())
    }

    /// `startSeenByPlayer(player)`.
    fn start_seen_by_player(&mut self, _player_id: EntityId) {}

    /// `stopSeenByPlayer(player)`.
    fn stop_seen_by_player(&mut self, _player_id: EntityId) {}

    // ---- synched data ----------------------------------------------------------------------------------------

    /// `onSyncedDataUpdated(accessor)`.
    fn on_synced_data_updated(&mut self, accessor_id: u8) {
        if accessor_id == DATA_POSE.id() {
            self.refresh_dimensions();
        }
    }

    // ---- dimensions ----------------------------------------------------------------------------------------

    /// `getDimensions(pose)`.
    fn get_dimensions(&self, _pose: Pose) -> EntityDimensions {
        EntityDimensions::of_type(self.base().entity_type)
    }

    /// `makeBoundingBox(position)`.
    fn make_bounding_box(&self, position: Vec3) -> AABB {
        self.base().dimensions.make_bounding_box_vec(position)
    }

    /// `setPos(x, y, z)`.
    fn set_pos(&mut self, x: f64, y: f64, z: f64) {
        self.base_mut().set_pos_raw(x, y, z);
        let bb = self.make_bounding_box(self.base().position());
        self.base_mut().set_bounding_box(bb);
    }

    fn set_pos_vec(&mut self, pos: Vec3) {
        self.set_pos(pos.x, pos.y, pos.z);
    }

    /// `reapplyPosition()`.
    fn reapply_position(&mut self) {
        self.base_mut().last_known_position = None;
        let p = self.base().position();
        self.set_pos(p.x, p.y, p.z);
    }

    /// `refreshDimensions()` minus the level-dependent `fudgePositionAfterSizeChange` (recorded in
    /// `pending_fudge_from` and run by `base_tick`).
    fn refresh_dimensions(&mut self) {
        let old_dim = self.base().dimensions.clone();
        let pose = self.base().get_pose();
        let new_dim = self.get_dimensions(pose);
        self.base_mut().dimensions = new_dim.clone();
        self.base_mut().eye_height = new_dim.eye_height;
        self.reapply_position();
        let is_small = new_dim.width <= 4.0 && new_dim.height <= 4.0;
        let b = self.base();
        if !b.first_tick
            && !b.no_physics
            && is_small
            && (new_dim.width > old_dim.width || new_dim.height > old_dim.height)
            && !self.is_player()
        {
            self.base_mut().pending_fudge_from = Some(old_dim);
        }
    }

    /// `fixupDimensions()`.
    fn fixup_dimensions(&mut self) {
        let pose = self.base().get_pose();
        let new_dim = self.get_dimensions(pose);
        self.base_mut().eye_height = new_dim.eye_height;
        self.base_mut().dimensions = new_dim;
    }

    fn get_attachments(&self) -> &EntityAttachments {
        &self.base().dimensions.attachments
    }

    // ---- rotation ------------------------------------------------------------------------------------------

    /// `setRot(yRot, xRot)` (protected in vanilla).
    fn set_rot(&mut self, y_rot: f32, x_rot: f32) {
        self.base_mut().set_rot(y_rot, x_rot);
    }

    fn set_y_rot(&mut self, y_rot: f32) {
        self.base_mut().set_y_rot(y_rot);
    }
    fn set_x_rot(&mut self, x_rot: f32) {
        self.base_mut().set_x_rot(x_rot);
    }
    /// `getYHeadRot()` (`LivingEntity.yHeadRot`).
    fn get_y_head_rot(&self) -> f32 {
        self.living().map_or(0.0, |l| l.y_head_rot)
    }
    /// `setYHeadRot(yHeadRot)`.
    fn set_y_head_rot(&mut self, y_head_rot: f32) {
        if let Some(l) = self.living_mut() {
            l.y_head_rot = y_head_rot;
        }
    }
    /// `setYBodyRot(yBodyRot)`.
    fn set_y_body_rot(&mut self, y_body_rot: f32) {
        if let Some(l) = self.living_mut() {
            l.y_body_rot = y_body_rot;
        }
    }
    /// `lerpHeadTo(yRot, steps)`.
    fn lerp_head_to(&mut self, y_rot: f32, steps: i32) {
        if let Some(l) = self.living_mut() {
            l.lerp_y_head_rot = f64::from(y_rot);
            l.lerp_head_steps = steps;
        }
    }

    // ---- snapping ----------------------------------------------------------------------------------------------

    /// `absSnapTo(x, y, z, yRot, xRot)`.
    fn abs_snap_to(&mut self, x: f64, y: f64, z: f64, y_rot: f32, x_rot: f32) {
        self.abs_snap_to_pos(x, y, z);
        self.abs_snap_rotation_to(y_rot, x_rot);
    }

    fn abs_snap_rotation_to(&mut self, y_rot: f32, x_rot: f32) {
        let b = self.base_mut();
        b.set_y_rot(y_rot % 360.0);
        b.set_x_rot(mth::clamp_f32(x_rot, -90.0, 90.0) % 360.0);
        b.y_rot_o = b.y_rot();
        b.x_rot_o = b.x_rot();
    }

    fn abs_snap_to_pos(&mut self, x: f64, y: f64, z: f64) {
        let cx = mth::clamp_f64(x, -3.0e7, 3.0e7);
        let cz = mth::clamp_f64(z, -3.0e7, 3.0e7);
        let b = self.base_mut();
        b.xo = cx;
        b.yo = y;
        b.zo = cz;
        self.set_pos(cx, y, cz);
    }

    /// `snapTo(x, y, z, yRot, xRot)`.
    fn snap_to(&mut self, x: f64, y: f64, z: f64, y_rot: f32, x_rot: f32) {
        let b = self.base_mut();
        b.set_pos_raw(x, y, z);
        b.set_y_rot(y_rot);
        b.set_x_rot(x_rot);
        b.set_old_pos_and_rot();
        self.reapply_position();
    }

    fn snap_to_pos(&mut self, x: f64, y: f64, z: f64) {
        let (yr, xr) = (self.base().y_rot(), self.base().x_rot());
        self.snap_to(x, y, z, yr, xr);
    }

    // ---- ticking ---------------------------------------------------------------------------------------------

    /// `tick()` (`LivingEntity.tick()` for living entities).
    fn tick(&mut self, level: &mut ServerLevel) {
        if self.mob().is_some() {
            super::mob::tick::tick(self.as_dyn_mut(), level);
        } else if self.living().is_some() {
            super::living_entity::tick::tick(self.as_dyn_mut(), level);
        } else {
            self.base_tick(level);
        }
    }

    /// `baseTick()` (`LivingEntity.baseTick()` for living entities).
    fn base_tick(&mut self, level: &mut ServerLevel) {
        if self.mob().is_some() {
            super::mob::tick::base_tick(self.as_dyn_mut(), level);
        } else if self.living().is_some() {
            super::living_entity::base_tick::base_tick(self.as_dyn_mut(), level);
        } else {
            base_tick(self, level);
        }
    }

    /// `updateDataBeforeSync()`.
    fn update_data_before_sync(&mut self) {
        // `LivingEntity.updateDataBeforeSync`: `updateDirtyEffects()`
        if self.living().is_some() {
            super::living_entity::effects::update_dirty_effects(self.as_dyn_mut());
        }
    }

    /// `checkDespawn()`.
    fn check_despawn(&mut self, level: &mut ServerLevel) {
        if self.mob().is_some() {
            super::mob::spawn::check_despawn(self.as_dyn_mut(), level);
        }
    }
    /// `ServerPlayer.stopSleepInBed(forceWakeUp, updateLevelList)` (only players sleep; `ServerPlayer` overrides).
    fn stop_sleep_in_bed(&mut self, _level: &mut ServerLevel, _force_wake_up: bool, _update_level_list: bool) {}
    /// The entity's `DynamicGameEventListener` target, if it listens to game events (vibration listeners).
    fn game_event_listener(&mut self) -> Option<&mut dyn crate::world::level::gameevent::GameEventListener> {
        None
    }

    // ---- fire / air / frozen ----------------------------------------------------------------------------------

    fn fire_immune(&self) -> bool {
        self.base().entity_type.data().fire_immune
    }

    fn get_max_air_supply(&self) -> i32 {
        TOTAL_AIR_SUPPLY
    }

    /// `isOnFire()` (server side: `!fireImmune() && remainingFireTicks > 0`).
    fn is_on_fire(&self) -> bool {
        !self.fire_immune() && self.base().remaining_fire_ticks > 0
    }

    fn set_shared_flag_on_fire(&mut self, value: bool) {
        let visual = self.base().has_visual_fire;
        self.base_mut().set_shared_flag(FLAG_ONFIRE, value || visual);
    }

    fn set_remaining_fire_ticks(&mut self, remaining_ticks: i32) {
        self.base_mut().remaining_fire_ticks = remaining_ticks;
    }

    fn get_remaining_fire_ticks(&self) -> i32 {
        self.base().remaining_fire_ticks
    }

    fn clear_fire(&mut self) {
        let r = self.get_remaining_fire_ticks().min(0);
        self.set_remaining_fire_ticks(r);
    }

    fn ignite_for_ticks(&mut self, number_of_ticks: i32) {
        if self.get_remaining_fire_ticks() < number_of_ticks {
            self.set_remaining_fire_ticks(number_of_ticks);
        }
        self.clear_freeze();
    }

    fn ignite_for_seconds(&mut self, number_of_seconds: f32) {
        self.ignite_for_ticks(mth::floor_f32(number_of_seconds * 20.0));
    }

    fn clear_freeze(&mut self) {
        self.base_mut().set_ticks_frozen(0);
    }

    fn get_ticks_required_to_freeze(&self) -> i32 {
        BASE_TICKS_REQUIRED_TO_FREEZE
    }

    fn is_freezing(&self) -> bool {
        self.base().get_ticks_frozen() > 0
    }

    fn can_freeze(&self) -> bool {
        true
    }

    // ---- removal -------------------------------------------------------------------------------------------

    /// `remove(reason)` / `setRemoved(reason)` for an entity the caller holds `&mut`. Riding links are cut by
    /// the caller through `riding::stop_riding_all` (needs the level); the section manager is notified on sync.
    fn remove(&mut self, reason: RemovalReason) {
        self.base_mut().set_removal_reason(reason);
        self.on_removal(reason);
    }

    /// `onRemoval(reason)`.
    fn on_removal(&mut self, _reason: RemovalReason) {}

    fn kill(&mut self) {
        self.remove(RemovalReason::Killed);
    }

    fn discard(&mut self) {
        self.remove(RemovalReason::Discarded);
    }

    // ---- name -----------------------------------------------------------------------------------------------

    fn should_show_name(&self) -> bool {
        self.base().is_custom_name_visible()
    }

    fn get_scoreboard_name(&self) -> String {
        self.base().uuid.hyphenated().to_string()
    }

    // ---- riding ---------------------------------------------------------------------------------------------

    fn can_add_passenger(&self, _passenger: &dyn Entity) -> bool {
        self.base().passengers.is_empty()
    }
    fn could_accept_passenger(&self) -> bool {
        true
    }
    fn can_ride(&self, _vehicle: &dyn Entity) -> bool {
        !self.is_shift_key_down() && self.base().boarding_cooldown <= 0
    }
    fn is_shift_key_down(&self) -> bool {
        self.base().get_shared_flag(FLAG_SHIFT_KEY_DOWN)
    }
    /// `getControllingPassenger()` (the living passenger id that drives this entity).
    fn get_controlling_passenger(&self) -> Option<EntityId> {
        None
    }
    /// `isPushable()` (`LivingEntity`: alive, not spectating, not climbing, and in an entity-ticking chunk unless it
    /// always ticks).
    fn is_pushable(&self, level: &ServerLevel) -> bool {
        self.living().is_some() && super::living_entity::push::living_is_pushable(self.as_dyn(), level)
    }
    /// `isPickable()` (`LivingEntity`: not removed).
    fn is_pickable(&self) -> bool {
        self.living().is_some() && !self.is_removed()
    }
    fn can_be_collided_with(&self, _other: Option<&dyn Entity>) -> bool {
        false
    }
    fn is_attackable(&self) -> bool {
        true
    }
    fn is_invisible(&self) -> bool {
        self.base().get_shared_flag(FLAG_INVISIBLE)
    }

    // ---- save / load ------------------------------------------------------------------------------------------

    /// `setPose(pose)`.
    fn set_pose(&mut self, pose: mc_data::enums::Pose) {
        if self.base_mut().entity_data.set(DATA_POSE, pose) {
            self.on_synced_data_updated(DATA_POSE.id());
        }
    }

    /// `getEncodeId()`: the registry name, `None` if the type isn't serializable.
    fn get_encode_id(&self) -> Option<&'static str> {
        let ty = self.get_type();
        ty.data().serializable.then(|| ty.data().name)
    }

    /// `addAdditionalSaveData(output)` (abstract in vanilla; subclasses add their tags).
    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        if self.mob().is_some() {
            super::mob::nbt::add_additional_save_data(self.as_dyn(), out);
        }
    }

    /// `readAdditionalSaveData(input)`.
    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        if self.mob().is_some() {
            super::mob::nbt::read_additional_save_data(self.as_dyn_mut(), input);
        }
    }

    /// `repositionEntityAfterLoad()`.
    fn reposition_entity_after_load(&self) -> bool {
        true
    }

    /// `postDataManipulated()`.
    fn post_data_manipulated(&mut self) {}

    /// `Entity.load(input)`: errors for a non-finite position/rotation (vanilla throws `IllegalStateException`
    /// wrapped in a crash report).
    fn load(&mut self, input: &CompoundTag) -> Result<(), String> {
        load(self, input)
    }

    /// `saveWithoutId(output)`; `lookup` resolves the vehicle (for `Pos`) and the passengers.
    fn save_without_id(&self, out: &mut CompoundTag, lookup: &EntityLookupFn<'_>) {
        save_without_id(self, out, lookup);
    }
    // ==== [EntityBase] additions (Entity trait methods) ====

    // ---- fluids / water state ---------------------------------------------------------------------------------

    /// `isInWater()`.
    fn is_in_water(&self) -> bool {
        self.base().was_touching_water
    }
    /// `isInLava()`.
    fn is_in_lava(&self) -> bool {
        !self.base().first_tick && self.base().fluid_interaction.is_in_fluid(super::fluid_interaction::TAG_LAVA)
    }
    /// `isInLiquid()`.
    fn is_in_liquid(&self) -> bool {
        self.is_in_water() || self.is_in_lava()
    }
    /// `isInFloatableFluid()`.
    fn is_in_floatable_fluid(&self) -> bool {
        self.base().fluid_interaction.is_in_fluid(super::fluid_interaction::TAG_ENTITY_FLOATABLE)
    }
    /// `isUnderWater()`.
    fn is_under_water(&self) -> bool {
        self.base().was_eye_in_water && self.is_in_water()
    }
    /// `isInShallowWater()`.
    fn is_in_shallow_water(&self) -> bool {
        self.is_in_water() && !self.is_under_water()
    }
    /// `isInRain()`.
    fn is_in_rain(&self, level: &ServerLevel) -> bool {
        let pos = self.base().block_position();
        level.is_raining_at(pos) || level.is_raining_at(BlockPos::new(pos.x(), mth::floor_f64(self.base().bounding_box().max_y), pos.z()))
    }
    /// `isInWaterOrRain()`.
    fn is_in_water_or_rain(&self, level: &ServerLevel) -> bool {
        self.is_in_water() || self.is_in_rain(level)
    }
    /// `isEyeInFluid(tag)` (`"water"`, `"lava"`, ...).
    fn is_eye_in_fluid(&self, tag: &str) -> bool {
        self.base().fluid_interaction.is_eye_in_fluid(tag)
    }
    /// `getFluidHeight(tag)`.
    fn get_fluid_height(&self, tag: &str) -> f64 {
        self.base().fluid_interaction.get_fluid_height(tag)
    }
    /// `isPushedByFluid()`.
    fn is_pushed_by_fluid(&self) -> bool {
        true
    }
    /// `modifyPassengerFluidInteractionBox(passengerBox)` (boats return `null` for the box of a passenger).
    fn modify_passenger_fluid_interaction_box(&self, passenger_box: AABB) -> Option<AABB> {
        Some(passenger_box)
    }
    /// `getFluidInteractionBox()`.
    fn get_fluid_interaction_box(&self, level: &ServerLevel) -> Option<AABB> {
        let mut bx = self.base().bounding_box().deflate_scalar(0.001);
        if let Some(vehicle) = self.base().vehicle.and_then(|v| level.entity_manager.get_any(v)) {
            bx = vehicle.modify_passenger_fluid_interaction_box(bx)?;
        }
        Some(bx)
    }
    /// `updateFluidInteraction()`: returns whether the entity touches any fluid.
    fn update_fluid_interaction(&mut self, level: &mut ServerLevel) -> bool {
        let ignore_current = !self.is_pushed_by_fluid();
        let mut interaction = std::mem::take(&mut self.base_mut().fluid_interaction);
        let in_any_fluid = interaction.update(self.as_dyn(), level, ignore_current);
        let [water_height, lava_height] = interaction.water_lava_heights();
        let eyes = interaction.water_lava_eyes();
        let in_water = water_height > 0.0;
        let in_lava = lava_height > 0.0;
        let is_player = self.is_player();
        let old_movement = self.base().delta_movement();
        let water_impulse = if self.is_pushed_by_fluid() && in_water { interaction.current_impulse(true, is_player, old_movement, 0.014) } else { None };
        {
            let b = self.base_mut();
            b.fluid_interaction = interaction;
            b.fluid_height = [water_height, lava_height];
            b.eye_in_fluid = eyes;
        }
        if in_water {
            self.reset_fall_distance();
            if !self.base().was_touching_water && !self.base().first_tick {
                self.do_water_splash_effect(level);
            }
        }
        self.base_mut().was_touching_water = in_water;
        if self.is_pushed_by_fluid() {
            if let Some(impulse) = water_impulse {
                self.base_mut().add_delta_movement(impulse);
            }
            if in_lava {
                let scale = if level.fast_lava() { 0.007 } else { 0.0023333333333333335 };
                let old_movement = self.base().delta_movement();
                if let Some(impulse) = self.base().fluid_interaction.current_impulse(false, is_player, old_movement, scale) {
                    self.base_mut().add_delta_movement(impulse);
                }
            }
        }
        in_any_fluid
    }
    /// `doWaterSplashEffect()`.
    fn do_water_splash_effect(&mut self, level: &mut ServerLevel) {
        let controller = self.get_controlling_passenger().and_then(|id| level.entity_manager.get_any(id).map(|e| e.base().delta_movement()));
        let (volume_modifier, movement) = match controller {
            Some(m) => (0.9f32, m),
            None => (0.2f32, self.base().delta_movement()),
        };
        let speed = 1.0f32.min((movement.x * movement.x * f64::from(0.2f32) + movement.y * movement.y + movement.z * movement.z * f64::from(0.2f32)).sqrt() as f32 * volume_modifier);
        let pitch_a = self.base_mut().random.next_float();
        let pitch_b = self.base_mut().random.next_float();
        let pitch = 1.0 + (pitch_a - pitch_b) * 0.4;
        if speed < 0.25 {
            let sound = self.get_swim_splash_sound();
            self.play_sound(level, sound, speed, pitch);
        } else {
            let sound = self.get_swim_high_speed_splash_sound();
            self.play_sound(level, sound, speed, pitch);
        }
        // the bubble / splash particles are client-side (`Level.addParticle` is a no-op on the server) but their
        // random draws advance the entity's random like vanilla
        let width = self.base().dimensions.width;
        let count = |i: i32| (i as f32) < 1.0 + width * 20.0;
        let mut i = 0;
        while count(i) {
            self.base_mut().random.next_double();
            self.base_mut().random.next_double();
            self.base_mut().random.next_double();
            i += 1;
        }
        let mut i = 0;
        while count(i) {
            self.base_mut().random.next_double();
            self.base_mut().random.next_double();
            i += 1;
        }
        self.game_event(level, mc_data::GameEvent::SPLASH);
    }
    /// `updateSwimming()`.
    fn update_swimming(&mut self, level: &ServerLevel) {
        if self.is_swimming() {
            let swimming = self.is_sprinting() && self.is_in_water() && !self.base().is_passenger();
            self.set_swimming(swimming);
        } else {
            let swimming = self.is_sprinting()
                && self.is_under_water()
                && !self.base().is_passenger()
                && crate::world::level::material::FluidStateExt::is_tag(crate::world::level::BlockGetter::get_fluid_state(level, self.base().block_position()), "water");
            self.set_swimming(swimming);
        }
    }
    fn is_sprinting(&self) -> bool {
        self.base().get_shared_flag(FLAG_SPRINTING)
    }
    fn is_swimming(&self) -> bool {
        self.base().get_shared_flag(FLAG_SWIMMING)
    }
    fn set_swimming(&mut self, swimming: bool) {
        self.base_mut().set_shared_flag(FLAG_SWIMMING, swimming);
    }
    /// `isVisuallySwimming()`.
    fn is_visually_swimming(&self) -> bool {
        self.base().get_pose() == Pose::Swimming
    }
    /// `isVisuallyCrawling()`.
    fn is_visually_crawling(&self) -> bool {
        self.is_visually_swimming() && !self.is_in_water()
    }
    /// `isCrouching()`.
    fn is_crouching(&self) -> bool {
        self.base().get_pose() == Pose::Crouching
    }
    /// `isOnRails()`.
    fn is_on_rails(&self) -> bool {
        false
    }
    /// `isIgnoringBlockTriggers()`.
    fn is_ignoring_block_triggers(&self) -> bool {
        false
    }
    /// `dampensVibrations()`.
    fn dampens_vibrations(&self) -> bool {
        false
    }

    // ---- tick parts ------------------------------------------------------------------------------------------------

    /// `canSpawnSprintParticle()`.
    fn can_spawn_sprint_particle(&self) -> bool {
        self.is_sprinting() && !self.is_in_water() && !self.is_spectator() && !self.is_crouching() && !self.is_in_lava() && self.is_alive()
    }
    /// `spawnSprintParticle()`: the particle itself is client-side; the draws of the entity's random are kept.
    fn spawn_sprint_particle(&mut self, level: &ServerLevel) {
        use crate::world::level::BlockGetter;
        let pos = super::movement::get_on_pos_legacy(self.as_dyn(), level);
        let block_state = level.get_block_state(pos);
        if !super::movement::is_render_shape_invisible(block_state) {
            // x, z: `getX() + (random.nextDouble() - 0.5) * width` (clamped, unobservable on the server)
            self.base_mut().random.next_double();
            self.base_mut().random.next_double();
        }
    }
    /// `checkBelowWorld()`.
    fn check_below_world(&mut self, level: &mut ServerLevel) {
        if self.base().y() < f64::from(crate::world::level::BlockGetter::get_min_y(&*level) - 64) {
            self.on_below_world(level);
        }
    }
    /// `lavaIgnite()`.
    fn lava_ignite(&mut self) {
        if !self.fire_immune() {
            self.ignite_for_seconds(15.0);
        }
    }
    /// `lavaHurt()`.
    fn lava_hurt(&mut self, level: &mut ServerLevel) {
        if !self.fire_immune() {
            let source = level.damage_sources().lava();
            if self.hurt_server(level, &source, 4.0) && self.should_play_lava_hurt_sound() && !self.base().is_silent() {
                let pitch = 2.0 + self.base_mut().random.next_float() * 0.4;
                let p = self.base().position();
                let sound_source = self.get_sound_source();
                level.play_sound_at(None, p.x, p.y, p.z, mc_data::SoundEvent::ENTITY_GENERIC_BURN, sound_source, 0.4, pitch);
            }
        }
    }
    /// `shouldPlayLavaHurtSound()`.
    fn should_play_lava_hurt_sound(&self) -> bool {
        true
    }
    /// `getFireImmuneTicks()`.
    fn get_fire_immune_ticks(&self) -> i32 {
        0
    }
    /// `playEntityOnFireExtinguishedSound()`.
    fn play_entity_on_fire_extinguished_sound(&mut self, level: &mut ServerLevel) {
        let a = self.base_mut().random.next_float();
        let b = self.base_mut().random.next_float();
        let p = self.base().position();
        let sound_source = self.get_sound_source();
        level.play_sound_at(None, p.x, p.y, p.z, mc_data::SoundEvent::ENTITY_GENERIC_EXTINGUISH_FIRE, sound_source, 0.7, 1.6 + (a - b) * 0.4);
    }
    /// `extinguishFire()`.
    fn extinguish_fire(&mut self, level: &mut ServerLevel) {
        if self.is_on_fire() {
            self.play_entity_on_fire_extinguished_sound(level);
        }
        self.clear_fire();
    }
    /// `thunderHit(level, lightningBolt)`.
    fn thunder_hit(&mut self, level: &mut ServerLevel) {
        let t = self.get_remaining_fire_ticks().wrapping_add(1);
        self.set_remaining_fire_ticks(t);
        if self.get_remaining_fire_ticks() == 0 {
            self.ignite_for_seconds(8.0);
        }
        let source = level.damage_sources().lightning_bolt();
        self.hurt_server(level, &source, 5.0);
    }
    /// `isAffectedByBlocks()`.
    fn is_affected_by_blocks(&self) -> bool {
        !self.is_removed() && !self.base().no_physics
    }
    /// `onInsideBlock(state)`.
    fn on_inside_block(&mut self, _state: mc_data::block::BlockState) {}
    /// `setIsInPowderSnow(isInPowderSnow)`.
    fn set_is_in_powder_snow(&mut self, is_in_powder_snow: bool) {
        self.base_mut().is_in_powder_snow = is_in_powder_snow;
    }
    /// `Leashable.tickLeash(level, this)` for leashable entities.
    fn tick_leash(&mut self, level: &mut ServerLevel) {
        if self.mob().is_some() {
            super::mob::leashable::tick_leash(self.as_dyn_mut(), level);
        }
    }
    /// `applyEffectsFromBlocks()`.
    fn apply_effects_from_blocks(&mut self, level: &mut ServerLevel) {
        super::inside_block_effects::apply_effects_from_blocks(self.as_dyn_mut(), level);
    }
    /// `applyEffectsFromBlocksForLastMovements()`.
    fn apply_effects_from_blocks_for_last_movements(&mut self, level: &mut ServerLevel) {
        super::inside_block_effects::apply_effects_from_blocks_for_last_movements(self.as_dyn_mut(), level);
    }
    /// `applyEffectsFromBlocks(from, to)`.
    fn apply_effects_from_blocks_between(&mut self, level: &mut ServerLevel, from: Vec3, to: Vec3) {
        super::inside_block_effects::apply_effects_from_blocks_between(self.as_dyn_mut(), level, from, to);
    }
    /// `getInBlockState()` (cached until the block position changes / the next tick).
    fn get_in_block_state(&mut self, level: &ServerLevel) -> mc_data::block::BlockState {
        use crate::world::level::BlockGetter;
        if let Some(s) = self.base().in_block_state {
            return s;
        }
        let s = level.get_block_state(self.base().block_position());
        self.base_mut().in_block_state = Some(s);
        s
    }
    /// `isFree(xa, ya, za)`.
    fn is_free(&self, level: &ServerLevel, xa: f64, ya: f64, za: f64) -> bool {
        use crate::world::level::collision::{BlockGetterExt, CollisionGetter};
        let bx = self.base().bounding_box().move_(xa, ya, za);
        level.no_collision(Some(self.id()), Some(&super::movement::collision_context(self.as_dyn())), &bx) && !super::movement::contains_any_liquid(level, &bx)
    }
    /// `isInWall()`.
    fn is_in_wall(&self, level: &ServerLevel) -> bool {
        super::movement::is_in_wall(self.as_dyn(), level)
    }
    /// `Entity.getSoundSource()`.
    fn get_sound_source(&self) -> mc_protocol::packets::play::SoundSource {
        mc_protocol::packets::play::SoundSource::Neutral
    }
    /// `playSound(sound, volume, pitch)` (silent entities make no sound).
    fn play_sound(&self, level: &mut ServerLevel, sound: mc_data::SoundEvent, volume: f32, pitch: f32) {
        if !self.base().is_silent() {
            let p = self.base().position();
            level.play_sound_at(None, p.x, p.y, p.z, sound, self.get_sound_source(), volume, pitch);
        }
    }
    /// `playSound(sound)`.
    fn play_sound_default(&self, level: &mut ServerLevel, sound: mc_data::SoundEvent) {
        if !self.base().is_silent() {
            self.play_sound(level, sound, 1.0, 1.0);
        }
    }
    /// `gameEvent(event)` (source = this entity).
    fn game_event(&self, level: &mut ServerLevel, event: mc_data::GameEvent) {
        level.game_event_from_entity_ref(self.as_dyn(), event, self.base().position());
    }
    /// `getSwimSound()`.
    fn get_swim_sound(&self) -> mc_data::SoundEvent {
        mc_data::SoundEvent::ENTITY_GENERIC_SWIM
    }
    /// `getSwimSplashSound()`.
    fn get_swim_splash_sound(&self) -> mc_data::SoundEvent {
        mc_data::SoundEvent::ENTITY_GENERIC_SPLASH
    }
    /// `getSwimHighSpeedSplashSound()`.
    fn get_swim_high_speed_splash_sound(&self) -> mc_data::SoundEvent {
        mc_data::SoundEvent::ENTITY_GENERIC_SPLASH
    }
    /// `nextStep()`.
    fn next_step(&self) -> f32 {
        (self.base().move_dist as i32 + 1) as f32
    }
    /// `isFlapping()`.
    fn is_flapping(&self) -> bool {
        false
    }
    /// `onFlap()`.
    fn on_flap(&mut self) {}
    /// `processFlappingMovement()`.
    fn process_flapping_movement(&mut self, level: &mut ServerLevel) {
        if self.is_flapping() {
            self.on_flap();
            if self.get_movement_emission().emits_events() {
                self.game_event(level, mc_data::GameEvent::FLAP);
            }
        }
    }
    /// `waterSwimSound()`.
    fn water_swim_sound(&mut self, level: &mut ServerLevel) {
        let controller = self.get_controlling_passenger().and_then(|id| level.entity_manager.get_any(id).map(|e| e.base().delta_movement()));
        let (volume_modifier, delta_movement) = match controller {
            Some(m) => (0.4f32, m),
            None => (0.35f32, self.base().delta_movement()),
        };
        let speed = 1.0f32.min((delta_movement.x * delta_movement.x * f64::from(0.2f32) + delta_movement.y * delta_movement.y + delta_movement.z * delta_movement.z * f64::from(0.2f32)).sqrt() as f32 * volume_modifier);
        self.play_swim_sound(level, speed);
    }
    /// `playSwimSound(volume)`.
    fn play_swim_sound(&mut self, level: &mut ServerLevel, volume: f32) {
        let a = self.base_mut().random.next_float();
        let b = self.base_mut().random.next_float();
        let sound = self.get_swim_sound();
        self.play_sound(level, sound, volume, 1.0 + (a - b) * 0.4);
    }
    /// `playStepSound(pos, blockState)`.
    fn play_step_sound(&mut self, level: &mut ServerLevel, _pos: BlockPos, block_state: mc_data::block::BlockState) {
        let sound_type = block_state.get_sound_type();
        self.play_sound(level, sound_type.step_sound, sound_type.volume * 0.15, sound_type.pitch);
    }
    /// `getPrimaryStepSoundBlockPos(affectingPos)`.
    fn get_primary_step_sound_block_pos(&self, level: &ServerLevel, affecting_pos: BlockPos) -> BlockPos {
        use crate::world::level::BlockGetter;
        let above_pos = affecting_pos.above();
        let above_state = level.get_block_state(above_pos);
        if !above_state.block().is_in_tag("inside_step_sound_blocks") && !above_state.block().is_in_tag("combination_step_sound_blocks") { affecting_pos } else { above_pos }
    }
    /// `playCombinationStepSounds(primary, secondary)`.
    fn play_combination_step_sounds(&mut self, level: &mut ServerLevel, primary_step_sound: mc_data::block::BlockState, secondary_step_sound: mc_data::block::BlockState) {
        let sound_type = primary_step_sound.get_sound_type();
        self.play_sound(level, sound_type.step_sound, sound_type.volume * 0.15, sound_type.pitch);
        self.play_muffled_step_sound(level, secondary_step_sound);
    }
    /// `playMuffledStepSound(blockState)`.
    fn play_muffled_step_sound(&mut self, level: &mut ServerLevel, block_state: mc_data::block::BlockState) {
        let sound_type = block_state.get_sound_type();
        self.play_sound(level, sound_type.step_sound, sound_type.volume * 0.05, sound_type.pitch * 0.8);
    }
    /// `Entity.checkFallDamage(ya, onGround, onState, pos)`.
    fn check_fall_damage(&mut self, level: &mut ServerLevel, ya: f64, on_ground: bool, on_state: mc_data::block::BlockState, pos: BlockPos) {
        use crate::world::level::BlockGetter;
        if self.living().is_some() {
            super::living_entity::fall::before_check_fall_damage(self.as_dyn_mut(), level, on_ground, on_state, pos);
        }
        if !self.is_in_water() && ya < 0.0 {
            self.base_mut().fall_distance -= f64::from(ya as f32);
        }
        if on_ground {
            if self.base().fall_distance > 0.0 {
                let fall_distance = self.base().fall_distance;
                crate::world::level::block::BlockStateExt::fall_on(on_state, level, pos, self.as_dyn_mut(), fall_distance);
                let ctx_state = match self.base().main_supporting_block_pos {
                    Some(p) => level.get_block_state(p),
                    None => on_state,
                };
                level.post_game_event(mc_data::GameEvent::HIT_GROUND, self.base().position(), &crate::world::level::gameevent::GameEventContext::of(Some(self.id()), Some(ctx_state)));
            }
            self.reset_fall_distance();
        }
        if self.living().is_some() {
            super::living_entity::fall::after_check_fall_damage(self.as_dyn_mut(), on_ground);
        }
    }
    /// `onAboveBubbleColumn(dragDown, pos)`.
    fn on_above_bubble_column(&mut self, level: &mut ServerLevel, drag_down: bool, pos: BlockPos) {
        if self.can_simulate_movement() {
            let movement = self.base().delta_movement();
            let yd = if drag_down { (-0.9f64).max(movement.y - 0.03) } else { 1.8f64.min(movement.y + 0.1) };
            self.base_mut().set_delta_movement(Vec3::new(movement.x, yd, movement.z));
        }
        send_bubble_column_particles(level, pos);
    }
    /// `onInsideBubbleColumn(dragDown)`.
    fn on_inside_bubble_column(&mut self, _level: &mut ServerLevel, drag_down: bool) {
        if self.can_simulate_movement() {
            let movement = self.base().delta_movement();
            let yd = if drag_down { (-0.3f64).max(movement.y - 0.03) } else { 0.7f64.min(movement.y + 0.06) };
            self.base_mut().set_delta_movement(Vec3::new(movement.x, yd, movement.z));
        }
        self.reset_fall_distance();
    }
    /// `checkFallDistanceAccumulation()`.
    fn check_fall_distance_accumulation(&mut self) {
        if self.base().delta_movement().y > -0.5 && self.base().fall_distance > 1.0 {
            self.base_mut().fall_distance = 1.0;
        }
    }
    /// `makeStuckInBlock(blockState, speedMultiplier)`.
    fn make_stuck_in_block(&mut self, _block_state: mc_data::block::BlockState, speed_multiplier: Vec3) {
        self.reset_fall_distance();
        self.base_mut().stuck_speed_multiplier = speed_multiplier;
    }
    /// `applyGravity()`.
    fn apply_gravity(&mut self) {
        let gravity = self.get_gravity();
        if gravity != 0.0 {
            let m = self.base().delta_movement();
            self.base_mut().set_delta_movement(m.add(Vec3::new(0.0, -gravity, 0.0)));
        }
    }
    /// `moveRelative(speed, input)`.
    fn move_relative(&mut self, speed: f32, input: Vec3) {
        let delta = get_input_vector(input, speed, self.base().y_rot());
        let m = self.base().delta_movement();
        self.base_mut().set_delta_movement(m.add(delta));
    }
    /// `onBelowWorld(level)`: `discard()` (living entities hurt with `fell_out_of_world`).
    fn on_below_world(&mut self, level: &mut ServerLevel) {
        if self.living().is_some() {
            let source = level.damage_sources().fell_out_of_world();
            self.hurt_server(level, &source, 4.0);
        } else {
            self.discard();
        }
    }

    // ---- portals ---------------------------------------------------------------------------------------------------

    /// `getDimensionChangingDelay()` (players 10, projectiles 2, vehicles 10: those override).
    fn get_dimension_changing_delay(&self, level: &ServerLevel) -> i32 {
        if self.is_player() {
            return 10;
        }
        match self.base().passengers.first().and_then(|&p| level.entity_manager.get_any(p)) {
            Some(first) if first.is_player() => first.get_dimension_changing_delay(level),
            _ => 300,
        }
    }
    /// `setPortalCooldown()`.
    fn set_portal_cooldown(&mut self, level: &ServerLevel) {
        let delay = self.get_dimension_changing_delay(level);
        self.base_mut().portal_cooldown = delay;
    }
    /// `isOnPortalCooldown()`.
    fn is_on_portal_cooldown(&self) -> bool {
        self.base().portal_cooldown > 0
    }
    /// `processPortalCooldown()`.
    fn process_portal_cooldown(&mut self) {
        if self.is_on_portal_cooldown() {
            self.base_mut().portal_cooldown -= 1;
        }
    }
    /// `canUsePortal(ignorePassenger)`.
    fn can_use_portal(&self, ignore_passenger: bool) -> bool {
        (ignore_passenger || !self.base().is_passenger()) && self.is_alive()
    }
    /// `canTeleport(from, to)`: no player passenger that has not seen the credits may leave the End for the overworld.
    fn can_teleport(&self, level: &ServerLevel, from: &mc_core::ident::Identifier, to: &mc_core::ident::Identifier) -> bool {
        if *from == crate::server::world_dimensions::end_key() && *to == crate::server::world_dimensions::overworld_key() {
            for &pid in &self.base().passengers {
                if level.entity_manager.get_any(pid).and_then(|p| p.as_any().downcast_ref::<super::ServerPlayer>()).is_some_and(|p| !p.seen_credits) {
                    return false;
                }
            }
        }
        true
    }
    /// `Leashable.removeLeash()` of `removeAfterChangingDimensions` (the mob slice overrides it for leashable mobs).
    fn remove_leash(&mut self, _level: &mut ServerLevel) {}
    /// `setAsInsidePortal(portal, pos)`.
    fn set_as_inside_portal(&mut self, level: &ServerLevel, portal: &'static dyn super::portal::Portal, pos: BlockPos) {
        if self.is_on_portal_cooldown() {
            self.set_portal_cooldown(level);
        } else {
            let b = self.base_mut();
            match &mut b.portal_process {
                Some(p) if p.is_same_portal(portal) => {
                    if !p.is_inside_portal_this_tick() {
                        p.update_entry_position(pos);
                        p.set_as_inside_portal_this_tick(true);
                    }
                }
                _ => b.portal_process = Some(super::portal::PortalProcessor::new(portal, pos)),
            }
        }
    }
    /// `handlePortal()`: the teleport itself is queued on the level (see `world::entity::portal`).
    fn handle_portal(&mut self, level: &mut ServerLevel) {
        self.process_portal_cooldown();
        if let Some(mut process) = self.base_mut().portal_process.take() {
            let can_use = self.can_use_portal(false);
            if process.process_portal_teleportation(&*level, self.as_dyn(), can_use) {
                self.set_portal_cooldown(&*level);
                level.queue_portal_teleport(self.id());
                self.base_mut().portal_process = Some(process);
            } else if !process.has_expired() {
                self.base_mut().portal_process = Some(process);
            }
        }
    }

    // ---- riding / passengers ------------------------------------------------------------------------------------

    /// `getVehicleAttachmentPoint(vehicle)`.
    fn get_vehicle_attachment_point(&self, _vehicle: &dyn Entity) -> Vec3 {
        self.get_attachments().get(EntityAttachment::Vehicle, 0, self.base().y_rot())
    }
    /// `getPassengerAttachmentPoint(passenger, dimensions, scale)`.
    fn get_passenger_attachment_point(&self, passenger: &dyn Entity, dimensions: &EntityDimensions, _scale: f32) -> Vec3 {
        default_passenger_attachment_point(self.as_dyn(), passenger, &dimensions.attachments)
    }
    /// `getPassengerRidingPosition(passenger)`.
    fn get_passenger_riding_position(&self, passenger: &dyn Entity) -> Vec3 {
        let base = self.base();
        base.position().add(self.get_passenger_attachment_point(passenger, &base.dimensions, 1.0))
    }
    /// `positionRider(passenger, moveFunction)` (protected, overridden by boats, minecarts, ...).
    fn position_rider_with(&self, passenger: &mut dyn Entity, move_function: MoveFunction) {
        let position = self.get_passenger_riding_position(passenger.as_dyn());
        let offset = passenger.get_vehicle_attachment_point(self.as_dyn());
        move_function.apply(passenger, position.x - offset.x, position.y - offset.y, position.z - offset.z);
    }
    /// `positionRider(passenger)`.
    fn position_rider(&self, passenger: &mut dyn Entity) {
        if self.base().passengers.contains(&passenger.id()) {
            self.position_rider_with(passenger, MoveFunction::SetPos);
        }
    }
    /// `onPassengerTurned(passenger)`.
    fn on_passenger_turned(&mut self, _passenger: &mut dyn Entity) {}
    /// `getDismountLocationForPassenger(passenger)`.
    fn get_dismount_location_for_passenger(&self, _level: &ServerLevel, _passenger: &dyn Entity) -> Vec3 {
        let b = self.base();
        Vec3::new(b.x(), b.bounding_box().max_y, b.z())
    }
    /// `addPassenger(passenger)`: `first_passenger_is_player` = `getFirstPassenger() instanceof Player`.
    fn add_passenger(&mut self, passenger: &dyn Entity, first_passenger_is_player: bool) {
        let id = passenger.id();
        let is_player = passenger.is_player();
        let passengers = &mut self.base_mut().passengers;
        if passengers.is_empty() || !(is_player && !first_passenger_is_player) {
            passengers.push(id);
        } else {
            passengers.insert(0, id);
        }
    }
    /// `removePassenger(passenger)` (the vehicle side; the boarding cooldown is the rider's).
    fn remove_passenger(&mut self, passenger: EntityId) {
        self.base_mut().passengers.retain(|&p| p != passenger);
    }
    /// `dismountsUnderwater()`.
    fn dismounts_underwater(&self) -> bool {
        mc_data::tags::get::<EntityType>("dismounts_underwater").is_some_and(|t| t.contains(self.get_type()))
    }
    /// `canControlVehicle()`.
    fn can_control_vehicle(&self) -> bool {
        !mc_data::tags::get::<EntityType>("non_controlling_rider").is_some_and(|t| t.contains(self.get_type()))
    }
    /// `getRootVehicle()`.
    fn get_root_vehicle(&self, level: &ServerLevel) -> EntityId {
        let mut id = self.id();
        let mut cur = self.base().vehicle;
        while let Some(v) = cur {
            id = v;
            cur = level.entity_manager.get_any(v).and_then(|e| e.base().vehicle);
        }
        id
    }
    /// `isPassengerOfSameVehicle(other)`.
    fn is_passenger_of_same_vehicle(&self, level: &ServerLevel, other: &dyn Entity) -> bool {
        level.is_passenger_of_same_vehicle(self.as_dyn(), other)
    }
    /// `hasIndirectPassenger(entity)`.
    fn has_indirect_passenger(&self, level: &ServerLevel, entity: &dyn Entity) -> bool {
        let mut cur = entity.base().vehicle;
        while let Some(v) = cur {
            if v == self.id() {
                return true;
            }
            cur = level.entity_manager.get_any(v).and_then(|e| e.base().vehicle);
        }
        false
    }
    /// `getIndirectPassengers()`: every passenger below this entity (pre-order, `getSelfAndPassengers()` without self).
    fn get_indirect_passengers(&self, level: &ServerLevel) -> Vec<EntityId> {
        let mut out = Vec::new();
        fn walk(level: &ServerLevel, passengers: &[EntityId], out: &mut Vec<EntityId>) {
            for &p in passengers {
                out.push(p);
                if let Some(e) = level.entity_manager.get_any(p) {
                    walk(level, &e.base().passengers, out);
                }
            }
        }
        walk(level, &self.base().passengers, &mut out);
        out
    }
    /// `getSelfAndPassengers()`.
    fn get_self_and_passengers(&self, level: &ServerLevel) -> Vec<EntityId> {
        let mut out = vec![self.id()];
        out.extend(self.get_indirect_passengers(level));
        out
    }
    /// `canCollideWith(entity)`.
    fn can_collide_with(&self, level: &ServerLevel, entity: &dyn Entity) -> bool {
        entity.can_be_collided_with(Some(self.as_dyn())) && !self.is_passenger_of_same_vehicle(level, entity)
    }
    /// `getKnownMovement()`.
    fn get_known_movement(&self, level: &ServerLevel) -> Vec3 {
        if let Some(controller) = self.get_controlling_passenger().and_then(|id| level.entity_manager.get_any(id)) {
            if controller.is_player() && self.is_alive() {
                return controller.get_known_movement(level);
            }
        }
        self.base().delta_movement()
    }
    /// `getKnownSpeed()`.
    fn get_known_speed(&self, level: &ServerLevel) -> Vec3 {
        if let Some(controller) = self.get_controlling_passenger().and_then(|id| level.entity_manager.get_any(id)) {
            if controller.is_player() && self.is_alive() {
                return controller.get_known_speed(level);
            }
        }
        self.base().last_known_speed
    }

    // ---- teleport / push / look ---------------------------------------------------------------------------------

    /// `teleportTo(x, y, z)` (same level; `ServerPlayer` sends a client teleport instead).
    fn teleport_to(&mut self, level: &mut ServerLevel, x: f64, y: f64, z: f64) {
        let (yr, xr) = (self.base().y_rot(), self.base().x_rot());
        self.snap_to(x, y, z, yr, xr);
        teleport_passengers(self.as_dyn_mut(), level);
    }
    /// `teleportRelative(dx, dy, dz)`.
    fn teleport_relative(&mut self, level: &mut ServerLevel, dx: f64, dy: f64, dz: f64) {
        let p = self.base().position();
        self.teleport_to(level, p.x + dx, p.y + dy, p.z + dz);
    }
    /// `dismountTo(x, y, z)`.
    fn dismount_to(&mut self, level: &mut ServerLevel, x: f64, y: f64, z: f64) {
        self.teleport_to(level, x, y, z);
    }
    /// `push(entity)`: separates two overlapping entities (`other` is the entity this one collides with).
    fn push_entity(&mut self, level: &ServerLevel, other: &mut dyn Entity) {
        // `LivingEntity.push`: sleeping entities do not take part
        if self.living().is_some_and(|l| l.is_sleeping()) || self.is_passenger_of_same_vehicle(level, other.as_dyn()) {
            return;
        }
        if other.base().no_physics || self.base().no_physics {
            return;
        }
        let mut xa = other.base().x() - self.base().x();
        let mut za = other.base().z() - self.base().z();
        let mut dd = mth::abs_max_f64(xa, za);
        if dd >= f64::from(0.01f32) {
            dd = dd.sqrt();
            xa /= dd;
            za /= dd;
            let mut pow = 1.0 / dd;
            if pow > 1.0 {
                pow = 1.0;
            }
            xa *= pow;
            za *= pow;
            xa *= f64::from(0.05f32);
            za *= f64::from(0.05f32);
            if !self.base().is_vehicle() && self.is_pushable(level) {
                self.push_xyz(-xa, 0.0, -za);
            }
            if !other.base().is_vehicle() && other.is_pushable(level) {
                other.push_xyz(xa, 0.0, za);
            }
        }
    }
    /// `push(impulse)`.
    fn push_vec(&mut self, impulse: Vec3) {
        if impulse.is_finite() {
            self.push_xyz(impulse.x, impulse.y, impulse.z);
        }
    }
    /// `pushFromExplosion(impulse)`.
    fn push_from_explosion(&mut self, impulse: Vec3) {
        self.push_vec(impulse);
    }
    /// `push(xa, ya, za)`.
    fn push_xyz(&mut self, xa: f64, ya: f64, za: f64) {
        if xa.is_finite() && ya.is_finite() && za.is_finite() {
            let b = self.base_mut();
            let m = b.delta_movement();
            b.set_delta_movement(m.add(Vec3::new(xa, ya, za)));
            b.needs_sync = true;
        }
    }
    /// `markHurt()`.
    fn mark_hurt(&mut self) {
        self.base_mut().sync_velocity = true;
    }
    /// `getLookAngle()`.
    fn get_look_angle(&self) -> Vec3 {
        calculate_view_vector(self.base().x_rot(), self.base().y_rot())
    }
    /// `getHeadLookAngle()`.
    fn get_head_look_angle(&self) -> Vec3 {
        calculate_view_vector(self.base().x_rot(), self.get_y_head_rot())
    }
    /// `getViewXRot(a)` / `getXRot(a)`.
    fn get_view_x_rot(&self, a: f32) -> f32 {
        if a == 1.0 { self.base().x_rot() } else { mth::lerp_f32(a, self.base().x_rot_o, self.base().x_rot()) }
    }
    /// `getViewYRot(a)` / `getYRot(a)`.
    fn get_view_y_rot(&self, a: f32) -> f32 {
        if a == 1.0 { self.base().y_rot() } else { mth::rot_lerp_f32(a, self.base().y_rot_o, self.base().y_rot()) }
    }
    /// `getViewVector(a)`.
    fn get_view_vector(&self, a: f32) -> Vec3 {
        calculate_view_vector(self.get_view_x_rot(a), self.get_view_y_rot(a))
    }
    /// `getEyePosition(partialTick)`.
    fn get_eye_position_partial(&self, a: f32) -> Vec3 {
        let b = self.base();
        let x = mth::lerp_f64(f64::from(a), b.xo, b.x());
        let y = mth::lerp_f64(f64::from(a), b.yo, b.y()) + f64::from(b.eye_height);
        let z = mth::lerp_f64(f64::from(a), b.zo, b.z());
        Vec3::new(x, y, z)
    }
    /// `pick(range, a, withLiquids)`.
    fn pick(&self, level: &ServerLevel, range: f64, a: f32, with_liquids: bool) -> mc_core::phys::BlockHitResult {
        use crate::world::level::collision::{ClipBlock, ClipContext, ClipFluid};
        let from = self.get_eye_position_partial(a);
        let view_vector = self.get_view_vector(a);
        let to = from.add(Vec3::new(view_vector.x * range, view_vector.y * range, view_vector.z * range));
        crate::world::level::collision::BlockGetterExt::clip(level, &ClipContext::new(from, to, ClipBlock::Outline, if with_liquids { ClipFluid::Any } else { ClipFluid::None }, super::movement::collision_context(self.as_dyn())))
    }
    /// `lookAt(anchor, pos)`.
    fn look_at(&mut self, anchor: crate::commands::arguments::entity_anchor_argument::Anchor, pos: Vec3) {
        let from = anchor.apply_entity(self.base().position(), self.base().eye_height);
        let (xd, yd, zd) = (pos.x - from.x, pos.y - from.y, pos.z - from.z);
        let sd = (xd * xd + zd * zd).sqrt();
        let pi = std::f64::consts::PI as f32;
        self.set_x_rot(mth::wrap_degrees_f32((-(mth::atan2(yd, sd) * f64::from(180.0f32) / f64::from(pi))) as f32));
        self.set_y_rot(mth::wrap_degrees_f32((mth::atan2(zd, xd) * f64::from(180.0f32) / f64::from(pi)) as f32 - 90.0));
        let y_rot = self.base().y_rot();
        self.set_y_head_rot(y_rot);
        let b = self.base_mut();
        b.x_rot_o = b.x_rot();
        b.y_rot_o = b.y_rot();
    }
    /// `canBeHitByProjectile()`.
    fn can_be_hit_by_projectile(&self) -> bool {
        self.is_alive() && self.is_pickable()
    }
    /// `projectileReceivesSideEffectsOnHit(wasHurt)` (Enderman: `wasHurt`).
    fn projectile_receives_side_effects_on_hit(&self, _was_hurt: bool) -> bool {
        true
    }
    /// `mayInteract(level, pos)`.
    fn may_interact(&self, _level: &ServerLevel, _pos: BlockPos) -> bool {
        true
    }
    /// `getRequiresPrecisePosition()`.
    fn get_requires_precise_position(&self) -> bool {
        self.base().requires_precise_position
    }
    /// `setRequiresPrecisePosition(requiresPrecisePosition)`.
    fn set_requires_precise_position(&mut self, requires_precise_position: bool) {
        self.base_mut().requires_precise_position = requires_precise_position;
    }
    /// What `ServerPlayer.removeVehicle()` adds after the base `removeVehicle`: `old_vehicle` (in storage, or `held`
    /// when it is the entity being processed) lost this entity as a passenger.
    fn on_vehicle_removed(&mut self, _level: &mut ServerLevel, _old_vehicle: EntityId, _held: Option<&dyn Entity>) {}
    /// `isSteppingCarefully()`.
    fn is_stepping_carefully(&self) -> bool {
        self.is_shift_key_down()
    }
    /// `isDiscrete()`.
    fn is_discrete(&self) -> bool {
        self.is_shift_key_down()
    }
    // ==== [LivingCore] additions (Entity trait methods) ====
    // ---- `LivingEntity` behaviour: defaults are the vanilla methods for every entity whose `living()` is Some;
    // ---- mobs/players override the hooks vanilla overrides (`travel`, `serverAiStep`, `isImmobile`, ...) -----------

    /// `isEffectiveAi()` (server side: `Player` true, `Mob` `!isNoAi()`).
    fn is_effective_ai(&self) -> bool {
        !self.mob().is_some_and(|m| m.is_no_ai())
    }
    /// `isImmobile()` (`LivingEntity`: dead or dying; `Player` adds sleeping).
    fn is_immobile(&self) -> bool {
        self.living().is_some_and(|l| l.is_dead_or_dying())
    }
    /// `serverAiStep()` (`Mob`: brain, goals, controls).
    fn server_ai_step(&mut self, level: &mut ServerLevel) {
        if self.mob().is_some() {
            super::mob::tick::server_ai_step(self.as_dyn_mut(), level);
        }
    }
    /// `aiStep()`.
    fn ai_step(&mut self, level: &mut ServerLevel) {
        if self.mob().is_some() {
            super::mob::tick::ai_step(self.as_dyn_mut(), level);
            return;
        }
        super::living_entity::ai_step::ai_step(self.as_dyn_mut(), level);
    }
    /// `travel(input)`.
    fn travel(&mut self, level: &mut ServerLevel, input: Vec3) {
        super::living_entity::travel::travel(self.as_dyn_mut(), level, input);
    }
    /// `travelInFluid(input)`.
    fn travel_in_fluid(&mut self, level: &mut ServerLevel, input: Vec3) {
        super::living_entity::travel::travel_in_fluid(self.as_dyn_mut(), level, input);
    }
    /// `travelInWater(input, baseGravity, isFalling, oldY)`.
    fn travel_in_water(&mut self, level: &mut ServerLevel, input: Vec3, base_gravity: f64, is_falling: bool, old_y: f64) {
        super::living_entity::travel::travel_in_water(self.as_dyn_mut(), level, input, base_gravity, is_falling, old_y);
    }
    /// `getSpeed()` (`Player`: the `MOVEMENT_SPEED` attribute).
    fn get_speed(&self) -> f32 {
        self.living().map_or(0.0, |l| l.speed)
    }
    /// `setSpeed(speed)`.
    fn set_speed(&mut self, speed: f32) {
        let is_mob = self.mob().is_some();
        if let Some(l) = self.living_mut() {
            l.speed = speed;
            if is_mob {
                l.zza = speed; // `Mob.setSpeed`: `setZza(speed)`
            }
        }
    }
    /// `getFlyingSpeed()`.
    fn get_flying_speed(&self, level: &ServerLevel) -> f32 {
        super::living_entity::travel::get_flying_speed(self.as_dyn(), level)
    }
    /// `getWaterSlowDown()`.
    fn get_water_slow_down(&self) -> f32 {
        0.8
    }
    /// `isAffectedByFluids()`.
    fn is_affected_by_fluids(&self) -> bool {
        true
    }
    /// `omnidirectionalAirMover()`.
    fn omnidirectional_air_mover(&self) -> bool {
        false
    }
    /// `jumpInLiquid(fluid)`.
    fn jump_in_liquid(&mut self, _fluid: super::living_entity::util::FluidKind) {
        super::living_entity::jump::jump_in_liquid(self.as_dyn_mut());
    }
    /// `goDownInWater()`.
    fn go_down_in_water(&mut self) {
        super::living_entity::jump::go_down_in_water(self.as_dyn_mut());
    }
    /// `getJumpPower()`.
    fn get_jump_power(&self, level: &ServerLevel) -> f32 {
        super::living_entity::jump::get_jump_power(self.as_dyn(), level)
    }
    /// `jumpFromGround()`.
    fn jump_from_ground(&mut self, level: &ServerLevel) {
        super::living_entity::jump::jump_from_ground(self.as_dyn_mut(), level);
    }
    /// `onClimbable()` without the `lastClimbablePos` side effect (`ai_step`/`travel` record it themselves).
    fn on_climbable(&self, level: &ServerLevel) -> bool {
        super::living_entity::travel::on_climbable(self.as_dyn(), level)
    }
    /// `canGlide()`.
    fn can_glide(&self) -> bool {
        super::living_entity::fall_flying::can_glide(self.as_dyn())
    }
    /// `updateFallFlying()`.
    fn update_fall_flying(&mut self, level: &mut ServerLevel) {
        super::living_entity::fall_flying::update_fall_flying(self.as_dyn_mut(), level);
    }
    /// `ItemStack.hurtAndBreak(amount, this, slot)` on the item in `slot` (elytra wear); `Player` owners use the player
    /// item path.
    fn hurt_and_break_equipment(&mut self, level: &mut ServerLevel, slot: super::equipment::EquipmentSlot, amount: i32) {
        super::living_entity::fall_flying::hurt_and_break_equipment(self.as_dyn_mut(), level, slot, amount);
    }
    /// `pushEntities()`.
    fn push_entities(&mut self, level: &mut ServerLevel) {
        super::living_entity::push::push_entities(self.as_dyn_mut(), level);
    }
    /// `doPush(entity)` = `entity.push(this)`.
    fn do_push(&mut self, level: &mut ServerLevel, other: &mut dyn Entity) {
        super::living_entity::push::do_push(level, self.as_dyn_mut(), other);
    }
    /// `tickHeadTurn(yBodyRotT)`.
    fn tick_head_turn(&mut self, y_body_rot_t: f32) {
        if self.mob().is_some() {
            super::mob::tick::tick_head_turn(self.as_dyn_mut());
            return;
        }
        super::living_entity::tick::tick_head_turn(self.as_dyn_mut(), y_body_rot_t);
    }
    /// `getMaxHeadRotationRelativeToBody()`.
    fn get_max_head_rotation_relative_to_body(&self) -> f32 {
        50.0
    }
    /// `canBreatheUnderwater()` (entity type tag `can_breathe_under_water`).
    fn can_breathe_underwater(&self) -> bool {
        mc_data::tags::get::<EntityType>("can_breathe_under_water").is_some_and(|t| t.contains(self.get_type()))
    }
    /// `decreaseAirSupply(currentSupply)`.
    fn decrease_air_supply(&mut self, current_supply: i32) -> i32 {
        super::living_entity::base_tick::decrease_air_supply(self.as_dyn_mut(), current_supply)
    }
    /// `increaseAirSupply(currentSupply)`.
    fn increase_air_supply(&mut self, current_supply: i32) -> i32 {
        (current_supply + 4).min(self.get_max_air_supply())
    }
    /// `shouldTakeDrowningDamage()`.
    fn should_take_drowning_damage(&self) -> bool {
        self.base().get_air_supply() <= -20
    }
    /// `isSensitiveToWater()`.
    fn is_sensitive_to_water(&self) -> bool {
        false
    }
    /// `isSuppressingSlidingDownLadder()`.
    fn is_suppressing_sliding_down_ladder(&self) -> bool {
        self.is_shift_key_down()
    }
    /// `getBlockSpeedFactor()` modifier: `LivingEntity` lerps towards 1 by the `MOVEMENT_EFFICIENCY` attribute, `Player`
    /// ignores block speed factors while flying/gliding.
    fn modify_block_speed_factor(&self, base: f32) -> f32 {
        match self.living() {
            Some(l) => mth::lerp_f32(l.get_attribute_value(mc_data::Attribute::MOVEMENT_EFFICIENCY) as f32, base, 1.0),
            None => base,
        }
    }
    /// `setSprinting(isSprinting)` (`LivingEntity`: also toggles the sprinting speed modifier).
    fn set_sprinting(&mut self, sprinting: bool) {
        super::living_entity::set_sprinting(self.as_dyn_mut(), sprinting);
    }
    /// `canBeSeenByAnyone()`.
    fn can_be_seen_by_anyone(&self) -> bool {
        !self.is_spectator() && self.is_alive()
    }
    /// `canBeSeenAsEnemy()`.
    fn can_be_seen_as_enemy(&self) -> bool {
        !self.base().permanently_invulnerable && self.can_be_seen_by_anyone()
    }
    /// `EntityEquipment.tick(this)` (`Item.inventoryTick` of the equipped stacks); items run on a `ServerPlayer` owner.
    fn tick_equipment(&mut self, _level: &mut ServerLevel) {}
    /// `updatingUsingItem()` (`LivingEntity.tick`); the item state machine is `ServerPlayer`'s.
    fn updating_using_item(&mut self, _level: &mut ServerLevel) {}
    /// `getRiddenInput(controller, selfInput)`.
    fn get_ridden_input(&self, _level: &ServerLevel, _controller: EntityId, self_input: Vec3) -> Vec3 {
        self_input
    }
    /// `tickRidden(controller, riddenInput)`.
    fn tick_ridden(&mut self, _level: &mut ServerLevel, _controller: EntityId, _ridden_input: Vec3) {}
    /// `getRiddenSpeed(controller)`.
    fn get_ridden_speed(&self, _level: &ServerLevel, _controller: EntityId) -> f32 {
        self.get_speed()
    }
    /// `getFallSounds()` as `(small, big)`.
    fn get_fall_sounds(&self) -> (mc_data::SoundEvent, mc_data::SoundEvent) {
        (mc_data::SoundEvent::ENTITY_GENERIC_SMALL_FALL, mc_data::SoundEvent::ENTITY_GENERIC_BIG_FALL)
    }
    /// `canInteractWithLevel()`.
    fn can_interact_with_level(&self) -> bool {
        self.is_alive() && !self.is_removed() && !self.is_spectator()
    }
    /// `tickDeath()`.
    fn tick_death(&mut self, level: &mut ServerLevel) {
        super::living_entity::base_tick::tick_death(self.as_dyn_mut(), level);
    }
    /// `doAutoAttackOnTouch(entity)` (riptide spin attack; `Player`).
    fn do_auto_attack_on_touch(&mut self, _level: &mut ServerLevel, _target: &mut dyn Entity) {}
    /// `randomTeleport(x, y, z, showParticles, avoidanceTag)` (block tag name without namespace).
    fn random_teleport(&mut self, level: &mut ServerLevel, x: f64, y: f64, z: f64, show_particles: bool, avoidance_tag: &str) -> bool {
        super::living_entity::teleport::random_teleport(self.as_dyn_mut(), level, x, y, z, show_particles, avoidance_tag)
    }
    /// `resetCurrentImpulseContext()`.
    fn reset_current_impulse_context(&mut self) {
        super::living_entity::teleport::reset_current_impulse_context(self.as_dyn_mut());
    }
    /// `setLastHurtMob(target)`.
    fn set_last_hurt_mob(&mut self, target: &dyn Entity) {
        let tick_count = self.base().tick_count;
        if let Some(l) = self.living_mut() {
            l.last_hurt_mob = target.living().is_some().then(|| target.id());
            l.last_hurt_mob_timestamp = tick_count;
        }
    }
    /// `applyInput()` (`LivingEntity`: the movement input decays by 0.98; `LocalPlayer` reads the keyboard).
    fn apply_input(&mut self) {
        if let Some(l) = self.living_mut() {
            l.xxa *= 0.98;
            l.zza *= 0.98;
        }
    }
    /// `doesEmitEquipEvent(slot)` (`Player`: armor slots only).
    fn does_emit_equip_event(&self, _slot: super::equipment::EquipmentSlot) -> bool {
        true
    }
    /// `onEquipItem(slot, oldStack, stack)`.
    fn on_equip_item(&mut self, level: &mut ServerLevel, slot: super::equipment::EquipmentSlot, old: &crate::world::item::ItemStack, new: &crate::world::item::ItemStack) {
        super::living_entity::equipment::on_equip_item(self.as_dyn_mut(), level, slot, old, new);
    }
    /// `getArmorCoverPercentage()`.
    fn get_armor_cover_percentage(&self) -> f32 {
        super::living_entity::equipment::get_armor_cover_percentage(self.as_dyn())
    }
    /// `startSleeping(bedPosition)`.
    fn start_sleeping(&mut self, level: &mut ServerLevel, bed_position: BlockPos) -> bool {
        super::living_entity::sleeping::start_sleeping(self.as_dyn_mut(), level, bed_position)
    }
    /// `stopSleeping()`.
    fn stop_sleeping(&mut self, level: &mut ServerLevel) {
        super::living_entity::sleeping::stop_sleeping(self.as_dyn_mut(), level);
    }
    /// `getBedOrientation()`.
    fn get_bed_orientation(&self, level: &ServerLevel) -> Option<mc_core::direction::Direction> {
        super::living_entity::sleeping::get_bed_orientation(self.as_dyn(), level)
    }
    // ==== [Damage] additions (Entity trait methods) ====
    // Defaults implement `Entity`/`LivingEntity`; `Player`/`ServerPlayer` and mobs override and call the free
    // functions of `living_entity::damage` as their `super` call.

    /// A read-only copy of this entity for [`crate::world::damagesource::DamageEntity::capture`]: loot and experience
    /// evaluation at a victim's death reads the attacker (equipment, attributes, effects) while the attacker is taken
    /// out of the level. `None`: no copy (readers then see no attacker entity if it is out).
    fn snapshot_for_damage(&self) -> Option<Box<dyn Entity>> {
        None
    }

    /// `getWeaponItem()`: `LivingEntity` = main hand; projectiles override with the weapon that fired them.
    fn get_weapon_item(&self) -> Option<crate::world::item::ItemStack> {
        self.living().map(|_| self.get_item_by_slot(super::equipment::EquipmentSlot::MainHand))
    }

    /// `isInvulnerable()`.
    fn is_invulnerable(&self) -> bool {
        self.is_permanently_invulnerable() || self.is_temporarily_invulnerable()
    }

    /// `isPermanentlyInvulnerable()`.
    fn is_permanently_invulnerable(&self) -> bool {
        self.base().permanently_invulnerable
    }

    /// `isTemporarilyInvulnerable()`.
    fn is_temporarily_invulnerable(&self) -> bool {
        self.base().invulnerable_time > 0
    }

    /// `isInvulnerableToPiercingWeapon()`.
    fn is_invulnerable_to_piercing_weapon(&self) -> bool {
        self.is_invulnerable()
    }

    /// `isInvulnerableToBase(source)`.
    fn is_invulnerable_to_base(&self, level: &ServerLevel, source: &crate::world::damagesource::DamageSource) -> bool {
        use crate::world::damagesource::DamageTypeTags;
        self.is_removed()
            || self.is_invulnerable() && !source.is_tag(level, DamageTypeTags::BYPASSES_INVULNERABILITY) && !source.is_creative_player()
            || source.is_tag(level, DamageTypeTags::IS_FIRE) && self.fire_immune()
            || source.is_tag(level, DamageTypeTags::IS_FALL) && mc_data::tags::get::<EntityType>("fall_damage_immune").is_some_and(|t| t.contains(self.get_type()))
    }

    /// `isInvulnerableTo(level, source)` (`LivingEntity`: plus `EnchantmentHelper.isImmuneToDamage`).
    fn is_invulnerable_to(&mut self, level: &mut ServerLevel, source: &crate::world::damagesource::DamageSource) -> bool {
        super::living_entity::damage::is_invulnerable_to(self.as_dyn_mut(), level, source)
    }

    /// `hurtServer(level, source, damage)`: abstract in `Entity` (non-living entities override); every living entity runs `LivingEntity.hurtServer`.
    fn hurt_server(&mut self, level: &mut ServerLevel, source: &crate::world::damagesource::DamageSource, amount: f32) -> bool {
        super::living_entity::damage::hurt_server(self.as_dyn_mut(), level, source, amount)
    }

    /// `die(source)` (`LivingEntity`; no-op for non-living entities).
    fn die(&mut self, level: &mut ServerLevel, source: &crate::world::damagesource::DamageSource) {
        super::living_entity::damage::die(self.as_dyn_mut(), level, source);
    }

    /// `causeFallDamage(fallDistance, damageModifier, source)`.
    fn cause_fall_damage(&mut self, level: &mut ServerLevel, fall_distance: f64, damage_modifier: f32, source: &crate::world::damagesource::DamageSource) -> bool {
        super::living_entity::damage::cause_fall_damage(self.as_dyn_mut(), level, fall_distance, damage_modifier, source)
    }

    /// `kill(level)`: `Entity` removes itself (`KILLED`) and posts `ENTITY_DIE`; `LivingEntity` hurts itself with `generic_kill`.
    fn kill_in_level(&mut self, level: &mut ServerLevel) {
        if self.living().is_some() {
            let source = level.damage_sources().generic_kill();
            self.hurt_server(level, &source, f32::MAX);
        } else {
            self.remove(RemovalReason::Killed);
            level.game_event_from_entity(Some(self.id()), mc_data::GameEvent::ENTITY_DIE, self.position());
        }
    }

    /// `heal(amount)`.
    fn heal(&mut self, _level: &mut ServerLevel, amount: f32) {
        if let Some(l) = self.living_mut() {
            let health = l.get_health();
            if health > 0.0 {
                l.set_health(health + amount);
            }
        }
    }

    /// `getHealth()` (0 for non-living entities).
    fn get_health(&self) -> f32 {
        self.living().map_or(0.0, |l| l.get_health())
    }

    /// `isDeadOrDying()`.
    fn is_dead_or_dying(&self) -> bool {
        self.living().is_some_and(|l| l.get_health() <= 0.0 || l.dead)
    }

    /// `getArmorValue()`.
    fn get_armor_value(&self) -> i32 {
        self.living().map_or(0, |l| mth::floor_f64(l.get_attribute_value(mc_data::Attribute::ARMOR)))
    }

    /// `isBlocking()`.
    fn is_blocking(&self) -> bool {
        super::living_entity::blocking::get_item_blocking_with(self.as_dyn()).is_some()
    }

    /// `getSecondsToDisableBlocking()`.
    fn get_seconds_to_disable_blocking(&self) -> f32 {
        super::living_entity::blocking::seconds_to_disable_blocking(self.as_dyn())
    }

    /// `AbstractArrow.getPierceLevel()`.
    fn get_pierce_level(&self) -> i32 {
        0
    }

    /// `Projectile.calculateHorizontalHurtKnockbackDirection` (`None`: not a projectile).
    fn hurt_knockback_direction(&self) -> Option<crate::world::damagesource::HurtKnockbackDirection> {
        None
    }

    /// `Wolf.isTame()` + `getOwnerReference()`: `Some(owner)` for a tamed wolf (`resolvePlayerResponsibleForDamage`).
    fn tamed_wolf_owner(&self) -> Option<Option<Uuid>> {
        None
    }

    /// `getHurtSound(source)` (`Player`: the damage type's `effects` sound).
    fn get_hurt_sound(&self, _source: &crate::world::damagesource::DamageSource) -> Option<mc_data::SoundEvent> {
        Some(mc_data::SoundEvent::ENTITY_GENERIC_HURT)
    }

    /// `getDeathSound()`.
    fn get_death_sound(&self) -> Option<mc_data::SoundEvent> {
        Some(mc_data::SoundEvent::ENTITY_GENERIC_DEATH)
    }

    /// `getSoundVolume()`.
    fn get_sound_volume(&self) -> f32 {
        1.0
    }

    /// `getVoicePitch()` (draws two floats from the entity's random; players: 1.0).
    fn get_voice_pitch(&mut self) -> f32 {
        super::living_entity::damage::voice_pitch(self.as_dyn_mut())
    }

    /// `indicateDamage(xd, zd)` (`ServerPlayer`: the hurt animation packet).
    fn indicate_damage(&mut self, _level: &mut ServerLevel, _xd: f64, _zd: f64) {}

    /// `knockback(power, xd, zd, source, damage, comesFromEffect)`.
    fn knockback(&mut self, power: f64, xd: f64, zd: f64, source: &crate::world::damagesource::DamageSource, damage: f32, comes_from_effect: bool) {
        super::living_entity::damage::knockback(self.as_dyn_mut(), power, xd, zd, source, damage, comes_from_effect);
    }

    /// `actuallyHurt(level, source, damage)`.
    fn actually_hurt(&mut self, level: &mut ServerLevel, source: &crate::world::damagesource::DamageSource, damage: f32) {
        if self.mob().is_some_and(|m| m.animal.is_some()) {
            super::mob::animal::reset_love(self.as_dyn_mut()); // `Animal.actuallyHurt`
        }
        super::living_entity::damage::actually_hurt(self.as_dyn_mut(), level, source, damage);
    }

    /// `hurtArmor(source, damage)` (players damage their armor; mobs do nothing).
    fn hurt_armor(&mut self, _level: &mut ServerLevel, _source: &crate::world::damagesource::DamageSource, _damage: f32) {}

    /// `hurtHelmet(source, damage)`.
    fn hurt_helmet(&mut self, _level: &mut ServerLevel, _source: &crate::world::damagesource::DamageSource, _damage: f32) {}

    /// `blockUsingItem(level, attacker, source, damage, fullyBlocked)` (`attacker` = the direct living entity).
    fn block_using_item(
        &mut self,
        level: &mut ServerLevel,
        attacker: &crate::world::damagesource::DamageEntity,
        source: &crate::world::damagesource::DamageSource,
        damage: f32,
        fully_blocked: bool,
    ) {
        super::living_entity::blocking::block_using_item(self.as_dyn_mut(), level, attacker, source, damage, fully_blocked);
    }

    /// `onEnterCombat()` / `onLeaveCombat()` (`ServerPlayer` sends the combat packets).
    fn on_enter_combat(&mut self, _level: &mut ServerLevel) {}
    fn on_leave_combat(&mut self, _level: &mut ServerLevel) {}

    /// `dropEquipment(level)`.
    fn drop_equipment(&mut self, _level: &mut ServerLevel) {}

    /// `dropAllDeathLoot(level, source)`.
    fn drop_all_death_loot(&mut self, level: &mut ServerLevel, source: &crate::world::damagesource::DamageSource) {
        super::living_entity::death::drop_all_death_loot(self.as_dyn_mut(), level, source);
    }

    /// `dropCustomDeathLoot(level, source, killedByPlayer)`.
    fn drop_custom_death_loot(&mut self, level: &mut ServerLevel, source: &crate::world::damagesource::DamageSource, killed_by_player: bool) {
        if self.mob().is_some() {
            super::mob::equipment::drop_custom_death_loot(self.as_dyn_mut(), level, source, killed_by_player);
        }
    }

    /// `getLootTable()`: the loot table of the death drops (`None`: no drops).
    fn get_loot_table(&self) -> Option<mc_core::ident::Identifier> {
        if let Some(t) = self.mob().and_then(|m| m.loot_table.clone()) {
            return Some(t);
        }
        super::living_entity::death::default_loot_table(self.as_dyn())
    }

    /// `getLootTableSeed()`.
    fn get_loot_table_seed(&self) -> i64 {
        self.mob().map_or(0, |m| m.loot_table_seed)
    }

    /// `shouldDropExperience()` / `isAlwaysExperienceDropper()` / `getBaseExperienceReward(level)`.
    fn should_drop_experience(&self) -> bool {
        !self.is_baby()
    }
    fn is_always_experience_dropper(&self) -> bool {
        false
    }
    fn get_base_experience_reward(&mut self, level: &ServerLevel) -> i32 {
        let _ = level;
        super::mob::equipment::get_base_experience_reward(self.as_dyn_mut())
    }

    /// `shouldDropLoot(level)`.
    fn should_drop_loot(&self, level: &ServerLevel) -> bool {
        if super::mob::monster::is_monster(self.as_dyn()) {
            return super::mob::monster::should_drop_loot(level);
        }
        !self.is_baby() && level.mob_drops()
    }

    /// `killedEntity(level, victim, source)`: whether the victim's death proceeds (drops).
    fn killed_entity(&mut self, _level: &mut ServerLevel, _victim: &crate::world::damagesource::DamageEntity, _source: &crate::world::damagesource::DamageSource) -> bool {
        true
    }

    /// `awardKillScore(victim, killingBlow)` (`ServerPlayer`: scoreboard + stats).
    fn award_kill_score(&mut self, _level: &mut ServerLevel, _victim: &crate::world::damagesource::DamageEntity, _source: &crate::world::damagesource::DamageSource) {}

    // ==== [Effects] additions (Entity trait methods) ====
    /// `LivingEntity.addEffect(newEffect, source)`.
    fn add_effect(&mut self, level: &mut ServerLevel, effect: super::effect::MobEffectInstance, source: Option<EntityId>) -> bool {
        super::living_entity::effects::add_effect(self.as_dyn_mut(), level, effect, source)
    }
    /// `LivingEntity.forceAddEffect(newEffect, source)`.
    fn force_add_effect(&mut self, level: &mut ServerLevel, effect: super::effect::MobEffectInstance, source: Option<EntityId>) {
        super::living_entity::effects::force_add_effect(self.as_dyn_mut(), level, effect, source)
    }
    /// `LivingEntity.removeEffect(effect)`.
    fn remove_effect(&mut self, level: &mut ServerLevel, effect: mc_data::MobEffect) -> bool {
        super::living_entity::effects::remove_effect(self.as_dyn_mut(), level, effect)
    }
    /// `LivingEntity.removeAllEffects()`.
    fn remove_all_effects(&mut self, level: &mut ServerLevel) -> bool {
        super::living_entity::effects::remove_all_effects(self.as_dyn_mut(), level)
    }
    /// `LivingEntity.hasEffect(effect)`.
    fn has_effect(&self, effect: mc_data::MobEffect) -> bool {
        self.living().is_some_and(|l| l.active_effects.contains_key(&effect))
    }
    /// `LivingEntity.getEffect(effect)`.
    fn get_effect(&self, effect: mc_data::MobEffect) -> Option<&super::effect::MobEffectInstance> {
        self.living()?.active_effects.get(&effect)
    }
    /// `LivingEntity.canBeAffected(newEffect)`.
    fn can_be_affected(&self, level: &ServerLevel, effect: &super::effect::MobEffectInstance) -> bool {
        super::living_entity::effects::can_be_affected(self.as_dyn(), level, effect)
    }
    /// `LivingEntity.isInvertedHealAndHarm()`.
    fn is_inverted_heal_and_harm(&self, level: &ServerLevel) -> bool {
        super::living_entity::effects::is_inverted_heal_and_harm(self.as_dyn(), level)
    }
    /// `LivingEntity.tickEffects()` (server side).
    fn tick_effects(&mut self, level: &mut ServerLevel) {
        super::living_entity::effects::tick_effects(self.as_dyn_mut(), level)
    }
    /// `LivingEntity.onEffectAdded(effect, source)`.
    fn on_effect_added(&mut self, level: &mut ServerLevel, effect: &super::effect::MobEffectInstance, source: Option<EntityId>) {
        super::living_entity::effects::on_effect_added(self.as_dyn_mut(), level, effect, source)
    }
    /// `LivingEntity.onEffectUpdated(effect, doRefreshAttributes, source)`.
    fn on_effect_updated(&mut self, level: &mut ServerLevel, effect: &super::effect::MobEffectInstance, do_refresh_attributes: bool, source: Option<EntityId>) {
        super::living_entity::effects::on_effect_updated(self.as_dyn_mut(), level, effect, do_refresh_attributes, source)
    }
    /// `LivingEntity.onEffectsRemoved(effects)`.
    fn on_effects_removed(&mut self, level: &mut ServerLevel, effects: &[super::effect::MobEffectInstance]) {
        super::living_entity::effects::on_effects_removed(self.as_dyn_mut(), level, effects)
    }
    /// `LivingEntity.onAttributeUpdated(attribute)`.
    fn on_attribute_updated(&mut self, level: &mut ServerLevel, attribute: mc_data::Attribute) {
        super::living_entity::effects::on_attribute_updated(self.as_dyn_mut(), level, attribute)
    }
    /// `LivingEntity.updateInvisibilityStatus()` (`ServerPlayer` overrides it for spectators).
    fn update_invisibility_status(&mut self) {
        super::living_entity::effects::update_invisibility_status(self.as_dyn_mut())
    }
    /// `LivingEntity.updateEffectVisibility()`: the effect-derived flags are recomputed before the next sync.
    fn update_effect_visibility(&mut self) {
        if let Some(l) = self.living_mut() {
            l.effects_dirty = true;
        }
    }
    /// `LivingEntity.getAbsorptionAmount()` (players keep it in synched data).
    fn get_absorption_amount(&self) -> f32 {
        match (self.player(), self.living()) {
            (Some(p), _) => p.get_absorption_amount(),
            (None, Some(l)) => l.absorption_amount,
            _ => 0.0,
        }
    }
    /// `LivingEntity.getMaxAbsorption()`.
    fn get_max_absorption(&self) -> f32 {
        self.living().map_or(0.0, |l| l.attributes.get_value(mc_data::Attribute::MAX_ABSORPTION) as f32)
    }
    /// `LivingEntity.setAbsorptionAmount(amount)`: clamped to `0..=getMaxAbsorption()`.
    fn set_absorption_amount(&mut self, amount: f32) {
        let amount = mc_core::math::mth::clamp_f32(amount, 0.0, self.get_max_absorption());
        // `internalSetAbsorptionAmount`
        if let Some(p) = self.player_mut() {
            p.internal_set_absorption_amount(amount);
        } else if let Some(l) = self.living_mut() {
            l.absorption_amount = amount;
        }
    }
    // ==== [Player] additions (Entity trait methods) ====
    // ==== [Level] additions (Entity trait methods) ====
    // ==== [Items] additions (Entity trait methods) ====
    // ==== [Predicates] additions (Entity trait methods) ====
    /// `SlotProvider.getSlot(slot).get()`: the stack in the numbered slot (`SlotRanges`), `None` when the entity has no
    /// such slot. Default = `LivingEntity`/`Player` slots; mobs with their own inventory override.
    fn get_slot(&self, slot: i32) -> Option<crate::world::item::ItemStack> {
        crate::advancements::predicates::entity::support::get_slot(self.as_dyn(), slot)
    }
    /// `Entity.get(DataComponentType)` as a map (`CUSTOM_NAME`, `CUSTOM_DATA`); subclasses add their components.
    fn entity_components(&self) -> mc_protocol::components::DataComponentMap {
        crate::advancements::predicates::entity::support::base_entity_components(self.base())
    }
    /// `LivingEntity.isBaby()`.
    fn is_baby(&self) -> bool {
        self.mob().is_some_and(|m| m.ageable.is_some()) && super::mob::ageable::is_baby(self.as_dyn())
    }
    /// `Mob.getTarget()` (`None` for non-mobs). SEAM(Mob): overridden by the mob slice.
    fn get_target(&self) -> Option<EntityId> {
        self.mob().and_then(|m| m.target)
    }
    /// `Sheep.isSheared()`; `None` when the entity is not a `Sheep`. SEAM(Sheep).
    fn sheep_sheared(&self) -> Option<bool> {
        None
    }
    /// `AbstractCubeMob.getSize()`; `None` when the entity is not a cube mob. SEAM(AbstractCubeMob).
    fn cube_mob_size(&self) -> Option<i32> {
        None
    }
    /// `(Raider.hasRaid(), Raider.isCaptain())`; `None` when the entity is not a `Raider`. SEAM(Raider).
    fn raider_state(&self) -> Option<(bool, bool)> {
        None
    }
    /// `FishingHook.isOpenWaterFishing()`; `None` when the entity is not a `FishingHook`. SEAM(FishingHook).
    fn fishing_hook_open_water(&self) -> Option<bool> {
        None
    }
    /// `(LightningBolt.getBlocksSetOnFire(), LightningBolt.getHitEntities())`; `None` when the entity is not a
    /// `LightningBolt`. SEAM(LightningBolt): implemented by the explosions/lightning slice.
    fn lightning_bolt_state(&self) -> Option<crate::advancements::predicates::entity::support::LightningBoltState> {
        None
    }
    /// `ServerPlayer.getStats().getValue(statType.get(stat))` for the stat type / stat registry ids (0 for non-players).
    fn get_stat_value(&self, _stat_type: &mc_core::ident::Identifier, _stat: &mc_core::ident::Identifier) -> i32 {
        0
    }
    /// `PlayerAdvancements.getOrStartProgress(advancement)` as data; `None` when the advancement does not exist.
    /// SEAM(advancements): implemented once player advancements are modelled.
    fn advancement_progress(&self, _advancement: &mc_core::ident::Identifier) -> Option<crate::advancements::predicates::entity::support::AdvancementProgressSnapshot> {
        None
    }
    // ==== [Enchant] additions (Entity trait methods) ====

    /// The live stack of `slot` (`getItemBySlot(slot)` by reference), `None` for non-living entities. Enchantment effects
    /// write their durability changes back through it.
    fn item_by_slot_mut(&mut self, slot: super::equipment::EquipmentSlot) -> Option<&mut crate::world::item::ItemStack> {
        self.living_mut().map(|l| l.equipment.get_mut(slot))
    }

    /// `LivingEntity.onEquippedItemBroken(brokenItem, inSlot)`: the break entity event, then the location-based effects of the
    /// broken stack stop (`stopLocationBasedEffects`: its attribute modifiers are removed, its enchantment effects deactivated).
    fn on_equipped_item_broken(&mut self, level: &mut ServerLevel, broken: &crate::world::item::ItemStack, slot: super::equipment::EquipmentSlot) {
        crate::world::item::enchantment::broken_item::on_equipped_item_broken(self.as_dyn_mut(), level, broken, slot);
    }
    // ==== [Loot] additions (Entity trait methods) ====
    /// `this instanceof SlotProvider` (entities whose slots `slot_range` loot sources read).
    fn as_slot_provider(&self) -> Option<&dyn crate::world::item::slot::SlotProvider> {
        None
    }
    // ==== [PathfinderCore] additions (Entity trait methods) ====
    // Mob hooks the pathfinder/navigation call; the defaults are the behavior of an entity that is not a `Mob`
    // (`Mob` types override them: malus map, move control, ...).
    /// `Mob.getPathfindingMalus(type)` (incl. the controlled-vehicle redirect; `level` resolves the vehicle).
    fn get_pathfinding_malus(&self, level: &ServerLevel, ty: crate::world::level::pathfinder::PathType) -> f32 {
        if self.mob().is_some() { super::mob::get_pathfinding_malus(self.as_dyn(), level, ty) } else { ty.get_malus() }
    }
    /// `Mob.setPathfindingMalus(type, cost)`.
    fn set_pathfinding_malus(&mut self, ty: crate::world::level::pathfinder::PathType, cost: f32) {
        super::mob::set_pathfinding_malus(self.as_dyn_mut(), ty, cost);
    }
    /// `Mob.onPathfindingStart()`.
    fn on_pathfinding_start(&mut self) {}
    /// `Mob.onPathfindingDone()`.
    fn on_pathfinding_done(&mut self) {}
    /// `getMaxFallDistance()` (`Entity`: 3; `Mob`/`Creeper` override, reading the target/difficulty).
    fn get_max_fall_distance(&self, level: &ServerLevel) -> i32 {
        if self.mob().is_some() {
            super::mob::tick::get_max_fall_distance(self.as_dyn(), level)
        } else if self.living().is_some() {
            super::mob::tick::comfortable_fall_distance(0.0)
        } else {
            3
        }
    }
    /// `getMoveControl().setWantedPosition(x, y, z, speedModifier)`.
    fn set_wanted_position(&mut self, x: f64, y: f64, z: f64, speed_modifier: f64) {
        super::mob::set_wanted_position(self.as_dyn_mut(), x, y, z, speed_modifier);
    }
    /// `getMoveControl().getSpeedModifier()`.
    fn move_control_speed_modifier(&self) -> f64 {
        super::mob::move_control_speed_modifier(self.as_dyn())
    }
    // ==== [GoalsA] additions (Entity trait methods) ====
    /// `this instanceof AbstractHorse` (the surface the generic horse goals use; implemented by `AbstractHorse` mobs).
    fn as_abstract_horse(&self) -> Option<&dyn crate::world::entity::ai::goal::AbstractHorseAccess> {
        None
    }
    fn as_abstract_horse_mut(&mut self) -> Option<&mut dyn crate::world::entity::ai::goal::AbstractHorseAccess> {
        None
    }
    // ==== [MobSpine] additions (Entity trait methods) ====
    // `Mob` and its generic subclasses. Every default is the vanilla behavior and dispatches on `mob()` (the `MobData`
    // of mob types); concrete mobs override what their Java class overrides and call the `world::entity::mob` function
    // of the layer they extend as their `super` call (see the module docs of `world/entity/mob/mod.rs`).

    /// The `Mob` data of mob types.
    fn mob(&self) -> Option<&super::mob::MobData> {
        None
    }
    fn mob_mut(&mut self) -> Option<&mut super::mob::MobData> {
        None
    }
    // ---- SEAM(SpawnFramework): removed when its hooks land (identical names and signatures) ----
    /// `this instanceof Mob`.
    fn is_mob(&self) -> bool {
        self.mob().is_some()
    }
    /// `Mob.isPersistenceRequired()`.
    fn is_persistence_required(&self) -> bool {
        self.mob().is_some_and(|m| m.persistence_required)
    }
    /// `Mob.requiresCustomPersistence()`.
    fn requires_custom_persistence(&self) -> bool {
        self.mob().is_some() && (self.base().is_passenger() || super::mob::leashable::is_leashed(self.as_dyn()))
    }
    /// `Mob.removeWhenFarAway(dist)`.
    fn remove_when_far_away(&self, _distance_to_closest_player: f64) -> bool {
        !self.mob().is_some_and(|m| m.animal.is_some())
    }
    /// `Mob.checkSpawnRules(level, reason)`.
    fn check_spawn_rules(&self, level: &dyn crate::world::level::server_level_accessor::ServerLevelAccessor, _reason: mc_data::enums::EntitySpawnReason) -> bool {
        match self.mob() {
            Some(m) if m.kind.is_pathfinder() => super::mob::spawn::pathfinder_check_spawn_rules(self.as_dyn(), level),
            _ => true,
        }
    }
    /// `Mob.checkSpawnObstruction(level)`.
    fn check_spawn_obstruction(&self, level: &dyn crate::world::level::server_level_accessor::ServerLevelAccessor) -> bool {
        self.mob().is_none() || super::mob::spawn::check_spawn_obstruction(self.as_dyn(), level)
    }
    /// `Mob.getMaxSpawnClusterSize()`.
    fn get_max_spawn_cluster_size(&self) -> i32 {
        4
    }
    /// `Mob.isMaxGroupSizeReached(groupSize)`.
    fn is_max_group_size_reached(&self, _group_size: i32) -> bool {
        false
    }
    /// `Mob.finalizeSpawn(level, difficulty, reason, groupData)`.
    fn finalize_spawn(
        &mut self,
        level: &mut dyn crate::world::level::server_level_accessor::ServerLevelAccessor,
        difficulty: crate::world::difficulty_instance::DifficultyInstance,
        reason: mc_data::enums::EntitySpawnReason,
        group_data: Option<Box<dyn super::spawn_group_data::SpawnGroupData>>,
    ) -> Option<Box<dyn super::spawn_group_data::SpawnGroupData>> {
        if self.mob().is_some() { super::mob::spawn::finalize_spawn(self.as_dyn_mut(), level, difficulty, reason, group_data) } else { group_data }
    }

    /// `shouldPassengersInheritMalus()`.
    fn should_passengers_inherit_malus(&self) -> bool {
        false
    }

    // ---- sounds / timing ----
    /// `getAmbientSound()`.
    fn get_ambient_sound(&self) -> Option<mc_data::SoundEvent> {
        None
    }
    /// `getAmbientSoundInterval()` (`Animal`: 120).
    fn get_ambient_sound_interval(&self) -> i32 {
        if self.mob().is_some_and(|m| m.animal.is_some()) { 120 } else { 80 }
    }
    /// `playAmbientSound()`.
    fn play_ambient_sound(&mut self, level: &mut ServerLevel) {
        if self.mob().is_some() {
            super::mob::tick::play_ambient_sound(self.as_dyn_mut(), level);
        }
    }
    /// `customServerAiStep(level)` (`Animal`: forgets love while not an adult).
    fn custom_server_ai_step(&mut self, _level: &mut ServerLevel) {
        if self.mob().is_some_and(|m| m.animal.is_some()) {
            super::mob::animal::custom_server_ai_step(self.as_dyn_mut());
        }
    }
    /// `Monster.updateNoActionTime()`.
    fn update_no_action_time(&mut self, level: &ServerLevel) {
        super::mob::tick::monster_update_no_action_time(self.as_dyn_mut(), level);
    }
    fn get_max_head_x_rot(&self) -> i32 {
        40
    }
    fn get_max_head_y_rot(&self) -> i32 {
        75
    }
    fn get_head_rot_speed(&self) -> i32 {
        10
    }

    // ---- equipment ----
    fn can_use_non_melee_weapon(&self, _stack: &crate::world::item::ItemStack) -> bool {
        false
    }
    /// `canPickUpLoot()`.
    fn can_pick_up_loot(&self) -> bool {
        self.mob().is_some_and(|m| m.can_pick_up_loot)
    }
    fn set_can_pick_up_loot(&mut self, value: bool) {
        if let Some(m) = self.mob_mut() {
            m.can_pick_up_loot = value;
        }
    }
    /// `getPickupReach()` (x, y, z).
    fn get_pickup_reach(&self) -> (i32, i32, i32) {
        (1, 0, 1)
    }
    /// `canHoldItem(stack)`.
    fn can_hold_item(&self, _stack: &crate::world::item::ItemStack) -> bool {
        true
    }
    /// `wantsToPickUp(level, stack)`.
    fn wants_to_pick_up(&self, _level: &ServerLevel, stack: &crate::world::item::ItemStack) -> bool {
        self.can_hold_item(stack)
    }
    /// `pickUpItem(level, itemEntity)`.
    fn pick_up_item(&mut self, level: &mut ServerLevel, item: EntityId) {
        super::mob::equipment::pick_up_item(self.as_dyn_mut(), level, item);
    }
    /// `canReplaceEqualItem(new, current)`.
    fn can_replace_equal_item(&self, new: &crate::world::item::ItemStack, current: &crate::world::item::ItemStack) -> bool {
        super::mob::equipment::can_replace_equal_item(new, current)
    }
    /// `getPreferredWeaponType()` (an item tag name).
    fn get_preferred_weapon_type(&self) -> Option<&'static str> {
        None
    }
    /// `sunProtectionSlot()`.
    fn sun_protection_slot(&self) -> super::equipment::EquipmentSlot {
        super::equipment::EquipmentSlot::Head
    }
    /// `canUseSlot(slot)`.
    fn can_use_slot(&self, _slot: super::equipment::EquipmentSlot) -> bool {
        true
    }
    /// `getEquipmentSlotForItem(stack)`.
    fn get_equipment_slot_for_item(&self, stack: &crate::world::item::ItemStack) -> super::equipment::EquipmentSlot {
        super::mob::equipment::get_equipment_slot_for_item(self.as_dyn(), stack)
    }
    /// `isEquippableInSlot(stack, slot)`.
    fn is_equippable_in_slot(&self, stack: &crate::world::item::ItemStack, slot: super::equipment::EquipmentSlot) -> bool {
        super::mob::equipment::is_equippable_in_slot(self.as_dyn(), stack, slot)
    }
    /// `setItemSlot(slot, stack)` (non-player living entities).
    fn set_item_slot(&mut self, level: &mut ServerLevel, slot: super::equipment::EquipmentSlot, stack: crate::world::item::ItemStack) {
        super::mob::equipment::set_item_slot(self.as_dyn_mut(), level, slot, stack);
    }
    /// `setPersistenceRequired()`.
    fn set_persistence_required(&mut self) {
        if let Some(m) = self.mob_mut() {
            m.persistence_required = true;
        }
    }
    /// `populateDefaultEquipmentSlots(random, difficulty)`.
    fn populate_default_equipment_slots(&mut self, random: &mut dyn RandomSource, difficulty: &crate::world::difficulty_instance::DifficultyInstance) {
        super::mob::equipment::populate_default_equipment_slots(self.as_dyn_mut(), random, difficulty);
    }
    /// `populateDefaultEquipmentEnchantments(level, random, difficulty)`.
    fn populate_default_equipment_enchantments(&mut self, registries: &crate::registry::RegistryAccess, random: &mut dyn RandomSource, difficulty: &crate::world::difficulty_instance::DifficultyInstance) {
        super::mob::equipment::populate_default_equipment_enchantments(self.as_dyn_mut(), registries, random, difficulty);
    }
    /// `enchantSpawnedWeapon(level, random, difficulty)`.
    fn enchant_spawned_weapon(&mut self, registries: &crate::registry::RegistryAccess, random: &mut dyn RandomSource, difficulty: &crate::world::difficulty_instance::DifficultyInstance) {
        super::mob::equipment::enchant_spawned_weapon(self.as_dyn_mut(), registries, random, difficulty);
    }
    /// `enchantSpawnedArmor(level, random, slot, difficulty)`.
    fn enchant_spawned_armor(&mut self, registries: &crate::registry::RegistryAccess, random: &mut dyn RandomSource, slot: super::equipment::EquipmentSlot, difficulty: &crate::world::difficulty_instance::DifficultyInstance) {
        super::mob::equipment::enchant_spawned_armor(self.as_dyn_mut(), registries, random, slot, difficulty);
    }
    /// `equip(EquipmentTable)`.
    fn equip(&mut self, level: &mut ServerLevel, table: &super::equipment_table::EquipmentTable) {
        super::mob::equipment::equip(self.as_dyn_mut(), level, table);
    }
    /// `dropPreservedEquipment(level)`.
    fn drop_preserved_equipment(&mut self, level: &mut ServerLevel) {
        super::mob::equipment::drop_preserved_equipment_where(self.as_dyn_mut(), level, |_| true);
    }
    /// `setDropChance(slot, percent)`.
    fn set_drop_chance(&mut self, slot: super::equipment::EquipmentSlot, percent: f32) {
        super::mob::equipment::set_drop_chance(self.as_dyn_mut(), slot, percent);
    }

    // ---- targeting / combat ----
    /// `canAttack(target)` (`LivingEntity`; `Mob`: not a ghast; `TamableAnimal`: not its owner).
    fn can_attack(&self, level: &ServerLevel, target: &dyn Entity) -> bool {
        if self.mob().is_some() && target.get_type() == EntityType::GHAST {
            return false;
        }
        if self.mob().is_some_and(|m| m.tamable.is_some()) && super::mob::tamable::is_owned_by(self.as_dyn(), level, target) {
            return false;
        }
        if target.is_player() && level.difficulty() == mc_world::level_storage::Difficulty::Peaceful {
            return false;
        }
        target.can_be_seen_as_enemy()
    }
    /// `getVisibilityPercent(level, targetingEntity)`.
    fn get_visibility_percent(&self, _level: &ServerLevel, targeting_entity: Option<&dyn Entity>) -> f64 {
        super::mob::ext::get_visibility_percent(self.as_dyn(), targeting_entity)
    }
    /// `Entity.isPanicking()` (`PathfinderMob`).
    fn is_panicking(&self) -> bool {
        self.mob().is_some_and(|m| m.kind.is_pathfinder()) && super::mob::pathfinder::is_panicking(self.as_dyn())
    }
    /// The brain's `IS_PANICKING` memory; `None` = no brain.
    fn is_panicking_by_brain(&self) -> Option<bool> {
        None
    }
    /// `Entity.hasControllingPassenger()`.
    fn has_controlling_passenger(&self) -> bool {
        self.get_controlling_passenger().is_some()
    }
    /// `AbstractBoat.hasEnoughSpaceFor(passenger)`; `None` when this entity is not a boat.
    fn has_enough_space_for(&self, _level: &ServerLevel, _passenger: &dyn Entity) -> Option<bool> {
        None
    }
    /// `PathfinderMob.getWalkTargetValue(pos, level)`.
    fn get_walk_target_value(&self, pos: BlockPos, level: &dyn crate::world::level::LevelReader) -> f32 {
        super::mob::pathfinder::get_walk_target_value(self.as_dyn(), pos, level)
    }

    // ---- interaction ----
    /// `mobInteract(player, hand)`.
    fn mob_interact(&mut self, level: &mut ServerLevel, player: &mut super::ServerPlayer, hand: crate::world::item::InteractionHand) -> crate::world::item::InteractionResult {
        super::mob::interact::mob_interact(self.as_dyn_mut(), level, player, hand)
    }
    /// `onOffspringSpawnedFromEgg(spawner, offspring)`.
    fn on_offspring_spawned_from_egg(&mut self, _level: &mut ServerLevel, _spawner: &mut super::ServerPlayer, _offspring: EntityId) {}
    /// `dropAllLeashConnections(player)`: drops this entity's leash and every leash it holds.
    fn drop_all_leash_connections(&mut self, level: &mut ServerLevel, player: Option<EntityId>) -> bool {
        self.mob().is_some() && super::mob::interact::drop_all_leash_connections(self.as_dyn_mut(), level, player)
    }
    /// `canShearEquipment(player)`.
    fn can_shear_equipment(&self, _player: &super::ServerPlayer) -> bool {
        !self.base().is_vehicle()
    }
    /// `playEatingSound()`.
    fn play_eating_sound(&mut self, _level: &mut ServerLevel) {}

    // ---- leash ----
    /// `canBeLeashed()` (`Mob`: not an `Enemy`).
    fn can_be_leashed(&self) -> bool {
        self.mob().is_some_and(|m| !m.enemy || m.tamable.is_some())
    }
    fn leash_snap_distance(&self) -> f64 {
        12.0
    }
    fn leash_elastic_distance(&self) -> f64 {
        6.0
    }
    /// `onLeashRemoved()`.
    fn on_leash_removed(&mut self) {
        if let Some(m) = self.mob_mut() {
            if m.leash_data.is_none() {
                m.clear_home();
            }
        }
    }
    /// `whenLeashedTo(holder)`.
    fn when_leashed_to(&mut self, level: &mut ServerLevel, holder: EntityId) {
        super::mob::pathfinder::when_leashed_to(self.as_dyn_mut(), level, holder);
    }
    /// `leashTooFarBehaviour()`.
    fn leash_too_far_behaviour(&mut self, level: &mut ServerLevel) {
        super::mob::leashable::mob_leash_too_far_behaviour(self.as_dyn_mut(), level);
    }
    /// `closeRangeLeashBehaviour(holder)`.
    fn close_range_leash_behaviour(&mut self, level: &mut ServerLevel, holder: EntityId) {
        if self.mob().is_some_and(|m| m.kind.is_pathfinder()) {
            super::mob::pathfinder::close_range_leash_behaviour(self.as_dyn_mut(), level, holder);
        }
    }
    /// `onElasticLeashPull()`.
    fn on_elastic_leash_pull(&mut self) {
        self.check_fall_distance_accumulation();
    }
    /// `shouldStayCloseToLeashHolder()`.
    fn should_stay_close_to_leash_holder(&self) -> bool {
        true
    }
    /// `followLeashSpeed()`.
    fn follow_leash_speed(&self) -> f64 {
        1.0
    }
    fn support_quad_leash(&self) -> bool {
        false
    }
    fn support_quad_leash_as_holder(&self) -> bool {
        false
    }
    /// `notifyLeashHolder(leashee)`.
    fn notify_leash_holder(&mut self, _leashee: EntityId) {}
    /// `notifyLeasheeRemoved(leashee)`.
    fn notify_leashee_removed(&mut self, _level: &mut ServerLevel, _leashee: EntityId) {}
    /// `LeashFenceKnotEntity.getPos()` for knots.
    fn leash_knot_pos(&self) -> Option<BlockPos> {
        None
    }

    // ---- ageable / animal / tamable / neutral ----
    /// `canBeABaby()`.
    fn can_be_a_baby(&self) -> bool {
        true
    }
    /// `getBabyStartAge()`.
    fn get_baby_start_age(&self) -> i32 {
        -24000
    }
    /// `ageBoundaryReached()`.
    fn age_boundary_reached(&mut self, level: &mut ServerLevel) {
        super::mob::ageable::age_boundary_reached(self.as_dyn_mut(), level);
    }
    /// `canAgeUp()`.
    fn can_age_up(&self) -> bool {
        self.mob().is_some_and(|m| m.ageable.is_some()) && super::mob::ageable::can_age_up(self.as_dyn())
    }
    /// `Animal.isFood(stack)`.
    fn is_food(&self, _stack: &crate::world::item::ItemStack) -> bool {
        false
    }
    /// `Animal.canFallInLove()`.
    fn can_fall_in_love(&self) -> bool {
        super::mob::animal::can_fall_in_love(self.as_dyn())
    }
    /// `AgeableMob.getBreedOffspring(level, partner)`.
    fn get_breed_offspring(&mut self, _level: &mut ServerLevel, _partner: EntityId) -> Option<Box<dyn Entity>> {
        None
    }
    /// `TamableAnimal.applyTamingSideEffects()`.
    fn apply_taming_side_effects(&mut self) {}
    /// `TamableAnimal.canFlyToOwner()`.
    fn can_fly_to_owner(&self) -> bool {
        false
    }
    /// `NeutralMob.startPersistentAngerTimer()`.
    fn start_persistent_anger_timer(&mut self, _level: &mut ServerLevel) {}
    /// Class-specific part of `ConversionType.convertCommon(from, to)` (zombie door breaking, ...).
    fn conversion_common_extra(&self, _to: &mut dyn Entity) {}
    // ==== [EntityItems] additions (Entity trait methods) ====
    /// `interact(player, hand, location)`: the player (taken out of the level) uses the entity.
    fn interact(
        &mut self,
        level: &mut ServerLevel,
        player: &mut super::ServerPlayer,
        hand: crate::world::item::InteractionHand,
        location: Vec3,
    ) -> crate::world::item::InteractionResult {
        if self.mob().is_some() {
            super::mob::interact::interact(self.as_dyn_mut(), level, player, hand, location)
        } else {
            crate::world::item::InteractionResult::PASS
        }
    }
    /// `skipAttackInteraction(source)`: the attack on this entity is fully handled (`Player.attack` stops).
    fn skip_attack_interaction(&mut self, _level: &mut ServerLevel, _source: &dyn Entity) -> bool {
        false
    }
    /// `getPickResult()`: the stack of a pick-block on the entity (`None`: nothing).
    fn get_pick_result(&self) -> Option<crate::world::item::ItemStack> {
        None
    }
    /// `getDirection()`: `Direction.fromYRot(getYRot())` (hanging entities store theirs).
    fn get_direction(&self) -> mc_core::direction::Direction {
        mc_core::direction::Direction::from_y_rot(f64::from(self.base().y_rot()))
    }
    /// `rotate(rotation)`: the new `yRot` for structure placement.
    fn rotate(&mut self, rotation: mc_core::rotation::Rotation) -> f32 {
        use mc_core::rotation::Rotation;
        let angle = mc_core::math::mth::wrap_degrees_f32(self.base().y_rot());
        match rotation {
            Rotation::Clockwise180 => angle + 180.0,
            Rotation::Counterclockwise90 => angle + 270.0,
            Rotation::Clockwise90 => angle + 90.0,
            Rotation::None => angle,
        }
    }
    /// `mirror(mirror)`.
    fn mirror(&mut self, mirror: mc_core::mirror::Mirror) -> f32 {
        use mc_core::mirror::Mirror;
        let angle = mc_core::math::mth::wrap_degrees_f32(self.base().y_rot());
        match mirror {
            Mirror::FrontBack => -angle,
            Mirror::LeftRight => 180.0 - angle,
            Mirror::None => angle,
        }
    }
    /// `push(Vec3)` with the level at hand (block-attached entities break when pushed; explosions push through this).
    fn push_in_level(&mut self, _level: &mut ServerLevel, impulse: Vec3) {
        self.push_from_explosion(impulse);
    }
    /// Resolves registry references that were read from NBT without registry access (variants): called after `load`.
    fn resolve_registry_refs(&mut self, _registries: &crate::registry::RegistryAccess) {}
    /// `LivingEntity.attackable()`: mobs may target it.
    fn attackable(&self) -> bool {
        true
    }
    /// `getPistonPushReaction()`.
    fn get_piston_push_reaction(&self) -> mc_data::enums::PushReaction {
        mc_data::enums::PushReaction::PushPull
    }
    /// `applyComponentsFromItemStack(stack)` (`applyImplicitComponents`): the item's components that configure a placed entity.
    fn apply_implicit_components(&mut self, stack: &crate::world::item::ItemStack) {
        super::spawn_config::apply_base_implicit_components(self.as_dyn_mut(), stack);
    }
    /// The part of `EntityType.create(level, config, pos, ...)` that only applies to mobs (`yHeadRot`/`yBodyRot` = `yRot`,
    /// `finalizeSpawn`).
    fn finalize_spawn_on_create(&mut self, level: &mut ServerLevel, reason: mc_data::enums::EntitySpawnReason) {
        if !self.is_mob() {
            return;
        }
        let y_rot = self.base().y_rot();
        if let Some(living) = self.living_mut() {
            living.y_head_rot = y_rot;
            living.y_body_rot = y_rot;
        }
        let difficulty = crate::world::level::server_level_accessor::ServerLevelAccessor::get_current_difficulty_at(&*level, self.block_position());
        self.finalize_spawn(level, difficulty, reason, None);
    }
    /// `thunderHit(level, lightningBolt)` (the bolt is the cause of what the entity does).
    fn thunder_hit_by(&mut self, level: &mut ServerLevel, _bolt: EntityId) {
        self.thunder_hit(level);
    }
    /// `removePassenger(passenger)`'s level-dependent part (the passenger list is already updated).
    fn on_passenger_removed(&mut self, _level: &mut ServerLevel, _passenger: EntityId) {}
    /// `this instanceof AbstractMinecart minecart && minecart.isRideable()`.
    fn is_rideable_minecart(&self) -> bool {
        false
    }
    // ==== [Blocks:FireBedCauldron] additions (Entity trait methods) ====
    /// `Monster.isPreventingPlayerRest(level, player)` (read for the monster entity types only; `true` is the `Monster`
    /// default, `ZombifiedPiglin` overrides it with `isAngryAt(player)`).
    fn is_preventing_player_rest(&self, _level: &ServerLevel, _player: &dyn Entity) -> bool {
        true
    }
    // ==== [Blocks:TechnicalA] additions (Entity trait methods) ====
    /// `this instanceof Bee`: the `Bee` methods `BeehiveBlockEntity` / `BeehiveBlock` call (implemented by the bee entity).
    fn as_bee_access(&mut self) -> Option<&mut dyn crate::world::block_entity::BeeAccess> {
        None
    }
    /// `getBrain().setMemory(MemoryModuleType.HEARD_BELL_TIME, gameTime)` (`BellBlockEntity.updateEntities`): entities whose
    /// brain registers the memory (villagers, ...) override this; the memory write is a no-op for brains without it.
    fn set_heard_bell_time(&mut self, _game_time: i64) {}
}

/// `Entity.saveAsPassenger(output)`: `None` when the entity isn't saved (removal reason / non-serializable type).
pub fn save_as_passenger(entity: &dyn Entity, lookup: &EntityLookupFn<'_>) -> Option<CompoundTag> {
    if let Some(r) = entity.base().removal_reason() {
        if !r.should_save() {
            return None;
        }
    }
    let id = entity.get_encode_id()?;
    let mut out = CompoundTag::new();
    out.put_string(TAG_ID, id);
    entity.save_without_id(&mut out, lookup);
    Some(out)
}

/// `Entity.save(output)`: passengers are saved with their vehicle only.
pub fn save(entity: &dyn Entity, lookup: &EntityLookupFn<'_>) -> Option<CompoundTag> {
    if entity.base().is_passenger() { None } else { save_as_passenger(entity, lookup) }
}

fn put_vec3_list(out: &mut CompoundTag, key: &str, v: Vec3) {
    let mut list = ListTag::new();
    list.push(Tag::double(v.x));
    list.push(Tag::double(v.y));
    list.push(Tag::double(v.z));
    out.put(key, list);
}

/// `Entity.saveWithoutId`.
pub fn save_without_id<E: Entity + ?Sized>(entity: &E, out: &mut CompoundTag, lookup: &EntityLookupFn<'_>) {
    let b = entity.base();
    let pos = match b.vehicle.and_then(lookup) {
        Some(vehicle) => Vec3::new(vehicle.position().x, b.position().y, vehicle.position().z),
        None => b.position(),
    };
    put_vec3_list(out, TAG_POS, pos);
    put_vec3_list(out, TAG_MOTION, b.delta_movement());
    let mut rotation = ListTag::new();
    rotation.push(Tag::float(b.y_rot()));
    rotation.push(Tag::float(b.x_rot()));
    out.put(TAG_ROTATION, rotation);
    out.put_double("fall_distance", b.fall_distance);
    out.put_short("Fire", b.remaining_fire_ticks as i16);
    out.put_short("Air", b.get_air_supply() as i16);
    out.put_boolean("OnGround", b.on_ground());
    out.put_boolean("Invulnerable", b.permanently_invulnerable);
    out.put_int("PortalCooldown", b.portal_cooldown);
    if b.invulnerable_time > 0 {
        out.put_int("invulnerable_time", b.invulnerable_time);
    }
    out.put_int_array(TAG_UUID, mc_core::uuid_util::uuid_to_int_array(&b.uuid).to_vec());
    if let Some(name) = b.get_custom_name() {
        // Component codec failures leave the key out like `storeNullable` of a failed encode would not; encodable always here
        if let Ok(tag) = name.to_nbt() {
            out.put("CustomName", tag);
        }
    }
    if b.is_custom_name_visible() {
        out.put_boolean("CustomNameVisible", true);
    }
    if b.is_silent() {
        out.put_boolean("Silent", true);
    }
    if b.is_no_gravity() {
        out.put_boolean("NoGravity", true);
    }
    if b.has_glowing_tag {
        out.put_boolean("Glowing", true);
    }
    let ticks_frozen = b.get_ticks_frozen();
    if ticks_frozen > 0 {
        out.put_int("TicksFrozen", ticks_frozen);
    }
    if b.has_visual_fire {
        out.put_boolean("HasVisualFire", true);
    }
    if !b.tags.is_empty() {
        let mut tags = ListTag::new();
        for t in b.tags.iter_java_order() {
            tags.push(Tag::String(t.to_string()));
        }
        out.put("Tags", tags);
    }
    if !b.custom_data.is_empty() {
        out.put(TAG_DATA, Tag::Compound(b.custom_data.clone()));
    }
    entity.add_additional_save_data(out);
    if !b.passengers.is_empty() {
        let mut passengers = ListTag::new();
        for &pid in &b.passengers {
            if let Some(p) = lookup(pid) {
                if let Some(tag) = save_as_passenger(p, lookup) {
                    passengers.push(Tag::Compound(tag));
                }
            }
        }
        if !passengers.is_empty() {
            out.put(TAG_PASSENGERS, passengers);
        }
    }
}

/// `Vec3.CODEC` (list of 3 numbers); malformed = `None`.
pub fn read_vec3(tag: Option<&Tag>) -> Option<Vec3> {
    let list = tag?.as_list()?;
    if list.len() != 3 {
        return None;
    }
    let n = |i: usize| list.get(i).and_then(|t| t.as_number()).map(|n| n.double_value());
    Some(Vec3::new(n(0)?, n(1)?, n(2)?))
}

/// `Vec2.CODEC` (list of 2 floats).
pub fn read_vec2(tag: Option<&Tag>) -> Option<Vec2> {
    let list = tag?.as_list()?;
    if list.len() != 2 {
        return None;
    }
    let n = |i: usize| list.get(i).and_then(|t| t.as_number()).map(|n| n.float_value());
    Some(Vec2::new(n(0)?, n(1)?))
}

/// `UUIDUtil.CODEC`: int array (or list of ints) with exactly 4 elements.
pub fn read_uuid(tag: Option<&Tag>) -> Option<Uuid> {
    let ints: Vec<i32> = match tag? {
        Tag::IntArray(v) => v.clone(),
        Tag::List(l) => l.iter().map(|t| t.as_number().map(|n| n.int_value())).collect::<Option<_>>()?,
        _ => return None,
    };
    let arr: [i32; 4] = ints.try_into().ok()?;
    Some(mc_core::uuid_util::uuid_from_int_array(arr))
}

/// `Entity.load`.
pub fn load<E: Entity + ?Sized>(entity: &mut E, input: &CompoundTag) -> Result<(), String> {
    let pos = read_vec3(input.get(TAG_POS)).unwrap_or(Vec3::ZERO);
    let motion = read_vec3(input.get(TAG_MOTION)).unwrap_or(Vec3::ZERO);
    let rotation = read_vec2(input.get(TAG_ROTATION)).unwrap_or(Vec2::ZERO);
    {
        let b = entity.base_mut();
        let m = |v: f64| if v.abs() > 10.0 { 0.0 } else { v };
        b.set_delta_movement_xyz(m(motion.x), m(motion.y), m(motion.z));
        b.needs_sync = true;
        b.set_pos_raw(mth::clamp_f64(pos.x, -3.0000512E7, 3.0000512E7), mth::clamp_f64(pos.y, -2.0E7, 2.0E7), mth::clamp_f64(pos.z, -3.0000512E7, 3.0000512E7));
        b.set_y_rot(rotation.x);
        b.set_x_rot(rotation.y);
        b.set_old_pos_and_rot();
    }
    let y_rot = entity.base().y_rot();
    entity.set_y_head_rot(y_rot);
    entity.set_y_body_rot(y_rot);
    {
        let max_air = entity.get_max_air_supply();
        let b = entity.base_mut();
        b.fall_distance = input.get_double_or("fall_distance", 0.0);
        b.remaining_fire_ticks = i32::from(input.get_short_or("Fire", 0));
        b.set_air_supply(input.get_int_or("Air", max_air));
        b.set_on_ground_raw(input.get_boolean_or("OnGround", false));
        b.permanently_invulnerable = input.get_boolean_or("Invulnerable", false);
        b.invulnerable_time = input.get_int_or("invulnerable_time", 0);
        b.portal_cooldown = input.get_int_or("PortalCooldown", 0);
        if let Some(id) = read_uuid(input.get(TAG_UUID)) {
            b.uuid = id;
        }
    }
    let b = entity.base();
    if !b.position().is_finite() {
        return Err("Entity has invalid position".into());
    }
    if !(b.y_rot().is_finite() && b.x_rot().is_finite()) {
        return Err("Entity has invalid rotation".into());
    }
    entity.reapply_position();
    let (yr, xr) = (entity.base().y_rot(), entity.base().x_rot());
    entity.set_rot(yr, xr);
    {
        let b = entity.base_mut();
        let name = input.get("CustomName").and_then(|t| Component::from_nbt(t).ok());
        b.set_custom_name(name);
        b.set_custom_name_visible(input.get_boolean_or("CustomNameVisible", false));
        b.set_silent(input.get_boolean_or("Silent", false));
        b.set_no_gravity(input.get_boolean_or("NoGravity", false));
        b.has_glowing_tag = input.get_boolean_or("Glowing", false);
        let glowing = b.has_glowing_tag;
        b.set_shared_flag(FLAG_GLOWING, glowing);
        b.set_ticks_frozen(input.get_int_or("TicksFrozen", 0));
        b.has_visual_fire = input.get_boolean_or("HasVisualFire", false);
        b.custom_data = input.get_compound(TAG_DATA).cloned().unwrap_or_default();
        b.tags.clear();
        if let Some(list) = input.get_list("Tags") {
            if list.len() <= MAX_ENTITY_TAG_COUNT {
                for t in list.iter() {
                    if let Some(s) = t.as_string() {
                        b.tags.insert(s);
                    }
                }
            }
        }
    }
    entity.read_additional_save_data(input);
    if entity.reposition_entity_after_load() {
        entity.reapply_position();
    }
    Ok(())
}

/// `Entity.computeSpeed()`.
fn compute_speed<E: Entity + ?Sized>(entity: &mut E) {
    let b = entity.base_mut();
    let pos = b.position();
    let last = *b.last_known_position.get_or_insert(pos);
    b.last_known_speed = pos.subtract(last);
    b.last_known_position = Some(pos);
}

/// `Entity.baseTick()`.
pub fn base_tick<E: Entity + ?Sized>(entity: &mut E, level: &mut ServerLevel) {
    compute_speed(entity);
    entity.base_mut().in_block_state = None;
    if let Some(vehicle) = entity.base().vehicle {
        // a vehicle that is gone from the level (removed) cannot be ridden any more
        if level.entity_manager.get_any(vehicle).is_none_or(|v| v.is_removed()) {
            level.stop_riding(entity.as_dyn_mut());
        }
    }
    if entity.base().boarding_cooldown > 0 {
        entity.base_mut().boarding_cooldown -= 1;
    }
    entity.handle_portal(level);
    if entity.can_spawn_sprint_particle() {
        entity.spawn_sprint_particle(level);
    }
    {
        let b = entity.base_mut();
        b.was_in_powder_snow = b.is_in_powder_snow;
        b.is_in_powder_snow = false;
    }
    let eye_in_water = entity.is_eye_in_fluid(super::fluid_interaction::TAG_WATER);
    entity.base_mut().was_eye_in_water = eye_in_water;
    entity.update_fluid_interaction(level);
    entity.update_swimming(level);
    if entity.base().remaining_fire_ticks > 0 {
        if entity.fire_immune() {
            entity.clear_fire();
        } else {
            if entity.base().remaining_fire_ticks % 20 == 0 && !entity.is_in_lava() {
                let source = level.damage_sources().on_fire();
                entity.hurt_server(level, &source, 1.0);
            }
            let t = entity.base().remaining_fire_ticks - 1;
            entity.set_remaining_fire_ticks(t);
        }
    }
    if entity.is_in_lava() {
        entity.base_mut().fall_distance *= 0.5;
    }
    entity.check_below_world(level);
    let burning = entity.base().remaining_fire_ticks > 0;
    entity.set_shared_flag_on_fire(burning);
    entity.base_mut().first_tick = false;
    entity.tick_leash(level);
}

/// `Entity.getInputVector(input, speed, yRot)`.
pub fn get_input_vector(input: Vec3, speed: f32, y_rot: f32) -> Vec3 {
    let length = input.length_sqr();
    if length < 1.0E-7 {
        return Vec3::ZERO;
    }
    let movement = (if length > 1.0 { input.normalize() } else { input }).scale(f64::from(speed));
    let sin = mth::sin(f64::from(y_rot * (std::f64::consts::PI / 180.0) as f32));
    let cos = mth::cos(f64::from(y_rot * (std::f64::consts::PI / 180.0) as f32));
    Vec3::new(movement.x * f64::from(cos) - movement.z * f64::from(sin), movement.y, movement.z * f64::from(cos) + movement.x * f64::from(sin))
}

/// `Entity.MoveFunction` (`Entity::setPos` / `Entity::snapTo`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MoveFunction {
    SetPos,
    SnapTo,
}

impl MoveFunction {
    pub fn apply(self, target: &mut dyn Entity, x: f64, y: f64, z: f64) {
        match self {
            MoveFunction::SetPos => target.set_pos(x, y, z),
            MoveFunction::SnapTo => target.snap_to_pos(x, y, z),
        }
    }
}

/// `Entity.getDefaultPassengerAttachmentPoint(vehicle, passenger, attachments)`.
pub fn default_passenger_attachment_point(vehicle: &dyn Entity, passenger: &dyn Entity, attachments: &EntityAttachments) -> Vec3 {
    let index = vehicle.base().passengers.iter().position(|&p| p == passenger.id()).map_or(-1, |i| i as i32);
    attachments.get_clamped(super::EntityAttachment::Passenger, index, vehicle.base().y_rot())
}

/// `Entity.teleportPassengers()`: every passenger below `entity` is snapped to its riding position.
pub fn teleport_passengers(entity: &mut dyn Entity, level: &mut ServerLevel) {
    for passenger_id in entity.base().passengers.clone() {
        level.with_entity(passenger_id, |level, passenger| {
            entity.position_rider_with(passenger, MoveFunction::SnapTo);
            teleport_passengers(passenger, level);
        });
    }
}

/// `Entity.calculateViewVector(xRot, yRot)`.
pub fn calculate_view_vector(x_rot: f32, y_rot: f32) -> Vec3 {
    let deg = (std::f64::consts::PI / 180.0) as f32;
    let real_x_rot = x_rot * deg;
    let real_y_rot = -y_rot * deg;
    let y_cos = mth::cos(f64::from(real_y_rot));
    let y_sin = mth::sin(f64::from(real_y_rot));
    let x_cos = mth::cos(f64::from(real_x_rot));
    let x_sin = mth::sin(f64::from(real_x_rot));
    Vec3::new(f64::from(y_sin * x_cos), f64::from(-x_sin), f64::from(y_cos * x_cos))
}

/// `Entity.sendBubbleColumnParticles(level, pos)`.
pub fn send_bubble_column_particles(level: &mut ServerLevel, pos: BlockPos) {
    use mc_core::random::RandomSource;
    use mc_protocol::packets::play::ParticleOptions;
    for _ in 0..2 {
        let rx = crate::world::level::LevelAccessor::random(level).next_double();
        let rz = crate::world::level::LevelAccessor::random(level).next_double();
        level.send_particles(ParticleOptions::Simple(mc_data::ParticleType::SPLASH), f64::from(pos.x()) + rx, f64::from(pos.y() + 1), f64::from(pos.z()) + rz, 1, 0.0, 0.0, 0.0, 1.0);
        let rx = crate::world::level::LevelAccessor::random(level).next_double();
        let rz = crate::world::level::LevelAccessor::random(level).next_double();
        level.send_particles(ParticleOptions::Simple(mc_data::ParticleType::BUBBLE), f64::from(pos.x()) + rx, f64::from(pos.y() + 1), f64::from(pos.z()) + rz, 1, 0.0, 0.01, 0.0, 0.2);
    }
}

/// `Entity.causeFallDamage(fallDistance, damageModifier, damageSource)` of the base class: the entity itself takes no
/// damage, the fall is propagated to its passengers. Always `false`.
pub fn cause_fall_damage_base(entity: &mut dyn Entity, level: &mut ServerLevel, fall_distance: f64, damage_modifier: f32, source: &crate::world::damagesource::DamageSource) -> bool {
    if mc_data::tags::get::<EntityType>("fall_damage_immune").is_some_and(|t| t.contains(entity.get_type())) {
        return false;
    }
    propagate_fall_to_passengers(entity, level, fall_distance, damage_modifier, source);
    false
}

/// `Entity.propagateFallToPassengers(...)`.
pub fn propagate_fall_to_passengers(entity: &mut dyn Entity, level: &mut ServerLevel, fall_distance: f64, damage_modifier: f32, source: &crate::world::damagesource::DamageSource) {
    if entity.base().is_vehicle() {
        for passenger in entity.base().passengers.clone() {
            level.with_entity(passenger, |level, p| {
                p.cause_fall_damage(level, fall_distance, damage_modifier, source);
            });
        }
    }
}

/// `Entity.commonTick()`.
pub fn common_tick<E: Entity + ?Sized>(entity: &mut E) {
    let b = entity.base_mut();
    if b.invulnerable_time > 0 {
        b.invulnerable_time -= 1;
    }
    b.set_old_pos_and_rot();
    b.tick_count = b.tick_count.wrapping_add(1);
}

/// Silence unused warnings for items used by sibling modules.
#[allow(dead_code)]
fn _uses(_: EntityAttachment) {}

/// `EquipmentSlot` -> the wire enum (`STREAM_CODEC` writes the slot id).
pub fn protocol_slot(slot: super::equipment::EquipmentSlot) -> mc_protocol::packets::play::EquipmentSlot {
    use super::equipment::EquipmentSlot as S;
    use mc_protocol::packets::play::EquipmentSlot as P;
    match slot {
        S::MainHand => P::MainHand,
        S::OffHand => P::OffHand,
        S::Feet => P::Feet,
        S::Legs => P::Legs,
        S::Chest => P::Chest,
        S::Head => P::Head,
        S::Body => P::Body,
        S::Saddle => P::Saddle,
    }
}

/// `PositionMoveRotation.of(entity)`.
pub fn position_move_rotation_of(e: &dyn Entity) -> mc_protocol::packets::play::PositionMoveRotation {
    let b = e.base();
    mc_protocol::packets::play::PositionMoveRotation { position: b.position(), delta_movement: b.delta_movement(), y_rot: b.y_rot(), x_rot: b.x_rot() }
}

/// `PositionMoveRotation.calculateAbsolute(source, change, relatives)`.
pub fn calculate_absolute(
    source: mc_protocol::packets::play::PositionMoveRotation,
    change: mc_protocol::packets::play::PositionMoveRotation,
    relatives: mc_protocol::packets::play::Relatives,
) -> mc_protocol::packets::play::PositionMoveRotation {
    use mc_protocol::packets::play::Relative as R;
    let off = |r: R, v: f64| if relatives.contains(r) { v } else { 0.0 };
    let position = Vec3::new(off(R::X, source.position.x) + change.position.x, off(R::Y, source.position.y) + change.position.y, off(R::Z, source.position.z) + change.position.z);
    let y_rot = (if relatives.contains(R::YRot) { source.y_rot } else { 0.0 }) + change.y_rot;
    let x_rot = mth::clamp_f32((if relatives.contains(R::XRot) { source.x_rot } else { 0.0 }) + change.x_rot, -90.0, 90.0);
    let mut rotated = source.delta_movement;
    if relatives.contains(R::RotateDelta) {
        let diff_y = source.y_rot - y_rot;
        let diff_x = source.x_rot - x_rot;
        rotated = rotated.x_rot(f64::from(diff_x).to_radians() as f32);
        rotated = rotated.y_rot(f64::from(diff_y).to_radians() as f32);
    }
    let d = |cur: f64, ch: f64, r: R| if relatives.contains(r) { cur + ch } else { ch };
    let delta_movement = Vec3::new(d(rotated.x, change.delta_movement.x, R::DeltaX), d(rotated.y, change.delta_movement.y, R::DeltaY), d(rotated.z, change.delta_movement.z, R::DeltaZ));
    mc_protocol::packets::play::PositionMoveRotation { position, delta_movement, y_rot, x_rot }
}
