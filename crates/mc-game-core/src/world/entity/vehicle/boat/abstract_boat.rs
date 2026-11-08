//! `net.minecraft.world.entity.vehicle.boat.AbstractBoat` and its four leaf classes `Boat`, `ChestBoat`, `Raft`, `ChestRaft`
//! (`AbstractChestBoat` included): status detection, `floatBoat` buoyancy, bubble columns, passenger placement, dismount,
//! pushing/boarding of nearby entities, saving.
//!
//! Server side only: `controlBoat` (client input) is ported as [`AbstractBoat::control_boat`] but is driven by nobody on the server —
//! a player-ridden boat is client authoritative (`isClientAuthoritative`), the server then only mirrors the client's
//! `ServerboundMoveVehiclePacket` and `ServerboundPaddleBoatPacket`. `tickLerp` / the interpolation handler, `animateHurt`,
//! `handleEntityEvent` (bubble wobble) and the particles are client-only and not modelled.
//!
//! Leash data (`Leashable`) is kept as the raw `leash` tag; the leash mechanics belong to the mob code.

use crate::define_accessor;
use crate::server::level::ServerLevel;
use crate::world::container::Container;
use crate::world::damagesource::DamageSource;
use crate::world::entity::dimensions::EntityDimensions;
use crate::world::entity::entity::MoveFunction;
use crate::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use crate::world::entity::living_entity::push::pushable_entities_in;
use crate::world::entity::vehicle::container_entity::{ChestVehicleData, ContainerEntity};
use crate::world::entity::vehicle::vehicle_entity::{self, VehicleEntity};
use crate::world::entity::vehicle::{
    get_collision_horizontal_escape_vector, is_animal, no_collision,
};
use crate::world::entity::{
    Entity, EntityData, EntityId, MovementEmission, MoverType, RemovalReason, ServerPlayer, entity,
};
use crate::world::item::{InteractionHand, InteractionResult, ItemStack};
use crate::world::level::BlockGetter;
use crate::world::level::block::BlockStateExt;
use crate::world::level::collision::BlockGetterExt;
use crate::world::level::material::FluidStateExt;
use linkme::distributed_slice;
use mc_core::math::mth;
use mc_core::phys::shapes::{BooleanOp, Shapes};
use mc_core::phys::{AABB, Vec3};
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::Block;
use mc_data::{EntityType, GameEvent, Item, SoundEvent};
use mc_nbt::{CompoundTag, Tag};
use std::any::Any;

define_accessor!(DATA_ID_PADDLE_LEFT: Boolean = AbstractBoat);
define_accessor!(DATA_ID_PADDLE_RIGHT: Boolean = AbstractBoat);
define_accessor!(DATA_ID_BUBBLE_TIME: Int = AbstractBoat);

pub const PADDLE_LEFT: usize = 0;
pub const PADDLE_RIGHT: usize = 1;
const TIME_TO_EJECT: f32 = 60.0;
/// `(float) (Math.PI / 8)`, `(float) (Math.PI / 4)`, `(float) (Math.PI * 2)`.
const PADDLE_SPEED: f32 = (std::f64::consts::PI / 8.0) as f32;
const QUARTER_PI: f32 = (std::f64::consts::PI / 4.0) as f32;
const TWO_PI: f32 = (std::f64::consts::PI * 2.0) as f32;
pub const BUBBLE_TIME: i32 = 60;
const CONTAINER_SIZE: usize = 27;

/// `AbstractBoat.Status`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Status {
    InWater,
    UnderWater,
    UnderFlowingWater,
    OnLand,
    InAir,
}

/// `Boat` / `Raft` (`rideHeight`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BoatKind {
    /// `dimensions.height() / 3.0F`.
    Boat,
    /// `dimensions.height() * 0.8888889F`.
    Raft,
}

pub struct AbstractBoat {
    base: EntityData,
    kind: BoatKind,
    drop_item: Item,
    /// `AbstractChestBoat`: the 27 slots + loot table.
    chest: Option<ChestVehicleData>,
    paddle_positions: [f32; 2],
    out_of_control_ticks: f32,
    delta_rotation: f32,
    input_left: bool,
    input_right: bool,
    input_up: bool,
    input_down: bool,
    water_level: f64,
    land_friction: f32,
    status: Option<Status>,
    old_status: Option<Status>,
    last_yd: f64,
    is_above_bubble_column: bool,
    bubble_column_direction_is_down: bool,
    /// `leashData` kept verbatim (`leash` tag).
    leash: Option<Tag>,
    /// The player passengers, so that `getFirstPassenger() instanceof Player` needs no level lookup.
    player_passengers: Vec<EntityId>,
}

impl AbstractBoat {
    /// `new Boat/ChestBoat/Raft/ChestRaft(type, level, dropItem)`.
    pub fn new(ty: EntityType, id: EntityId, drop_item: Item, kind: BoatKind, chest: bool) -> Self {
        let mut base = EntityData::new(ty, id, entity::TOTAL_AIR_SUPPLY, |b| {
            vehicle_entity::define_synched_data(b);
            b.define(DATA_ID_PADDLE_LEFT, false);
            b.define(DATA_ID_PADDLE_RIGHT, false);
            b.define(DATA_ID_BUBBLE_TIME, 0);
        });
        base.blocks_building = true;
        Self {
            base,
            kind,
            drop_item,
            chest: chest.then(|| ChestVehicleData::new(id, CONTAINER_SIZE)),
            paddle_positions: [0.0; 2],
            out_of_control_ticks: 0.0,
            delta_rotation: 0.0,
            input_left: false,
            input_right: false,
            input_up: false,
            input_down: false,
            water_level: 0.0,
            land_friction: 0.0,
            status: None,
            old_status: None,
            last_yd: 0.0,
            is_above_bubble_column: false,
            bubble_column_direction_is_down: false,
            leash: None,
            player_passengers: Vec::new(),
        }
    }

