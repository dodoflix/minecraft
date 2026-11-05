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
    fn is_alive(&self) -> bool {
        !self.is_removed()
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
    /// `isStateClimbable(state)` (`BlockTags.CLIMBABLE`).
    fn is_state_climbable(&self, state: mc_data::block::BlockState) -> bool {
        state.block().is_in_tag("climbable")
    }
    /// `isSuppressingBounce()` (`isShiftKeyDown()` for living entities? see `LivingEntity`: false by default).
    fn is_suppressing_bounce(&self) -> bool {
        false
    }
    /// `getEntityBounciness()`.
    fn get_entity_bounciness(&self) -> f64 {
        0.0
    }
    /// `getEffectiveGravity()`.
    fn get_effective_gravity(&self) -> f64 {
        self.get_gravity()
    }
    /// `getGravity()` (`getDefaultGravity()` unless `noGravity`; living entities read the `GRAVITY` attribute).
    fn get_gravity(&self) -> f64 {
        if self.base().is_no_gravity() { 0.0 } else { self.get_default_gravity() }
    }
    fn get_default_gravity(&self) -> f64 {
        0.0
    }
    /// `getAirDrag()`.
    fn get_air_drag(&self) -> f32 {
        0.98
    }
    /// Players do not walk off edges while sneaking (`Player.maybeBackOffFromEdge`: `!flying && isStayingOnGroundSurface`).
    fn stays_on_ground_surface_at_edges(&self) -> bool {
        false
    }

    /// `move(moverType, delta)`; returns the block hooks (`fallOn`, bounce) the level layer still has to dispatch.
    fn r#move(&mut self, level: &ServerLevel, mover: MoverType, delta: Vec3) -> super::movement::MoveOutcome {
        super::movement::move_entity(self.as_dyn_mut(), level, mover, delta)
    }

    /// `setOnGroundWithMovement(onGround, horizontalCollision, movement)`.
    fn set_on_ground_with_movement(&mut self, level: &ServerLevel, on_ground: bool, horizontal_collision: bool, movement: Vec3) {
        super::movement::set_on_ground_with_movement(self.as_dyn_mut(), level, on_ground, horizontal_collision, movement);
    }

    /// `doCheckFallDamage(xa, ya, za, onGround)`; returns the fall distance when the entity hit the ground (the
    /// caller dispatches `Block.fallOn` for the block at `getOnPosLegacy`).
    fn do_check_fall_damage(&mut self, level: &ServerLevel, xa: f64, ya: f64, za: f64, on_ground: bool) -> Option<(BlockPos, f64)> {
        if super::movement::touching_unloaded_chunk(self.as_dyn(), level) {
            return None;
        }
        super::movement::check_supporting_block(self.as_dyn_mut(), level, on_ground, Some(Vec3::new(xa, ya, za)));
        let pos = super::movement::get_on_pos_legacy(self.as_dyn(), level);
        super::movement::check_fall_damage(self.as_dyn_mut(), ya, on_ground).map(|d| (pos, d))
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
        None
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
    fn get_y_head_rot(&self) -> f32 {
        0.0
    }
    fn set_y_head_rot(&mut self, _y_head_rot: f32) {}
    fn set_y_body_rot(&mut self, _y_body_rot: f32) {}

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

    /// `tick()`.
    fn tick(&mut self, level: &mut ServerLevel) {
        self.base_tick(level);
    }

    /// `baseTick()`.
    fn base_tick(&mut self, level: &mut ServerLevel) {
        base_tick(self, level);
    }

    /// The level-dependent part of `baseTick()` (fluid state, fire, lava, fall distance, shared fire flag).
    fn base_tick_world(&mut self, level: &ServerLevel) {
        super::movement::base_tick_world(self.as_dyn_mut(), level);
    }

    /// `getFluidInteractionBox()`.
    fn fluid_interaction_box(&self) -> Option<AABB> {
        Some(self.base().bounding_box().deflate_scalar(0.001))
    }

    /// `updateDataBeforeSync()`.
    fn update_data_before_sync(&mut self) {}

    /// `checkDespawn()`.
    fn check_despawn(&mut self, _level: &mut ServerLevel) {}
    /// `ServerPlayer.stopSleepInBed(forceWakeUp, updateLevelList)` (only players sleep; `ServerPlayer` overrides).
    fn stop_sleep_in_bed(&mut self, _level: &mut ServerLevel, _force_wake_up: bool, _update_level_list: bool) {}
    /// The entity's `DynamicGameEventListener` target, if it listens to game events (vibration listeners).
    fn game_event_listener(&mut self) -> Option<&mut dyn crate::world::level::gameevent::GameEventListener> {
        None
    }

    /// `onBelowWorld()`.
    fn on_below_world(&mut self) {
        self.base_mut().set_removal_reason(RemovalReason::Discarded);
    }

    // ---- fire / air / frozen ----------------------------------------------------------------------------------

    fn fire_immune(&self) -> bool {
        self.base().entity_type.data().fire_immune
    }

    fn get_max_air_supply(&self) -> i32 {
        TOTAL_AIR_SUPPLY
    }

    fn is_on_fire(&self) -> bool {
        self.base().get_shared_flag(FLAG_ONFIRE)
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
    fn is_pushable(&self) -> bool {
        false
    }
    fn is_pickable(&self) -> bool {
        false
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
    fn add_additional_save_data(&self, _out: &mut CompoundTag) {}

    /// `readAdditionalSaveData(input)`.
    fn read_additional_save_data(&mut self, _input: &CompoundTag) {}

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
    // ==== [LivingCore] additions (Entity trait methods) ====
    // ==== [Damage] additions (Entity trait methods) ====

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

    /// `isInvulnerableTo(level, source)` (`LivingEntity`: plus `EnchantmentHelper.isImmuneToDamage`; `Player`/`ServerPlayer` add their rules).
    fn is_invulnerable_to(&self, level: &ServerLevel, source: &crate::world::damagesource::DamageSource) -> bool {
        super::living_entity::damage::is_invulnerable_to(self.as_dyn(), level, source)
    }

    /// `hurtServer(level, source, damage)`: abstract in `Entity`; every living entity runs `LivingEntity.hurtServer`.
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
    // ==== [Effects] additions (Entity trait methods) ====
    // ==== [Player] additions (Entity trait methods) ====
    // ==== [Level] additions (Entity trait methods) ====
    // ==== [Items] additions (Entity trait methods) ====
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

/// `Entity.baseTick()`.
pub fn base_tick<E: Entity + ?Sized>(entity: &mut E, level: &mut ServerLevel) {
    {
        let b = entity.base_mut();
        // computeSpeed
        let pos = b.position();
        let last = b.last_known_position.unwrap_or(pos);
        b.last_known_speed = pos.subtract(last);
        b.last_known_position = Some(pos);
        if b.boarding_cooldown > 0 {
            b.boarding_cooldown -= 1;
        }
    }
    entity.base_tick_world(level);
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
