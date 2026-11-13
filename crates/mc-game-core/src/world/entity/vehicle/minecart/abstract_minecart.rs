//! `net.minecraft.world.entity.vehicle.minecart.AbstractMinecart`.
//!
//! Composition like the rest of the entity layer: a concrete cart embeds [`EntityData`] (`base`) and [`MinecartData`]
//! (`minecart`), implements [`AbstractMinecart`] (the overridable vanilla methods, defaults = `AbstractMinecart`) and
//! [`Entity`] (through [`minecart_entity_methods`] for what every cart shares, plus its own `tick` / `interact` / save methods).
//!
//! `AbstractMinecart.tick` does not run `Entity.baseTick`: fire, the water state of the previous tick and the leash stay as they
//! are, `updateFluidInteraction` is called explicitly.

use super::{new_behavior, old_behavior};
use crate::define_accessor;
use crate::server::level::ServerLevel;
use crate::world::entity::dismount_helper;
use crate::world::entity::entity::{EntityData, default_passenger_attachment_point};
use crate::world::entity::factory::{self, EntitySpawnRequest};
use crate::world::entity::movement;
use crate::world::entity::scenario_dump::{SCENARIO_DUMP_EXTRA, ScenarioDumpExtra};
use crate::world::entity::synched_data::SynchedEntityDataBuilder;
use crate::world::entity::vehicle::vehicle_entity::{self, VehicleEntity};
use crate::world::entity::{
    Entity, EntityDimensions, EntityId, MoverType, ServerPlayer, spawn_config,
};
use crate::world::item::ItemStack;
use crate::world::level::BlockGetter;
use crate::world::level::block::BlockStateExt;
use crate::world::level::block::impls::base_rail_block;
use crate::world::level::block::state::{decode_block_state, encode_block_state};
use crate::world::level::collision::BlockGetterExt;
use linkme::distributed_slice;
use mc_core::direction::Axis;
use mc_core::math::java::java_float_to_string;
use mc_core::math::mth;
use mc_core::phys::{AABB, Vec3};
use mc_core::pos::BlockPos;
use mc_data::block::{Block, BlockState};
use mc_data::enums::{EntitySpawnReason, Pose, RailShape};
use mc_data::{EntityType, props};
use mc_nbt::CompoundTag;
use mc_protocol::packets::play::MinecartStep;

define_accessor!(pub DATA_ID_CUSTOM_DISPLAY_BLOCK: OptionalBlockState = AbstractMinecart);
define_accessor!(pub DATA_ID_DISPLAY_OFFSET: Int = AbstractMinecart);

/// `AbstractMinecart.WATER_SLOWDOWN_FACTOR`.
pub const WATER_SLOWDOWN_FACTOR: f32 = 0.95;

/// The scenario dump fields of every cart: `rails flipped vhurt mdir rider`.
#[distributed_slice(SCENARIO_DUMP_EXTRA)]
static MINECART_DUMP: ScenarioDumpExtra = ScenarioDumpExtra {
    order: 30,
    applies: |e| e.as_abstract_minecart().is_some(),
    fields: |e, _level, ctx| {
        let m = e.as_abstract_minecart().expect("applies");
        let b = e.base();
        vec![
            ("rails".into(), u8::from(m.minecart().on_rails).to_string()),
            ("flipped".into(), u8::from(m.minecart().flipped).to_string()),
            (
                "vhurt".into(),
                format!(
                    "{}/{}/{}",
                    vehicle_entity::get_hurt_time(b),
                    vehicle_entity::get_hurt_dir(b),
                    java_float_to_string(vehicle_entity::get_damage(b))
                ),
            ),
            (
                "mdir".into(),
                e.get_motion_direction().get_serialized_name().to_string(),
            ),
            ("rider".into(), ctx.name_opt(b.passengers.first().copied())),
        ]
    },
};