    /// `setInitialPos(x, y, z)`.
    pub fn set_initial_pos(&mut self, x: f64, y: f64, z: f64) {
        self.set_pos(x, y, z);
        self.base.xo = x;
        self.base.yo = y;
        self.base.zo = z;
    }

    pub fn get_status_value(&self) -> Option<Status> {
        self.status
    }

    pub fn is_chest_boat(&self) -> bool {
        self.chest.is_some()
    }

    /// `rideHeight(dimensions)`.
    fn ride_height(&self, dimensions: &EntityDimensions) -> f64 {
        match self.kind {
            BoatKind::Boat => f64::from(dimensions.height / 3.0),
            BoatKind::Raft => f64::from(dimensions.height * 0.8888889f32),
        }
    }

    /// `getSinglePassengerXOffset()`.
    fn get_single_passenger_x_offset(&self) -> f32 {
        if self.chest.is_some() { 0.15 } else { 0.0 }
    }

    /// `getMaxPassengers()`.
    pub fn get_max_passengers(&self) -> usize {
        if self.chest.is_some() { 1 } else { 2 }
    }

    /// `getFirstPassenger() instanceof Player`.
    fn first_passenger_is_player(&self) -> bool {
        self.base
            .passengers
            .first()
            .is_some_and(|p| self.player_passengers.contains(p))
    }

    // ---- synched data --------------------------------------------------------------------------------------------

    pub fn set_paddle_state(&mut self, left: bool, right: bool) {
        self.base.entity_data.set(DATA_ID_PADDLE_LEFT, left);
        self.base.entity_data.set(DATA_ID_PADDLE_RIGHT, right);
    }

    /// `getPaddleState(side)`.
    pub fn get_paddle_state(&self, side: usize) -> bool {
        self.base.entity_data.get(if side == 0 {
            DATA_ID_PADDLE_LEFT
        } else {
            DATA_ID_PADDLE_RIGHT
        }) && self.first_passenger_is_player()
    }

    fn set_bubble_time(&mut self, value: i32) {
        self.base.entity_data.set(DATA_ID_BUBBLE_TIME, value);
    }

    pub fn get_bubble_time(&self) -> i32 {
        self.base.entity_data.get(DATA_ID_BUBBLE_TIME)
    }

    /// The `isAboveBubbleColumn` flag.
    pub fn is_above_bubble_column(&self) -> bool {
        self.is_above_bubble_column
    }

    /// `setInput(left, right, up, down)`.
    pub fn set_input(&mut self, left: bool, right: bool, up: bool, down: bool) {
        self.input_left = left;
        self.input_right = right;
        self.input_up = up;
        self.input_down = down;
    }

    // ---- water / ground detection ----------------------------------------------------------------------------------

    /// `getStatus()` (records `waterLevel` / `landFriction`).
    fn get_status(&mut self, level: &ServerLevel) -> Status {
        if let Some(water_status) = self.is_underwater_status(level) {
            self.water_level = self.base.bounding_box().max_y;
            water_status
        } else if self.check_in_water(level) {
            Status::InWater
        } else {
            let friction = self.get_ground_friction(level);
            if friction > 0.0 {
                self.land_friction = friction;
                Status::OnLand
            } else {
                Status::InAir
            }
        }
    }

