//! `net.minecraft.world.entity.decoration.HangingEntity`: item frames and paintings. Shared logic of the concrete types
//! (`ItemFrame`, `GlowItemFrame`, `Painting`) lives in free functions over [`Hanging`].

use super::block_attached::BlockAttached;
use crate::define_accessor;
use crate::server::level::ServerLevel;
use crate::world::entity::item::ItemEntity;
use crate::world::entity::movement;
use crate::world::level::block::impls::diode_block::is_diode;
use crate::world::level::collision::CollisionGetter;
use crate::world::item::ItemStack;
use mc_core::direction::Direction;
use mc_core::math::mth;
use mc_core::phys::{AABB, Vec3};
use mc_core::pos::BlockPos;
use mc_core::rotation::Rotation;
use mc_data::EntityType;
use mc_data::block::BlockState;

define_accessor!(pub DATA_DIRECTION: Direction = HangingEntity);

/// `HangingEntity.DEFAULT_DIRECTION`.
pub const DEFAULT_DIRECTION: Direction = Direction::South;

/// `instanceof HangingEntity`.
pub fn is_hanging_type(ty: EntityType) -> bool {
    matches!(ty, EntityType::ITEM_FRAME | EntityType::GLOW_ITEM_FRAME | EntityType::PAINTING)
}

pub trait Hanging: BlockAttached {
    /// `calculateBoundingBox(pos, direction)`.
    fn calculate_bounding_box(&self, pos: BlockPos, direction: Direction) -> AABB;
    /// `getPopBox()`.
    fn get_pop_box(&self) -> AABB {
        self.base().bounding_box()
    }
    /// `setDirection(direction)` (item frames also take vertical directions).
    fn set_direction(&mut self, direction: Direction);
    /// `playPlacementSound()` needs the level.
    fn play_placement_sound(&self, level: &mut ServerLevel);
    fn get_hanging_direction(&self) -> Direction {
        self.base().entity_data.get(DATA_DIRECTION)
    }
}

/// `setDirectionRaw(direction)`.
pub fn set_direction_raw<E: Hanging>(e: &mut E, direction: Direction) {
    e.base_mut().entity_data.set(DATA_DIRECTION, direction);
}

/// `HangingEntity.setDirection(direction)` (horizontal only).
pub fn set_direction<E: Hanging>(e: &mut E, direction: Direction) {
    assert!(direction.get_axis().is_horizontal());
    set_direction_raw(e, direction);
    let b = e.base_mut();
    b.set_y_rot((direction.get_2d_data_value() * 90) as f32);
    b.y_rot_o = b.y_rot();
    e.recalculate_bounding_box();
}

/// `HangingEntity.recalculateBoundingBox()`.
pub fn recalculate_bounding_box<E: Hanging>(e: &mut E) {
    let direction = e.get_hanging_direction();
    let aabb = e.calculate_bounding_box(e.attached().pos, direction);
    let center = aabb.get_center();
    let b = e.base_mut();
    b.set_pos_raw(center.x, center.y, center.z);
    b.set_bounding_box(aabb);
}

/// `HangingEntity.survives()`.
pub fn survives<E: Hanging>(e: &E, level: &ServerLevel) -> bool {
    if has_level_collision(e, level, &e.get_pop_box()) {
        return false;
    }
    let supported = super::all_blocks_in(level, &calculate_support_box(e), is_supporting_block);
    supported && can_coexist(e, level, false)
}

fn is_supporting_block(state: BlockState) -> bool {
    state.is_solid() || is_diode(state)
}

/// `HangingEntity.calculateSupportBox()`.
pub fn calculate_support_box<E: Hanging>(e: &E) -> AABB {
    let step = e.get_hanging_direction().get_step();
    // `getDirection().step()` is a float vector, `mul(-0.5F)` stays in float
    let shift = Vec3::new(f64::from(step.x as f32 * -0.5), f64::from(step.y as f32 * -0.5), f64::from(step.z as f32 * -0.5));
    e.base().bounding_box().move_vec(shift).deflate_scalar(1.0E-7)
}