/// The state `AbstractMinecart` keeps besides the entity base.
#[derive(Default)]
pub struct MinecartData {
    pub on_rails: bool,
    pub flipped: bool,
    /// The behaviour the constructor picked: `NewMinecartBehavior` when the level enables `minecart_improvements`
    /// (`useExperimentalMovement`), set when the cart is created for a level or added to one.
    pub experimental: bool,
    /// `NewMinecartBehavior.lerpSteps`: the steps of the movement since `ServerEntity` last sent them.
    pub lerp_steps: Vec<MinecartStep>,
}

/// `defineSynchedData`; `default_display_offset` is the virtual `getDefaultDisplayOffset()`.
pub fn define_synched_data(builder: &mut SynchedEntityDataBuilder, default_display_offset: i32) {
    vehicle_entity::define_synched_data(builder);
    builder.define(DATA_ID_CUSTOM_DISPLAY_BLOCK, None);
    builder.define(DATA_ID_DISPLAY_OFFSET, default_display_offset);
}

/// The `AbstractMinecart(type, level)` constructor: the entity base (with the shared synched data and `blocksBuilding = true`)
/// and the cart state. `define` adds the subclass accessors.
pub fn new_base(
    ty: EntityType,
    id: EntityId,
    default_display_offset: i32,
    define: impl FnOnce(&mut SynchedEntityDataBuilder),
) -> (EntityData, MinecartData) {
    let mut base = EntityData::new(
        ty,
        id,
        crate::world::entity::entity::TOTAL_AIR_SUPPLY,
        |b| {
            define_synched_data(b, default_display_offset);
            define(b);
        },
    );
    base.blocks_building = true;
    (base, MinecartData::default())
}

/// `AbstractMinecart.EXITS`: the two exits of a rail shape as block offsets `[x, y, z]`.
pub fn exits(shape: RailShape) -> ([i32; 3], [i32; 3]) {
    const X_NEG: [i32; 3] = [-1, 0, 0];
    const X_POS: [i32; 3] = [1, 0, 0];
    const Z_NEG: [i32; 3] = [0, 0, -1];
    const Z_POS: [i32; 3] = [0, 0, 1];
    const X_NEG_BELOW: [i32; 3] = [-1, -1, 0];
    const X_POS_BELOW: [i32; 3] = [1, -1, 0];
    const Z_NEG_BELOW: [i32; 3] = [0, -1, -1];
    const Z_POS_BELOW: [i32; 3] = [0, -1, 1];
    match shape {
        RailShape::NorthSouth => (Z_NEG, Z_POS),
        RailShape::EastWest => (X_NEG, X_POS),
        RailShape::AscendingEast => (X_NEG_BELOW, X_POS),
        RailShape::AscendingWest => (X_NEG, X_POS_BELOW),
        RailShape::AscendingNorth => (Z_NEG, Z_POS_BELOW),
        RailShape::AscendingSouth => (Z_NEG_BELOW, Z_POS),
        RailShape::SouthEast => (Z_POS, X_POS),
        RailShape::SouthWest => (Z_POS, X_NEG),
        RailShape::NorthWest => (Z_NEG, X_NEG),
        RailShape::NorthEast => (Z_NEG, X_POS),
    }
}

/// `state.getValue(((BaseRailBlock) state.getBlock()).getShapeProperty())`.
pub fn rail_shape(state: BlockState) -> RailShape {
    state.get(base_rail_block::shape_property(state))
}

/// `getCurrentBlockPosOrRailBelow()`: the experimental behaviour looks slightly below the cart (it rides 0.1 above the rail).
pub fn current_block_pos_or_rail_below<M: AbstractMinecart + ?Sized>(
    m: &M,
    level: &dyn BlockGetter,
) -> BlockPos {
    let base = m.base();
    let xt = mth::floor_f64(base.x());
    let mut yt = mth::floor_f64(base.y());
    let zt = mth::floor_f64(base.z());
    if m.minecart().experimental {
        let y = base.y() - 0.1 - f64::from(1.0E-5f32);
        if level
            .get_block_state(BlockPos::containing(f64::from(xt), y, f64::from(zt)))
            .is_tag("rails")
        {
            yt = mth::floor_f64(y);
        }
    } else if level
        .get_block_state(BlockPos::new(xt, yt - 1, zt))
        .is_tag("rails")
    {
        yt -= 1;
    }
    BlockPos::new(xt, yt, zt)
}