    /// `getWaterLevelAbove()`.
    pub fn get_water_level_above(&self, level: &ServerLevel) -> f32 {
        let aabb = self.base.bounding_box();
        let min_x = mth::floor_f64(aabb.min_x);
        let max_x = mth::ceil_f64(aabb.max_x);
        let min_y = mth::floor_f64(aabb.max_y);
        let max_y = mth::ceil_f64(aabb.max_y - self.last_yd);
        let min_z = mth::floor_f64(aabb.min_z);
        let max_z = mth::ceil_f64(aabb.max_z);
        'rows: for y in min_y..max_y {
            let mut block_height = 0.0f32;
            for x in min_x..max_x {
                for z in min_z..max_z {
                    let pos = BlockPos::new(x, y, z);
                    let fluid_state = level.get_fluid_state(pos);
                    if fluid_state.is_tag("water") {
                        block_height = block_height.max(fluid_state.get_height(level, pos));
                    }
                    if block_height >= 1.0 {
                        continue 'rows;
                    }
                }
            }
            if block_height < 1.0 {
                return y as f32 + block_height;
            }
        }
        (max_y + 1) as f32
    }

    /// `getGroundFriction()`.
    pub fn get_ground_friction(&self, level: &ServerLevel) -> f32 {
        let bb = self.base.bounding_box();
        let aabb = AABB::new(
            bb.min_x,
            bb.min_y - 0.001,
            bb.min_z,
            bb.max_x,
            bb.min_y,
            bb.max_z,
        );
        let x0 = mth::floor_f64(aabb.min_x) - 1;
        let x1 = mth::ceil_f64(aabb.max_x) + 1;
        let y0 = mth::floor_f64(aabb.min_y) - 1;
        let y1 = mth::ceil_f64(aabb.max_y) + 1;
        let z0 = mth::floor_f64(aabb.min_z) - 1;
        let z1 = mth::ceil_f64(aabb.max_z) + 1;
        let boat_shape = Shapes::create(aabb);
        let mut friction = 0.0f32;
        let mut count = 0i32;
        for x in x0..x1 {
            for z in z0..z1 {
                let edges = (if x != x0 && x != x1 - 1 { 0 } else { 1 })
                    + (if z != z0 && z != z1 - 1 { 0 } else { 1 });
                if edges == 2 {
                    continue;
                }
                for y in y0..y1 {
                    if edges <= 0 || (y != y0 && y != y1 - 1) {
                        let pos = BlockPos::new(x, y, z);
                        let state = level.get_block_state(pos);
                        if state.block() != Block::LILY_PAD
                            && Shapes::join_is_not_empty(
                                &state.get_collision_shape(level, pos).move_(
                                    f64::from(x),
                                    f64::from(y),
                                    f64::from(z),
                                ),
                                &boat_shape,
                                BooleanOp::AND,
                            )
                        {
                            friction += state.block().data().friction;
                            count += 1;
                        }
                    }
                }
            }
        }
        friction / count as f32
    }

    /// `checkInWater()` (records `waterLevel`).
    fn check_in_water(&mut self, level: &ServerLevel) -> bool {
        let bb = self.base.bounding_box();
        let min_x = mth::floor_f64(bb.min_x);
        let max_x = mth::ceil_f64(bb.max_x);
        let min_y = mth::floor_f64(bb.min_y);
        let max_y = mth::ceil_f64(bb.min_y + 0.001);
        let min_z = mth::floor_f64(bb.min_z);
        let max_z = mth::ceil_f64(bb.max_z);
        let mut in_water = false;
        self.water_level = -f64::MAX;
        for x in min_x..max_x {
            for y in min_y..max_y {
                for z in min_z..max_z {
                    let pos = BlockPos::new(x, y, z);
                    let fluid_state = level.get_fluid_state(pos);
                    if fluid_state.is_tag("water") {
                        let height = y as f32 + fluid_state.get_height(level, pos);
                        self.water_level = f64::from(height).max(self.water_level);
                        in_water |= bb.min_y < f64::from(height);
                    }
                }
            }
        }
        in_water
    }

    /// `isUnderwater()` (the private status probe; `UNDER_FLOWING_WATER` as soon as a flowing block is hit).
    fn is_underwater_status(&self, level: &ServerLevel) -> Option<Status> {
        let aabb = self.base.bounding_box();
        let max_y = aabb.max_y + 0.001;
        let x0 = mth::floor_f64(aabb.min_x);
        let x1 = mth::ceil_f64(aabb.max_x);
        let y0 = mth::floor_f64(aabb.max_y);
        let y1 = mth::ceil_f64(max_y);
        let z0 = mth::floor_f64(aabb.min_z);
        let z1 = mth::ceil_f64(aabb.max_z);
        let mut under_water = false;
        for x in x0..x1 {
            for y in y0..y1 {
                for z in z0..z1 {
                    let pos = BlockPos::new(x, y, z);
                    let fluid_state = level.get_fluid_state(pos);
                    if fluid_state.is_tag("water")
                        && max_y < f64::from(y as f32 + fluid_state.get_height(level, pos))
                    {
                        if !fluid_state.is_source() {
                            return Some(Status::UnderFlowingWater);
                        }
                        under_water = true;
                    }
                }
            }
        }
        under_water.then_some(Status::UnderWater)
    }

    // ---- physics ---------------------------------------------------------------------------------------------------

    /// `floatBoat()`.
    fn float_boat(&mut self, level: &ServerLevel) {
        let mut vspeed = -self.get_gravity();
        let mut buoyancy = 0.0f64;
        let mut inv_friction = 0.05f32;
        if self.old_status == Some(Status::InAir)
            && self.status != Some(Status::InAir)
            && self.status != Some(Status::OnLand)
        {
            self.water_level = self.base.get_y_progress(1.0);
            let target_y =
                f64::from(self.get_water_level_above(level) - self.base.get_bb_height()) + 0.101;
            let moved =
                self.base
                    .bounding_box()
                    .move_vec(Vec3::new(0.0, target_y - self.base.y(), 0.0));
            if no_collision(level, self.as_dyn(), &moved) {
                self.set_pos(self.base.x(), target_y, self.base.z());
                let m = self.base.delta_movement();
                self.base.set_delta_movement(m.multiply_xyz(1.0, 0.0, 1.0));
                self.last_yd = 0.0;
            }
            self.status = Some(Status::InWater);
        } else {
            match self.status {
                Some(Status::InWater) => {
                    buoyancy =
                        (self.water_level - self.base.y()) / f64::from(self.base.get_bb_height());
                    inv_friction = 0.9;
                }
                Some(Status::UnderFlowingWater) => {
                    vspeed = -7.0E-4;
                    inv_friction = 0.9;
                }
                Some(Status::UnderWater) => {
                    buoyancy = f64::from(0.01f32);
                    inv_friction = 0.45;
                }
                Some(Status::InAir) => inv_friction = 0.9,
                Some(Status::OnLand) => {
                    inv_friction = self.land_friction;
                    if self.first_passenger_is_player() {
                        self.land_friction /= 2.0;
                    }
                }
                None => {}
            }
            let movement = self.base.delta_movement();
            let f = f64::from(inv_friction);
            self.base
                .set_delta_movement_xyz(movement.x * f, movement.y + vspeed, movement.z * f);
            self.delta_rotation *= inv_friction;
            if buoyancy > 0.0 {
                let dm = self.base.delta_movement();
                self.base.set_delta_movement_xyz(
                    dm.x,
                    (dm.y + buoyancy * (self.get_default_gravity() / 0.65)) * 0.75,
                    dm.z,
                );
            }
        }
    }

    /// `controlBoat()`: the client's rider input step (the server never runs it, see the module docs).
    pub fn control_boat(&mut self) {
        if self.base.is_vehicle() {
            let mut acceleration = 0.0f32;
            if self.input_left {
                self.delta_rotation -= 1.0;
            }
            if self.input_right {
                self.delta_rotation += 1.0;
            }
            if self.input_right != self.input_left && !self.input_up && !self.input_down {
                acceleration += 0.005;
            }
            let y_rot = self.base.y_rot() + self.delta_rotation;
            self.set_y_rot(y_rot);
            if self.input_up {
                acceleration += 0.04;
            }
            if self.input_down {
                acceleration -= 0.005;
            }
            let rad = (std::f64::consts::PI / 180.0) as f32;
            let y_rot = self.base.y_rot();
            let m = self.base.delta_movement();
            self.base.set_delta_movement(m.add(Vec3::new(
                f64::from(mth::sin(f64::from(-y_rot * rad)) * acceleration),
                0.0,
                f64::from(mth::cos(f64::from(y_rot * rad)) * acceleration),
            )));
            self.set_paddle_state(
                self.input_right && !self.input_left || self.input_up,
                self.input_left && !self.input_right || self.input_up,
            );
        }
    }

    /// `handleBubbleColumnEffect(dragDown)`.
    fn handle_bubble_column_effect(&mut self, drag_down: bool) {
        if self.can_simulate_movement() {
            let movement = self.base.delta_movement();
            if drag_down {
                self.base
                    .set_delta_movement(movement.add(Vec3::new(0.0, -0.7, 0.0)));
            } else {
                self.base
                    .set_delta_movement_xyz(movement.x, 0.6, movement.z);
            }
        }
    }

    /// `tickBubbleColumn()` (server part).
    fn tick_bubble_column(&mut self, level: &mut ServerLevel) {
        if !self.is_above_bubble_column {
            self.set_bubble_time(0);
        }
        let mut bubble_time = self.get_bubble_time();
        if bubble_time > 0 {
            bubble_time -= 1;
            self.set_bubble_time(bubble_time);
            let diff = BUBBLE_TIME - bubble_time - 1;
            if diff > 0 && bubble_time == 0 {
                self.set_bubble_time(0);
                if self.bubble_column_direction_is_down {
                    level.eject_passengers(self.as_dyn_mut());
                    level.broadcast_entity_event(self.base.id, 71);
                } else {
                    level.broadcast_entity_event(self.base.id, 72);
                }
                self.handle_bubble_column_effect(self.bubble_column_direction_is_down);
            }
            self.is_above_bubble_column = false;
        }
    }

    /// `getPaddleSound()`.
    fn get_paddle_sound(&mut self, level: &ServerLevel) -> Option<SoundEvent> {
        match self.get_status(level) {
            Status::InWater | Status::UnderWater | Status::UnderFlowingWater => {
                Some(SoundEvent::ENTITY_BOAT_PADDLE_WATER)
            }
            Status::OnLand => Some(SoundEvent::ENTITY_BOAT_PADDLE_LAND),
            Status::InAir => None,
        }
    }

    /// `hasEnoughSpaceFor(entity)`.
    pub fn has_enough_space_for(&self, other: &dyn Entity) -> bool {
        other.base().get_bb_width() < self.base.get_bb_width()
    }

    // ---- passengers ----------------------------------------------------------------------------------------------------

    /// `calculatePassengerBodyYRot(passenger)`.
    fn calculate_passenger_body_y_rot(&self, passenger: &dyn Entity) -> f32 {
        if is_animal(passenger.get_type())
            && self.base.passengers.len() == self.get_max_passengers()
        {
            self.base.y_rot() + if passenger.id() % 2 == 0 { 90.0 } else { 270.0 }
        } else {
            self.base.y_rot()
        }
    }

    /// `clampRotation(passenger)`.
    fn clamp_rotation(&self, passenger: &mut dyn Entity) -> f32 {
        let passenger_body_y_rot = self.calculate_passenger_body_y_rot(passenger.as_dyn());
        passenger.set_y_body_rot(passenger_body_y_rot);
        if vehicle_entity::is_mob(passenger.as_dyn()) {
            passenger.set_y_rot(passenger_body_y_rot);
            clamp_head_rotation_to_body(passenger);
            mth::wrap_degrees_f32(passenger_body_y_rot - passenger.base().y_rot())
        } else {
            let delta = mth::wrap_degrees_f32(passenger.base().y_rot() - passenger_body_y_rot);
            let target_delta = mth::clamp_f32(delta, -105.0, 105.0);
            let y_rot = passenger.base().y_rot() + target_delta - delta;
            passenger.set_y_rot(y_rot);
            target_delta - delta
        }
    }
}