/// `HangingEntity.canCoexist(allowIntersectingSameType)`.
pub fn can_coexist<E: Hanging>(e: &E, level: &ServerLevel, allow_intersecting_same_type: bool) -> bool {
    let (id, ty, direction) = (e.id(), e.get_type(), e.get_hanging_direction());
    level
        .get_entities_of_class(
            &e.get_pop_box(),
            |other| is_hanging_type(other.get_type()),
            |other| {
                let intersects_same_type = !allow_intersecting_same_type && other.get_type() == ty;
                let is_same_direction = other.base().entity_data.get(DATA_DIRECTION) == direction;
                other.id() != id && (intersects_same_type || is_same_direction)
            },
        )
        .is_empty()
}

/// `HangingEntity.hasLevelCollision(popBox)`.
pub fn has_level_collision<E: Hanging>(e: &E, level: &ServerLevel, pop_box: &AABB) -> bool {
    let context = movement::collision_context(e.as_dyn());
    !level.no_block_collision(Some(&context), pop_box) || !level.no_border_collision(Some(e.id()), pop_box)
}

/// `HangingEntity.spawnAtLocation(level, itemStack, yOffs)`.
pub fn spawn_at_location<E: Hanging>(e: &E, level: &mut ServerLevel, stack: ItemStack, y_offs: f32) {
    let direction = e.get_hanging_direction();
    let p = e.base().position();
    let mut item = ItemEntity::with_stack(
        level,
        p.x + f64::from(direction.get_step_x() as f32 * 0.15),
        p.y + f64::from(y_offs),
        p.z + f64::from(direction.get_step_z() as f32 * 0.15),
        stack,
    );
    item.set_default_pick_up_delay();
    level.add_fresh_entity(Box::new(item));
}

/// `HangingEntity.rotate(rotation)`.
pub fn rotate<E: Hanging>(e: &mut E, rotation: Rotation) -> f32 {
    let mut direction = e.get_hanging_direction();
    if direction.get_axis() != mc_core::direction::Axis::Y {
        direction = match rotation {
            Rotation::Clockwise180 => direction.get_opposite(),
            Rotation::Counterclockwise90 => direction.get_counter_clockwise(),
            Rotation::Clockwise90 => direction.get_clockwise(),
            Rotation::None => direction,
        };
        e.set_direction(direction);
    }
    let angle = mth::wrap_degrees_f32(e.base().y_rot());
    match rotation {
        Rotation::Clockwise180 => angle + 180.0,
        Rotation::Counterclockwise90 => angle + 90.0,
        Rotation::Clockwise90 => angle + 270.0,
        Rotation::None => angle,
    }
}

/// `HangingEntity.mirror(mirror)`.
pub fn mirror<E: Hanging>(e: &mut E, mirror: mc_core::mirror::Mirror) -> f32 {
    let rotation = mirror.get_rotation(e.get_hanging_direction());
    rotate(e, rotation)
}

/// `new ClientboundAddEntityPacket(entity, data, pos)`: the block position as the entity position, the entity's current
/// rotation/movement (not the last sent ones).
pub fn add_entity_packet(
    _server_entity: &crate::server::level::ServerEntity,
    e: &dyn crate::world::entity::Entity,
    data: i32,
    pos: BlockPos,
) -> mc_protocol::packets::play::ClientboundAddEntityPacket {
    mc_protocol::packets::play::ClientboundAddEntityPacket {
        id: e.id(),
        uuid: e.uuid(),
        r#type: e.get_type(),
        x: f64::from(pos.x()),
        y: f64::from(pos.y()),
        z: f64::from(pos.z()),
        movement: mc_protocol::codec::LpVec3(e.base().delta_movement()),
        x_rot: mth::pack_degrees(e.base().x_rot()),
        y_rot: mth::pack_degrees(e.base().y_rot()),
        y_head_rot: mth::pack_degrees(e.get_y_head_rot()),
        data,
    }
}