/// `AbstractMinecart.createMinecart(level, x, y, z, type, reason, itemStack, player)`: the cart at `(x, y, z)` with the item's
/// entity data and components applied, not added to the level yet.
pub fn create_minecart(
    level: &ServerLevel,
    x: f64,
    y: f64,
    z: f64,
    ty: EntityType,
    reason: EntitySpawnReason,
    item_stack: &ItemStack,
    player: Option<&ServerPlayer>,
) -> Option<Box<dyn Entity>> {
    let mut entity = factory::create(
        ty,
        level,
        EntitySpawnRequest {
            reason,
            ignore_checks: false,
        },
    )?;
    entity.set_pos(x, y, z);
    {
        let b = entity.base_mut();
        b.xo = x;
        b.yo = y;
        b.zo = z;
    }
    spawn_config::apply_default_stack_config(level, entity.as_mut(), item_stack, player);
    if let Some(cart) = entity.as_abstract_minecart_mut() {
        let experimental = new_behavior::use_experimental_movement(level);
        cart.minecart_mut().experimental = experimental;
        if experimental {
            let current_pos = current_block_pos_or_rail_below(cart, level);
            let current_state = level.get_block_state(current_pos);
            new_behavior::adjust_to_rails(cart, current_pos, current_state, true);
        }
    }
    Some(entity)
}

/// The overridable part of `AbstractMinecart` (and what its subclasses override); defaults are `AbstractMinecart`'s.
pub trait AbstractMinecart: VehicleEntity {
    fn minecart(&self) -> &MinecartData;
    fn minecart_mut(&mut self) -> &mut MinecartData;

    /// `getDefaultDisplayBlockState()`.
    fn get_default_display_block_state(&self) -> BlockState {
        Block::AIR.default_state()
    }
    /// `getDefaultDisplayOffset()`.
    fn get_default_display_offset(&self) -> i32 {
        6
    }
    /// `isRideable()`.
    fn is_rideable(&self) -> bool {
        false
    }
    /// `isFurnace()`.
    fn is_furnace(&self) -> bool {
        false
    }
    /// `getMaxSpeed(level)`.
    fn get_max_speed(&self, level: &ServerLevel) -> f64 {
        behavior_max_speed(self, level)
    }
    /// `activateMinecart(level, x, y, z, state)` (activator rails).
    fn activate_minecart(
        &mut self,
        _level: &mut ServerLevel,
        _x: i32,
        _y: i32,
        _z: i32,
        _state: bool,
    ) {
    }
    /// `applyNaturalSlowdown(movement)`.
    fn apply_natural_slowdown(&mut self, movement: Vec3) -> Vec3 {
        apply_natural_slowdown(self, movement)
    }
}

/// `behavior.getMaxSpeed(level)`: what `AbstractMinecart.getMaxSpeed` returns and its subclasses scale.
pub fn behavior_max_speed<M: AbstractMinecart + ?Sized>(m: &M, level: &ServerLevel) -> f64 {
    if m.minecart().experimental {
        new_behavior::get_max_speed(level, m.is_in_water())
    } else {
        old_behavior::get_max_speed(m.is_in_water())
    }
}

/// `getRedstoneDirection(pos)`: the direction a powered rail pushes a standing cart (away from the solid block at its end).
pub fn get_redstone_direction(level: &ServerLevel, pos: BlockPos) -> Vec3 {
    let state = level.get_block_state(pos);
    if state.is_block(Block::POWERED_RAIL) && state.get(props::POWERED) {
        match rail_shape(state) {
            RailShape::EastWest => {
                if is_redstone_conductor(level, pos.west()) {
                    return Vec3::new(1.0, 0.0, 0.0);
                }
                if is_redstone_conductor(level, pos.east()) {
                    return Vec3::new(-1.0, 0.0, 0.0);
                }
            }
            RailShape::NorthSouth => {
                if is_redstone_conductor(level, pos.north()) {
                    return Vec3::new(0.0, 0.0, 1.0);
                }
                if is_redstone_conductor(level, pos.south()) {
                    return Vec3::new(0.0, 0.0, -1.0);
                }
            }
            _ => {}
        }
    }
    Vec3::ZERO
}