/// `Mob.clampHeadRotationToBody()` (`getMaxHeadYRot()` 75).
fn clamp_head_rotation_to_body(mob: &mut dyn Entity) {
    let limit = 75.0f32;
    let Some(l) = mob.living() else { return };
    let (body, head) = (l.y_body_rot, l.y_head_rot);
    let delta = mth::wrap_degrees_f32(body - head);
    let target_delta = mth::clamp_f32(mth::wrap_degrees_f32(body - head), -limit, limit);
    mob.set_y_head_rot(head + delta - target_delta);
}

impl VehicleEntity for AbstractBoat {
    fn get_drop_item(&self) -> Item {
        self.drop_item
    }

    /// `AbstractChestBoat.destroy(level, source)`: the boat item, then the contents.
    fn destroy(&mut self, level: &mut ServerLevel, source: &DamageSource) {
        vehicle_entity::destroy_item(self, level, self.drop_item);
        if self.chest.is_some() {
            self.chest_vehicle_destroyed(source, level);
        }
    }
}

impl ContainerEntity for AbstractBoat {
    fn chest(&self) -> &ChestVehicleData {
        self.chest.as_ref().expect("chest boat")
    }
    fn chest_mut(&mut self) -> &mut ChestVehicleData {
        self.chest.as_mut().expect("chest boat")
    }
}

impl Entity for AbstractBoat {
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

    fn get_movement_emission(&self) -> MovementEmission {
        MovementEmission::Events
    }

    fn can_collide_with(&self, level: &ServerLevel, other: &dyn Entity) -> bool {
        can_vehicle_collide(self, level, other)
    }

    fn can_be_collided_with(&self, _other: Option<&dyn Entity>) -> bool {
        true
    }

    fn is_pushable(&self, _level: &ServerLevel) -> bool {
        true
    }

    fn is_pickable(&self) -> bool {
        !self.is_removed()
    }

    fn get_default_gravity(&self) -> f64 {
        0.04
    }

    fn get_air_drag(&self) -> f32 {
        1.0
    }

    fn get_dimension_changing_delay(&self, _level: &ServerLevel) -> i32 {
        vehicle_entity::DIMENSION_CHANGING_DELAY
    }

    /// `AbstractBoat.getPassengerAttachmentPoint(passenger, dimensions, scale)`.
    fn get_passenger_attachment_point(
        &self,
        passenger: &dyn Entity,
        dimensions: &EntityDimensions,
        _scale: f32,
    ) -> Vec3 {
        let mut offset = self.get_single_passenger_x_offset();
        if self.base.passengers.len() > 1 {
            let index = self
                .base
                .passengers
                .iter()
                .position(|&p| p == passenger.id());
            offset = if index == Some(0) { 0.2 } else { -0.6 };
            if is_animal(passenger.get_type()) {
                offset += 0.2;
            }
        }
        Vec3::new(0.0, self.ride_height(dimensions), f64::from(offset))
            .y_rot(-self.base.y_rot() * (std::f64::consts::PI / 180.0) as f32)
    }

    /// `AbstractBoat.onAboveBubbleColumn(dragDown, pos)`: the client-only splash sound/particles still draw from the entity random.
    fn on_above_bubble_column(&mut self, level: &mut ServerLevel, drag_down: bool, _pos: BlockPos) {
        self.is_above_bubble_column = true;
        self.bubble_column_direction_is_down = drag_down;
        if self.get_bubble_time() == 0 {
            self.set_bubble_time(BUBBLE_TIME);
        }
        if !self.is_under_water() && self.base.random.next_int_bounded(100) == 0 {
            // `playLocalSound(.., 0.8F + 0.4F * random.nextFloat())` + `addParticle(SPLASH, x + nextFloat(), y + 0.7, z + nextFloat())`
            for _ in 0..3 {
                self.base.random.next_float();
            }
            let controller = self.get_controlling_passenger();
            level.game_event_from_entity(controller, GameEvent::SPLASH, self.base.position());
        }
    }

    /// `AbstractBoat.push(entity)`.
    fn push_entity(&mut self, level: &mut ServerLevel, other: &mut dyn Entity) {
        let other_bb = other.get_bounding_box();
        let own_bb = self.base.bounding_box();
        if other.as_any().is::<AbstractBoat>() {
            if other_bb.min_y < own_bb.max_y {
                entity::push_entity_base(self, &*level, other);
            }
        } else if other_bb.min_y <= own_bb.min_y {
            entity::push_entity_base(self, &*level, other);
        }
    }

    fn tick(&mut self, level: &mut ServerLevel) {
        self.old_status = self.status;
        let status = self.get_status(level);
        self.status = Some(status);
        if status != Status::UnderWater && status != Status::UnderFlowingWater {
            self.out_of_control_ticks = 0.0;
        } else {
            self.out_of_control_ticks += 1.0;
        }
        if self.out_of_control_ticks >= TIME_TO_EJECT {
            level.eject_passengers(self.as_dyn_mut());
        }
        let hurt_time = vehicle_entity::get_hurt_time(&self.base);
        if hurt_time > 0 {
            vehicle_entity::set_hurt_time(&mut self.base, hurt_time - 1);
        }
        let damage = vehicle_entity::get_damage(&self.base);
        if damage > 0.0 {
            vehicle_entity::set_damage(&mut self.base, damage - 1.0);
        }
        self.base_tick(level);
        if self.is_local_instance_authoritative() {
            if !self.first_passenger_is_player() {
                self.set_paddle_state(false, false);
            }
            self.float_boat(level);
            let movement = self.base.delta_movement();
            self.r#move(level, MoverType::SelfMover, movement);
        } else {
            self.base.set_delta_movement(Vec3::ZERO);
            self.delta_rotation = 0.0;
        }
        self.apply_effects_from_blocks(level);
        self.apply_effects_from_blocks(level);
        self.tick_bubble_column(level);

        for i in 0..=1usize {
            if self.get_paddle_state(i) {
                if !self.base.is_silent()
                    && self.paddle_positions[i] % TWO_PI <= QUARTER_PI
                    && (self.paddle_positions[i] + PADDLE_SPEED) % TWO_PI >= QUARTER_PI
                    && let Some(sound) = self.get_paddle_sound(level)
                {
                    let view = self.get_view_vector(1.0);
                    let dx = if i == 1 { -view.z } else { view.z };
                    let dz = if i == 1 { view.x } else { -view.x };
                    let pitch = 0.8 + 0.4 * self.base.random.next_float();
                    let p = self.base.position();
                    level.play_sound_at(
                        None,
                        p.x + dx,
                        p.y,
                        p.z + dz,
                        sound,
                        self.get_sound_source(),
                        1.0,
                        pitch,
                    );
                }
                self.paddle_positions[i] += PADDLE_SPEED;
            } else {
                self.paddle_positions[i] = 0.0;
            }
        }

        let area = self.base.bounding_box().inflate(
            f64::from(0.2f32),
            -f64::from(0.01f32),
            f64::from(0.2f32),
        );
        let entities = pushable_entities_in(level, self.as_dyn(), &area);
        if !entities.is_empty() {
            let add_new_passengers = !self.first_passenger_is_player();
            let self_id = self.base.id;
            for id in entities {
                level.with_entity(id, |level, other| {
                    if other.base().passengers.contains(&self_id) {
                        return;
                    }
                    if add_new_passengers
                        && self.base.passengers.len() < self.get_max_passengers()
                        && !other.base().is_passenger()
                        && self.has_enough_space_for(other.as_dyn())
                        && other.living().is_some()
                        && !mc_data::tags::get::<EntityType>("cannot_be_pushed_onto_boats")
                            .is_some_and(|t| t.contains(other.get_type()))
                    {
                        level.start_riding_held(other, self, false, true);
                    } else {
                        self.push_entity(level, other);
                    }
                });
            }
        }
    }