/// `isRedstoneConductor(pos)`.
pub fn is_redstone_conductor(level: &ServerLevel, pos: BlockPos) -> bool {
    level.get_block_state(pos).is_redstone_conductor()
}

/// `AbstractMinecart.applyNaturalSlowdown(movement)`.
pub fn apply_natural_slowdown<M: AbstractMinecart + ?Sized>(m: &M, movement: Vec3) -> Vec3 {
    let slowdown_factor = if m.minecart().experimental {
        new_behavior::get_slowdown_factor(m.base().is_vehicle())
    } else {
        old_behavior::get_slowdown_factor(m.base().is_vehicle())
    };
    let new_movement = movement.multiply_xyz(slowdown_factor, 0.0, slowdown_factor);
    if m.is_in_water() {
        new_movement.scale(f64::from(WATER_SLOWDOWN_FACTOR))
    } else {
        new_movement
    }
}

/// `AbstractMinecart.tick()`.
pub fn tick<M: AbstractMinecart + ?Sized>(m: &mut M, level: &mut ServerLevel) {
    let hurt_time = vehicle_entity::get_hurt_time(m.base());
    if hurt_time > 0 {
        vehicle_entity::set_hurt_time(m.base_mut(), hurt_time - 1);
    }
    let damage = vehicle_entity::get_damage(m.base());
    if damage > 0.0 {
        vehicle_entity::set_damage(m.base_mut(), damage - 1.0);
    }
    m.check_below_world(level);
    crate::world::entity::entity::compute_speed(m);
    m.handle_portal(level);
    if m.minecart().experimental {
        new_behavior::tick(m, level);
    } else {
        old_behavior::tick(m, level);
    }
    m.update_fluid_interaction(level);
    if m.is_in_lava() {
        m.lava_ignite();
        m.lava_hurt(level);
        m.base_mut().fall_distance *= 0.5;
    }
    m.base_mut().first_tick = false;
}

/// `comeOffTrack(level)`.
pub fn come_off_track<M: AbstractMinecart + ?Sized>(m: &mut M, level: &mut ServerLevel) {
    let max_speed = m.get_max_speed(level);
    let movement = m.base().delta_movement();
    m.base_mut().set_delta_movement_xyz(
        mth::clamp_f64(movement.x, -max_speed, max_speed),
        movement.y,
        mth::clamp_f64(movement.z, -max_speed, max_speed),
    );
    if m.base().on_ground() {
        let d = m.base().delta_movement();
        m.base_mut().set_delta_movement(d.scale(0.5));
    }
    let delta = m.base().delta_movement();
    m.r#move(level, MoverType::SelfMover, delta);
    if !m.base().on_ground() {
        let d = m.base().delta_movement();
        let drag = f64::from(m.get_air_drag());
        m.base_mut().set_delta_movement(d.scale(drag));
    }
}

/// `move(moverType, delta)`.
pub fn move_cart<M: AbstractMinecart + ?Sized>(
    m: &mut M,
    level: &mut ServerLevel,
    mover: MoverType,
    delta: Vec3,
) {
    if m.minecart().experimental {
        let to_position = m.base().position().add(delta);
        movement::move_entity(m.as_dyn_mut(), level, mover, delta);
        if new_behavior::push_and_pickup_entities(m, level) {
            let rest = to_position.subtract(m.base().position());
            movement::move_entity(m.as_dyn_mut(), level, mover, rest);
        }
        if mover == MoverType::Piston {
            m.minecart_mut().on_rails = false;
        }
    } else {
        movement::move_entity(m.as_dyn_mut(), level, mover, delta);
        m.apply_effects_from_blocks(level);
    }
}