    /// `AbstractBoat.checkFallDamage(ya, onGround, onState, pos)`.
    fn check_fall_damage(
        &mut self,
        level: &mut ServerLevel,
        ya: f64,
        on_ground: bool,
        _on_state: mc_data::block::BlockState,
        _pos: BlockPos,
    ) {
        self.last_yd = self.base.delta_movement().y;
        if !self.base.is_passenger() {
            if on_ground {
                self.reset_fall_distance();
            } else if !level
                .get_fluid_state(self.base.block_position().below())
                .is_tag("water")
                && ya < 0.0
            {
                self.base.fall_distance -= f64::from(ya as f32);
            }
        }
    }

    fn is_under_water(&self) -> bool {
        matches!(
            self.status,
            Some(Status::UnderWater | Status::UnderFlowingWater)
        )
    }

    fn can_add_passenger(&self, _passenger: &dyn Entity) -> bool {
        self.base.passengers.len() < self.get_max_passengers() && !self.is_eye_in_fluid("water")
    }

    fn get_controlling_passenger(&self) -> Option<EntityId> {
        self.base
            .passengers
            .first()
            .copied()
            .filter(|p| self.player_passengers.contains(p))
    }

    /// `isClientAuthoritative()`: the controlling passenger is a player.
    fn is_client_authoritative(&self) -> bool {
        self.first_passenger_is_player()
    }

    fn add_passenger(&mut self, passenger: &dyn Entity, first_passenger_is_player: bool) {
        let id = passenger.id();
        let is_player = passenger.is_player();
        let passengers = &mut self.base.passengers;
        if passengers.is_empty() || !(is_player && !first_passenger_is_player) {
            passengers.push(id);
        } else {
            passengers.insert(0, id);
        }
        if is_player {
            self.player_passengers.push(id);
        }
    }

    fn remove_passenger(&mut self, passenger: EntityId) {
        self.base.passengers.retain(|&p| p != passenger);
        self.player_passengers.retain(|&p| p != passenger);
    }

    /// `AbstractBoat.positionRider(passenger, moveFunction)`.
    fn position_rider_with(&self, passenger: &mut dyn Entity, move_function: MoveFunction) {
        let position = self.get_passenger_riding_position(passenger.as_dyn());
        let offset = passenger.get_vehicle_attachment_point(self.as_dyn());
        move_function.apply(
            passenger,
            position.x - offset.x,
            position.y - offset.y,
            position.z - offset.z,
        );
        if !mc_data::tags::get::<EntityType>("can_turn_in_boats")
            .is_some_and(|t| t.contains(passenger.get_type()))
        {
            if passenger.is_local_instance_authoritative() {
                let y_rot = passenger.base().y_rot() + self.delta_rotation;
                passenger.set_y_rot(y_rot);
                let head = passenger.get_y_head_rot() + self.delta_rotation;
                passenger.set_y_head_rot(head);
            }
            self.clamp_rotation(passenger);
        }
    }

    fn on_passenger_turned(&mut self, passenger: &mut dyn Entity) {
        let y_delta = self.clamp_rotation(passenger);
        passenger.base_mut().y_rot_o += y_delta;
    }