/// `applyEffectsFromBlocks()`: `Entity`'s under the experimental behaviour, otherwise the effects of the cart's position only.
pub fn apply_effects_from_blocks<M: AbstractMinecart + ?Sized>(m: &mut M, level: &mut ServerLevel) {
    if m.minecart().experimental {
        crate::world::entity::inside_block_effects::apply_effects_from_blocks(
            m.as_dyn_mut(),
            level,
        );
    } else {
        let p = m.base().position();
        m.apply_effects_from_blocks_between(level, p, p);
        m.base_mut().movement_this_tick.clear();
    }
}

/// `push(entity)` (`AbstractMinecart.push`): `m` is the cart, `entity` what overlaps it.
pub fn push<M: AbstractMinecart + ?Sized>(m: &mut M, entity: &mut dyn Entity) {
    if entity.base().no_physics || m.base().no_physics || m.base().passengers.contains(&entity.id())
    {
        return;
    }
    let mut xa = entity.base().x() - m.base().x();
    let mut za = entity.base().z() - m.base().z();
    let mut dd = xa * xa + za * za;
    if dd >= f64::from(1.0E-4f32) {
        dd = dd.sqrt();
        xa /= dd;
        za /= dd;
        let mut pow = 1.0 / dd;
        if pow > 1.0 {
            pow = 1.0;
        }
        xa *= pow;
        za *= pow;
        xa *= f64::from(0.1f32);
        za *= f64::from(0.1f32);
        xa *= 0.5;
        za *= 0.5;
        if let Some(other) = entity.as_abstract_minecart_mut() {
            push_other_minecart(m, other, xa, za);
        } else {
            m.push_xyz(-xa, 0.0, -za);
            entity.push_xyz(xa / 4.0, 0.0, za / 4.0);
        }
    }
}

fn push_other_minecart<M: AbstractMinecart + ?Sized>(
    m: &mut M,
    other: &mut dyn AbstractMinecart,
    xa: f64,
    za: f64,
) {
    let experimental = m.minecart().experimental;
    let (xo, zo) = if experimental {
        (m.base().delta_movement().x, m.base().delta_movement().z)
    } else {
        (
            other.base().x() - m.base().x(),
            other.base().z() - m.base().z(),
        )
    };
    let dir = Vec3::new(xo, 0.0, zo).normalize();
    let rad = (std::f64::consts::PI / 180.0) as f32;
    let y_rot = m.base().y_rot();
    let facing = Vec3::new(
        f64::from(mth::cos(f64::from(y_rot * rad))),
        0.0,
        f64::from(mth::sin(f64::from(y_rot * rad))),
    )
    .normalize();
    let dot = dir.dot(facing).abs();
    if !(dot < f64::from(0.8f32)) || experimental {
        let movement = m.base().delta_movement();
        let entity_movement = other.base().delta_movement();
        if other.is_furnace() && !m.is_furnace() {
            m.base_mut()
                .set_delta_movement(movement.multiply_xyz(0.2, 1.0, 0.2));
            m.push_xyz(entity_movement.x - xa, 0.0, entity_movement.z - za);
            other
                .base_mut()
                .set_delta_movement(entity_movement.multiply_xyz(0.95, 1.0, 0.95));
        } else if !other.is_furnace() && m.is_furnace() {
            other
                .base_mut()
                .set_delta_movement(entity_movement.multiply_xyz(0.2, 1.0, 0.2));
            other.push_xyz(movement.x + xa, 0.0, movement.z + za);
            m.base_mut()
                .set_delta_movement(movement.multiply_xyz(0.95, 1.0, 0.95));
        } else {
            let xdd = (entity_movement.x + movement.x) / 2.0;
            let zdd = (entity_movement.z + movement.z) / 2.0;
            m.base_mut()
                .set_delta_movement(movement.multiply_xyz(0.2, 1.0, 0.2));
            m.push_xyz(xdd - xa, 0.0, zdd - za);
            other
                .base_mut()
                .set_delta_movement(entity_movement.multiply_xyz(0.2, 1.0, 0.2));
            other.push_xyz(xdd + xa, 0.0, zdd + za);
        }
    }
}