    /// `AbstractBoat.getDismountLocationForPassenger(passenger)`: the free spot on the escape side (sets the dismount pose), else
    /// the spot above the boat.
    fn get_dismount_location_for_passenger_setting_pose(
        &self,
        level: &ServerLevel,
        passenger: &mut dyn Entity,
    ) -> Vec3 {
        let direction = get_collision_horizontal_escape_vector(
            f64::from(self.base.get_bb_width() * mth::SQRT_OF_TWO),
            f64::from(passenger.base().get_bb_width()),
            passenger.base().y_rot(),
        );
        let target_x = self.base.x() + direction.x;
        let target_z = self.base.z() + direction.z;
        let target_block_pos =
            BlockPos::containing(target_x, self.base.bounding_box().max_y, target_z);
        let below_block_pos = target_block_pos.below();
        if !level.get_fluid_state(below_block_pos).is_tag("water") {
            let mut targets = Vec::new();
            let target_floor = level.get_block_floor_height(target_block_pos);
            if crate::world::entity::dismount_helper::is_block_floor_valid(target_floor) {
                targets.push(Vec3::new(
                    target_x,
                    f64::from(target_block_pos.y()) + target_floor,
                    target_z,
                ));
            }
            let below_floor = level.get_block_floor_height(below_block_pos);
            if crate::world::entity::dismount_helper::is_block_floor_valid(below_floor) {
                targets.push(Vec3::new(
                    target_x,
                    f64::from(below_block_pos.y()) + below_floor,
                    target_z,
                ));
            }
            for &dismount_pose in passenger.get_dismount_poses() {
                for &target in &targets {
                    if crate::world::entity::dismount_helper::can_dismount_to_pose(
                        level,
                        target,
                        passenger.as_dyn(),
                        dismount_pose,
                    ) {
                        passenger.set_pose(dismount_pose);
                        return target;
                    }
                }
            }
        }
        Vec3::new(self.base.x(), self.base.bounding_box().max_y, self.base.z())
    }

    /// `AbstractBoat.modifyPassengerFluidInteractionBox(passengerBox)`.
    fn modify_passenger_fluid_interaction_box(&self, passenger_box: AABB) -> Option<AABB> {
        if self.is_under_water() {
            return Some(passenger_box);
        }
        let boat_box = self.base.bounding_box();
        if boat_box.max_y >= passenger_box.max_y {
            None
        } else {
            let min_y = passenger_box.min_y.max(boat_box.max_y);
            Some(AABB::new(
                passenger_box.min_x,
                min_y,
                passenger_box.min_z,
                passenger_box.max_x,
                passenger_box.max_y,
                passenger_box.max_z,
            ))
        }
    }

    fn hurt_server(&mut self, level: &mut ServerLevel, source: &DamageSource, amount: f32) -> bool {
        vehicle_entity::hurt_server(self, level, source, amount)
    }

    fn ignore_explosion_in_level(
        &self,
        level: &ServerLevel,
        explosion: &dyn crate::world::level::explosion::Explosion,
    ) -> bool {
        vehicle_entity::ignore_explosion(level, explosion)
    }

    fn get_pick_result(&self) -> Option<ItemStack> {
        Some(ItemStack::new(self.drop_item))
    }

    /// `AbstractBoat.interact` (+ `AbstractChestBoat.interact`).
    fn interact(
        &mut self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        _hand: InteractionHand,
        _location: Vec3,
    ) -> InteractionResult {
        // Not implemented: `Entity.interact` leash handling (lead item, shears, `Leashable`)
        let secondary = player.is_shift_key_down();
        let boarded = if secondary || !(self.out_of_control_ticks < TIME_TO_EJECT) {
            false
        } else {
            level.start_riding_held(player, self, false, true)
        };
        if boarded {
            return InteractionResult::SUCCESS;
        }
        if self.chest.is_none() {
            return InteractionResult::PASS;
        }
        if self.can_add_passenger(&*player) && !secondary {
            return InteractionResult::PASS;
        }
        let result = self.interact_with_container_vehicle(level, player);
        if result.consumes_action() {
            level.game_event_from_entity(
                Some(player.id()),
                GameEvent::CONTAINER_OPEN,
                self.base.position(),
            );
            crate::server::level::block_break::anger_nearby_piglins(level, &*player, true);
        }
        result
    }

    /// `AbstractChestBoat.remove(reason)` (contents) and `AbstractBoat.remove(reason)` (leash) run before `setRemoved`.
    fn before_remove(&mut self, level: &mut ServerLevel, reason: RemovalReason) {
        if reason.should_destroy() && self.chest.is_some() {
            self.drop_chest_contents(level);
        }
        // Not implemented: `if (isLeashed()) dropLeash()`
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        if let Some(leash) = &self.leash {
            out.put("leash", leash.clone());
        }
        if let Some(chest) = &self.chest {
            chest.add_chest_vehicle_save_data(out);
        }
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        self.leash = input.get("leash").cloned();
        if let Some(chest) = &mut self.chest {
            chest.read_chest_vehicle_save_data(input);
        }
    }

    fn as_container(&self) -> Option<&dyn Container> {
        self.chest.as_ref().map(|c| c as &dyn Container)
    }
    fn as_container_mut(&mut self) -> Option<&mut dyn Container> {
        self.chest.as_mut().map(|c| c as &mut dyn Container)
    }
    fn chest_vehicle(&self) -> Option<&ChestVehicleData> {
        self.chest.as_ref()
    }
    fn chest_vehicle_mut(&mut self) -> Option<&mut ChestVehicleData> {
        self.chest.as_mut()
    }

    /// `AbstractChestBoat.stopOpen(user)`: `CONTAINER_CLOSE` at the boat (the open event is posted by `interact`).
    fn container_open_close(&mut self, level: &mut ServerLevel, user: EntityId, open: bool) {
        if let Some(c) = self.chest.as_mut() {
            if open {
                c.start_open(user);
            } else {
                c.stop_open(user);
                level.game_event_from_entity(
                    Some(user),
                    GameEvent::CONTAINER_CLOSE,
                    self.base.position(),
                );
            }
        }
    }
}

/// `AbstractBoat.canVehicleCollide(vehicle, entity)`.
pub fn can_vehicle_collide(vehicle: &dyn Entity, level: &ServerLevel, other: &dyn Entity) -> bool {
    (other.can_be_collided_with(Some(vehicle)) || other.is_pushable(level))
        && !vehicle.is_passenger_of_same_vehicle(level, other)
}