/// `getCustomDisplayBlockState()`.
pub fn get_custom_display_block_state<M: AbstractMinecart + ?Sized>(m: &M) -> Option<BlockState> {
    m.base().entity_data.get(DATA_ID_CUSTOM_DISPLAY_BLOCK)
}

/// `getDisplayBlockState()`.
pub fn get_display_block_state<M: AbstractMinecart + ?Sized>(m: &M) -> BlockState {
    get_custom_display_block_state(m).unwrap_or_else(|| m.get_default_display_block_state())
}

/// `getDisplayOffset()`.
pub fn get_display_offset<M: AbstractMinecart + ?Sized>(m: &M) -> i32 {
    m.base().entity_data.get(DATA_ID_DISPLAY_OFFSET)
}

/// `addAdditionalSaveData(output)`.
pub fn add_additional_save_data<M: AbstractMinecart + ?Sized>(m: &M, out: &mut CompoundTag) {
    if let Some(state) = get_custom_display_block_state(m) {
        out.put("DisplayState", encode_block_state(state));
    }
    let display_offset = get_display_offset(m);
    if display_offset != m.get_default_display_offset() {
        out.put_int("DisplayOffset", display_offset);
    }
    out.put_boolean("FlippedRotation", m.minecart().flipped);
    out.put_boolean("HasTicked", m.base().first_tick);
}

/// `readAdditionalSaveData(input)`.
pub fn read_additional_save_data<M: AbstractMinecart + ?Sized>(m: &mut M, input: &CompoundTag) {
    let state = input.get("DisplayState").and_then(decode_block_state);
    m.base_mut()
        .entity_data
        .set(DATA_ID_CUSTOM_DISPLAY_BLOCK, state);
    let default_offset = m.get_default_display_offset();
    m.base_mut().entity_data.set(
        DATA_ID_DISPLAY_OFFSET,
        input.get_int_or("DisplayOffset", default_offset),
    );
    m.minecart_mut().flipped = input.get_boolean_or("FlippedRotation", false);
    m.base_mut().first_tick = input.get_boolean_or("HasTicked", false);
}

/// `getPassengerAttachmentPoint(passenger, dimensions, scale)`.
pub fn get_passenger_attachment_point<M: AbstractMinecart + ?Sized>(
    m: &M,
    passenger: &dyn Entity,
    dimensions: &EntityDimensions,
) -> Vec3 {
    if matches!(
        passenger.get_type(),
        EntityType::VILLAGER | EntityType::WANDERING_TRADER
    ) {
        Vec3::ZERO
    } else {
        default_passenger_attachment_point(m.as_dyn(), passenger, &dimensions.attachments)
    }
}

/// `POSE_DISMOUNT_HEIGHTS`.
fn pose_dismount_heights(pose: Pose) -> &'static [i32] {
    match pose {
        Pose::Swimming => &[0, 1],
        _ => &[0, 1, -1],
    }
}

/// `getDismountLocationForPassenger(passenger)`: the first free spot beside the cart for the passenger's poses (sets the pose),
/// else the spot above it (the pose that fits under the ceiling).
pub fn get_dismount_location_for_passenger<M: AbstractMinecart + ?Sized>(
    m: &M,
    level: &ServerLevel,
    passenger: &mut dyn Entity,
) -> Vec3 {
    let above_vehicle = || {
        let b = m.base();
        Vec3::new(b.x(), b.bounding_box().max_y, b.z())
    };
    let forward = m.get_motion_direction();
    if forward.get_axis() == Axis::Y {
        return above_vehicle();
    }
    let offsets = dismount_helper::offsets_for_direction(forward);
    let vehicle_block_pos = m.base().block_position();
    for &pose in passenger.get_dismount_poses() {
        let passenger_dimensions = passenger.get_dimensions(pose);
        let dismount_area_reach = passenger_dimensions.width.min(1.0) / 2.0;
        for &offset_y in pose_dismount_heights(pose) {
            for offset_xz in &offsets {
                let target = BlockPos::new(
                    vehicle_block_pos.x() + offset_xz[0],
                    vehicle_block_pos.y() + offset_y,
                    vehicle_block_pos.z() + offset_xz[1],
                );
                let block_floor_height = ServerLevel::get_block_floor_height_of(
                    &dismount_helper::non_climbable_shape(level, target),
                    || dismount_helper::non_climbable_shape(level, target.below()),
                );
                if dismount_helper::is_block_floor_valid(block_floor_height) {
                    let reach = f64::from(dismount_area_reach);
                    let dismount_collision_box = AABB::new(
                        -reach,
                        0.0,
                        -reach,
                        reach,
                        f64::from(passenger_dimensions.height),
                        reach,
                    );
                    let location = Vec3::up_from_bottom_center_of(&target, block_floor_height);
                    if dismount_helper::can_dismount_to(
                        level,
                        passenger.as_dyn(),
                        &dismount_collision_box.move_vec(location),
                    ) {
                        passenger.set_pose(pose);
                        return location;
                    }
                }
            }
        }
    }
    let vehicle_top = m.base().bounding_box().max_y;
    let target = BlockPos::new(
        vehicle_block_pos.x(),
        mth::floor_f64(vehicle_top),
        vehicle_block_pos.z(),
    );
    for &pose in passenger.get_dismount_poses() {
        let pose_height = f64::from(passenger.get_dimensions(pose).height);
        let block_coverage_y = mth::ceil_f64(vehicle_top - f64::from(target.y()) + pose_height);
        let ceiling_above_vehicle =
            dismount_helper::find_ceiling_from(target, block_coverage_y, |pos| {
                level.get_block_state(pos).get_collision_shape(level, pos)
            });
        if vehicle_top + pose_height <= ceiling_above_vehicle {
            passenger.set_pose(pose);
            break;
        }
    }
    above_vehicle()
}