/// The `EntityType` -> (drop item, raft, chest) table of `EntityTypes` (`boatFactory/chestBoatFactory/raftFactory/chestRaftFactory`).
pub fn boat_spec(ty: EntityType) -> Option<(Item, BoatKind, bool)> {
    use BoatKind::{Boat, Raft};
    Some(match ty {
        EntityType::OAK_BOAT => (Item::OAK_BOAT, Boat, false),
        EntityType::OAK_CHEST_BOAT => (Item::OAK_CHEST_BOAT, Boat, true),
        EntityType::SPRUCE_BOAT => (Item::SPRUCE_BOAT, Boat, false),
        EntityType::SPRUCE_CHEST_BOAT => (Item::SPRUCE_CHEST_BOAT, Boat, true),
        EntityType::BIRCH_BOAT => (Item::BIRCH_BOAT, Boat, false),
        EntityType::BIRCH_CHEST_BOAT => (Item::BIRCH_CHEST_BOAT, Boat, true),
        EntityType::JUNGLE_BOAT => (Item::JUNGLE_BOAT, Boat, false),
        EntityType::JUNGLE_CHEST_BOAT => (Item::JUNGLE_CHEST_BOAT, Boat, true),
        EntityType::ACACIA_BOAT => (Item::ACACIA_BOAT, Boat, false),
        EntityType::ACACIA_CHEST_BOAT => (Item::ACACIA_CHEST_BOAT, Boat, true),
        EntityType::CHERRY_BOAT => (Item::CHERRY_BOAT, Boat, false),
        EntityType::CHERRY_CHEST_BOAT => (Item::CHERRY_CHEST_BOAT, Boat, true),
        EntityType::DARK_OAK_BOAT => (Item::DARK_OAK_BOAT, Boat, false),
        EntityType::DARK_OAK_CHEST_BOAT => (Item::DARK_OAK_CHEST_BOAT, Boat, true),
        EntityType::PALE_OAK_BOAT => (Item::PALE_OAK_BOAT, Boat, false),
        EntityType::PALE_OAK_CHEST_BOAT => (Item::PALE_OAK_CHEST_BOAT, Boat, true),
        EntityType::MANGROVE_BOAT => (Item::MANGROVE_BOAT, Boat, false),
        EntityType::MANGROVE_CHEST_BOAT => (Item::MANGROVE_CHEST_BOAT, Boat, true),
        EntityType::POPLAR_BOAT => (Item::POPLAR_BOAT, Boat, false),
        EntityType::POPLAR_CHEST_BOAT => (Item::POPLAR_CHEST_BOAT, Boat, true),
        EntityType::BAMBOO_RAFT => (Item::BAMBOO_RAFT, Raft, false),
        EntityType::BAMBOO_CHEST_RAFT => (Item::BAMBOO_CHEST_RAFT, Raft, true),
        _ => return None,
    })
}

fn create_boat(
    ty: EntityType,
    id: EntityId,
    _registries: &crate::registry::RegistryAccess,
) -> Box<dyn Entity> {
    let (item, kind, chest) = boat_spec(ty).expect("boat entity type");
    Box::new(AbstractBoat::new(ty, id, item, kind, chest))
}

macro_rules! register_boat {
    ($($name:ident = $ty:ident),* $(,)?) => {$(
        #[distributed_slice(ENTITY_FACTORIES)]
        static $name: EntityRegistration = EntityRegistration { entity_type: EntityType::$ty, create: create_boat };
    )*};
}

register_boat!(
    OAK_BOAT_FACTORY = OAK_BOAT,
    OAK_CHEST_BOAT_FACTORY = OAK_CHEST_BOAT,
    SPRUCE_BOAT_FACTORY = SPRUCE_BOAT,
    SPRUCE_CHEST_BOAT_FACTORY = SPRUCE_CHEST_BOAT,
    BIRCH_BOAT_FACTORY = BIRCH_BOAT,
    BIRCH_CHEST_BOAT_FACTORY = BIRCH_CHEST_BOAT,
    JUNGLE_BOAT_FACTORY = JUNGLE_BOAT,
    JUNGLE_CHEST_BOAT_FACTORY = JUNGLE_CHEST_BOAT,
    ACACIA_BOAT_FACTORY = ACACIA_BOAT,
    ACACIA_CHEST_BOAT_FACTORY = ACACIA_CHEST_BOAT,
    CHERRY_BOAT_FACTORY = CHERRY_BOAT,
    CHERRY_CHEST_BOAT_FACTORY = CHERRY_CHEST_BOAT,
    DARK_OAK_BOAT_FACTORY = DARK_OAK_BOAT,
    DARK_OAK_CHEST_BOAT_FACTORY = DARK_OAK_CHEST_BOAT,
    PALE_OAK_BOAT_FACTORY = PALE_OAK_BOAT,
    PALE_OAK_CHEST_BOAT_FACTORY = PALE_OAK_CHEST_BOAT,
    MANGROVE_BOAT_FACTORY = MANGROVE_BOAT,
    MANGROVE_CHEST_BOAT_FACTORY = MANGROVE_CHEST_BOAT,
    POPLAR_BOAT_FACTORY = POPLAR_BOAT,
    POPLAR_CHEST_BOAT_FACTORY = POPLAR_CHEST_BOAT,
    BAMBOO_RAFT_FACTORY = BAMBOO_RAFT,
    BAMBOO_CHEST_RAFT_FACTORY = BAMBOO_CHEST_RAFT,
);