/// The `Entity` methods every minecart shares (`AbstractMinecart` overrides); expands inside `impl Entity for X`. The struct needs
/// the fields `base: EntityData` and `minecart: MinecartData`. `tick`, `hurt_server`, `interact`, `get_pick_result` and the save
/// methods are the cart's own.
#[macro_export]
macro_rules! minecart_entity_methods {
    () => {
        fn base(&self) -> &$crate::world::entity::EntityData {
            &self.base
        }
        fn base_mut(&mut self) -> &mut $crate::world::entity::EntityData {
            &mut self.base
        }
        fn as_any(&self) -> &dyn ::std::any::Any {
            self
        }
        fn as_any_mut(&mut self) -> &mut dyn ::std::any::Any {
            self
        }
        fn as_dyn(&self) -> &dyn $crate::world::entity::Entity {
            self
        }
        fn as_dyn_mut(&mut self) -> &mut dyn $crate::world::entity::Entity {
            self
        }
        fn as_abstract_minecart(
            &self,
        ) -> Option<&dyn $crate::world::entity::vehicle::minecart::AbstractMinecart> {
            Some(self)
        }
        fn as_abstract_minecart_mut(
            &mut self,
        ) -> Option<&mut dyn $crate::world::entity::vehicle::minecart::AbstractMinecart> {
            Some(self)
        }
        fn get_movement_emission(&self) -> $crate::world::entity::MovementEmission {
            $crate::world::entity::MovementEmission::Events
        }
        fn can_collide_with(
            &self,
            level: &$crate::server::level::ServerLevel,
            other: &dyn $crate::world::entity::Entity,
        ) -> bool {
            $crate::world::entity::vehicle::boat::abstract_boat::can_vehicle_collide(
                self, level, other,
            )
        }
        fn is_pushable(&self, _level: &$crate::server::level::ServerLevel) -> bool {
            true
        }
        fn is_pickable(&self) -> bool {
            !self.is_removed()
        }
        fn get_default_gravity(&self) -> f64 {
            if self.is_in_water() { 0.005 } else { 0.04 }
        }
        fn get_air_drag(&self) -> f32 {
            0.95
        }
        fn get_motion_direction(&self) -> ::mc_core::direction::Direction {
            if self.minecart.experimental {
                self.get_direction()
            } else {
                $crate::world::entity::vehicle::minecart::old_behavior::get_motion_direction(self)
            }
        }
        fn read_level_dependent_data(&mut self, level: &mut $crate::server::level::ServerLevel) {
            self.minecart.experimental =
                $crate::world::entity::vehicle::minecart::new_behavior::use_experimental_movement(
                    level,
                );
        }
        fn resets_forward_portal_offset(&self) -> bool {
            true
        }
        fn r#move(
            &mut self,
            level: &mut $crate::server::level::ServerLevel,
            mover: $crate::world::entity::MoverType,
            delta: ::mc_core::phys::Vec3,
        ) {
            $crate::world::entity::vehicle::minecart::abstract_minecart::move_cart(
                self, level, mover, delta,
            );
        }
        fn apply_effects_from_blocks(&mut self, level: &mut $crate::server::level::ServerLevel) {
            $crate::world::entity::vehicle::minecart::abstract_minecart::apply_effects_from_blocks(
                self, level,
            );
        }
        fn is_on_rails(&self) -> bool {
            self.minecart.on_rails
        }
        fn get_block_speed_factor(&self, level: &$crate::server::level::ServerLevel) -> f32 {
            use $crate::world::level::BlockGetter as _;
            use $crate::world::level::block::BlockStateExt as _;
            if level
                .get_block_state(self.base.block_position())
                .is_tag("rails")
            {
                1.0
            } else {
                $crate::world::entity::movement::get_block_speed_factor(self, level)
            }
        }
        fn get_known_movement(
            &self,
            level: &$crate::server::level::ServerLevel,
        ) -> ::mc_core::phys::Vec3 {
            let known_movement =
                $crate::world::entity::entity::default_known_movement(self, level);
            if self.minecart.experimental {
                known_movement
            } else {
                $crate::world::entity::vehicle::minecart::old_behavior::get_known_movement(
                    known_movement,
                )
            }
        }
        fn get_passenger_attachment_point(
            &self,
            passenger: &dyn $crate::world::entity::Entity,
            dimensions: &$crate::world::entity::EntityDimensions,
            _scale: f32,
        ) -> ::mc_core::phys::Vec3 {
            $crate::world::entity::vehicle::minecart::abstract_minecart::get_passenger_attachment_point(
                self, passenger, dimensions,
            )
        }
        fn get_dismount_location_for_passenger_setting_pose(
            &self,
            level: &$crate::server::level::ServerLevel,
            passenger: &mut dyn $crate::world::entity::Entity,
        ) -> ::mc_core::phys::Vec3 {
            $crate::world::entity::vehicle::minecart::abstract_minecart::get_dismount_location_for_passenger(
                self, level, passenger,
            )
        }
        fn get_dimension_changing_delay(&self, _level: &$crate::server::level::ServerLevel) -> i32 {
            $crate::world::entity::vehicle::vehicle_entity::DIMENSION_CHANGING_DELAY
        }
        fn ignore_explosion_in_level(
            &self,
            level: &$crate::server::level::ServerLevel,
            explosion: &dyn $crate::world::level::explosion::Explosion,
        ) -> bool {
            $crate::world::entity::vehicle::vehicle_entity::ignore_explosion(level, explosion)
        }
        fn push_entity(
            &mut self,
            _level: &mut $crate::server::level::ServerLevel,
            other: &mut dyn $crate::world::entity::Entity,
        ) {
            $crate::world::entity::vehicle::minecart::abstract_minecart::push(self, other);
        }
    };
}
