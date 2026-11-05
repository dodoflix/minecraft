//! `net.minecraft.world.entity.decoration.painting.Painting`.

use super::block_attached::{self, Attached, BlockAttached, Cause};
use super::hanging::{self, DATA_DIRECTION, Hanging};
use crate::define_accessor;
use crate::registry::keys;
use crate::registry::typed::painting_variant::PaintingVariant;
use crate::registry::RegistryAccess;
use crate::server::level::ServerLevel;
use crate::server::level::server_entity::ServerEntity;
use crate::world::damagesource::DamageSource;
use crate::world::entity::entity::EntityData;
use crate::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use crate::world::entity::{Entity, EntityId, MoverType};
use crate::world::item::ItemStack;
use linkme::distributed_slice;
use mc_core::direction::{Axis, Direction};
use mc_core::ident::Identifier;
use mc_core::phys::{AABB, Vec3};
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::{EntityType, Item, SoundEvent};
use mc_nbt::CompoundTag;
use mc_protocol::codec::Holder;
use mc_protocol::components::CUSTOM_NAME;
use std::any::Any;

define_accessor!(pub DATA_PAINTING_VARIANT_ID: PaintingVariant = Painting);

/// `Painting.DEPTH`.
pub const DEPTH: f32 = 0.0625;

pub struct Painting {
    base: EntityData,
    attached: Attached,
    /// The synched variant (`DATA_PAINTING_VARIANT_ID`), its direct value and, for a registry reference, its key.
    variant: mc_protocol::packets::play::PaintingVariant,
    variant_key: Option<Identifier>,
    /// `variant` read from NBT, resolved against the registry after loading ([`Entity::resolve_registry_refs`]).
    pending_variant: Option<Identifier>,
}

fn protocol_variant(variant: &PaintingVariant) -> mc_protocol::packets::play::PaintingVariant {
    variant.to_protocol()
}

impl Painting {
    /// `Painting(type, level)`: the data default is `VariantUtils.getAny(registryAccess, PAINTING_VARIANT)`.
    pub fn new(ty: EntityType, id: EntityId, registries: &RegistryAccess) -> Self {
        let (holder, key, variant) = any_variant(registries);
        let base = EntityData::new(ty, id, crate::world::entity::entity::TOTAL_AIR_SUPPLY, |b| {
            b.define(DATA_DIRECTION, hanging::DEFAULT_DIRECTION);
            b.define(DATA_PAINTING_VARIANT_ID, holder);
        });
        let mut painting = Self { base, attached: Attached::default(), variant, variant_key: key, pending_variant: None };
        painting.set_pos(0.0, 0.0, 0.0);
        painting
    }

    /// `Painting(level, blockPos)`.
    fn at_pos(id: EntityId, registries: &RegistryAccess, pos: BlockPos) -> Self {
        let mut painting = Self::new(EntityType::PAINTING, id, registries);
        painting.attached.pos = pos;
        painting
    }

    /// `Painting(level, blockPos, direction, variant)`.
    pub fn with_variant(id: EntityId, registries: &RegistryAccess, pos: BlockPos, direction: Direction, variant: &Identifier) -> Option<Self> {
        let mut painting = Self::at_pos(id, registries, pos);
        painting.set_variant_key(registries, variant)?;
        painting.set_direction(direction);
        Some(painting)
    }

    /// `setVariant(holder)` for a registry entry.
    fn set_variant_key(&mut self, registries: &RegistryAccess, key: &Identifier) -> Option<()> {
        let registry = registries.registry(keys::PAINTING_VARIANT)?;
        let id = registry.id_of(key)?;
        let value = registry.get_by_id::<PaintingVariant>(id)?;
        self.set_variant(Holder::Reference(id as i32), Some(key.clone()), protocol_variant(value));
        Some(())
    }

    fn set_variant(&mut self, holder: Holder<mc_protocol::packets::play::PaintingVariant>, key: Option<Identifier>, variant: mc_protocol::packets::play::PaintingVariant) {
        self.variant = variant;
        self.variant_key = key;
        if self.base.entity_data.set(DATA_PAINTING_VARIANT_ID, holder) {
            self.on_synced_data_updated(DATA_PAINTING_VARIANT_ID.id());
        }
    }

    pub fn get_variant(&self) -> &mc_protocol::packets::play::PaintingVariant {
        &self.variant
    }

    /// `Painting.create(level, pos, direction)`.
    pub fn create(level: &ServerLevel, pos: BlockPos, direction: Direction) -> Option<Painting> {
        let registries = &level.registries;
        let mut candidate = Self::at_pos(level.next_entity_id(), registries, pos);
        let registry = registries.registry(keys::PAINTING_VARIANT)?;
        let placeable = Identifier::new("minecraft", "placeable").ok()?;
        let mut potential: Vec<u32> = registry.tag(&placeable).map(<[u32]>::to_vec).unwrap_or_default();
        if potential.is_empty() {
            return None;
        }
        candidate.set_direction(direction);
        potential.retain(|&id| {
            let (Some(value), Some(key)) = (registry.get_by_id::<PaintingVariant>(id), registry.name_of(id)) else { return false };
            candidate.set_variant(Holder::Reference(id as i32), Some(key), protocol_variant(value));
            candidate.survives(level)
        });
        if potential.is_empty() {
            return None;
        }
        let area = |id: u32| registry.get_by_id::<PaintingVariant>(id).map_or(0, PaintingVariant::area);
        let largest = potential.iter().map(|&id| area(id)).max().unwrap_or(0);
        potential.retain(|&id| area(id) >= largest);
        // `Util.getRandomSafe(list, random)`
        if potential.is_empty() {
            return None;
        }
        let selected = potential[candidate.base.random.next_int_bounded(potential.len() as i32) as usize];
        let value = registry.get_by_id::<PaintingVariant>(selected)?;
        candidate.set_variant(Holder::Reference(selected as i32), registry.name_of(selected), protocol_variant(value));
        candidate.set_direction(direction);
        Some(candidate)
    }

    fn offset_for_painting_size(size: i32) -> f64 {
        if size % 2 == 0 { 0.5 } else { 0.0 }
    }
}

/// `VariantUtils.getAny(registryAccess, PAINTING_VARIANT)`: the registry's first entry.
fn any_variant(registries: &RegistryAccess) -> (Holder<mc_protocol::packets::play::PaintingVariant>, Option<Identifier>, mc_protocol::packets::play::PaintingVariant) {
    let registry = registries.registry_or_throw(keys::PAINTING_VARIANT);
    let value = registry.get_by_id::<PaintingVariant>(0).expect("painting variant registry is empty");
    (Holder::Reference(0), registry.name_of(0), protocol_variant(value))
}

impl BlockAttached for Painting {
    fn attached(&self) -> &Attached {
        &self.attached
    }
    fn attached_mut(&mut self) -> &mut Attached {
        &mut self.attached
    }
    fn recalculate_bounding_box(&mut self) {
        hanging::recalculate_bounding_box(self);
    }
    fn survives(&self, level: &ServerLevel) -> bool {
        hanging::survives(self, level)
    }
    /// `dropItem(level, causedBy)`.
    fn drop_item(&mut self, level: &mut ServerLevel, caused_by: Option<&Cause>) {
        if level.entity_drops() {
            self.play_sound(level, SoundEvent::ENTITY_PAINTING_BREAK, 1.0, 1.0);
            if !caused_by.is_some_and(|c| c.is_player && c.instabuild) {
                let mut stack = ItemStack::new(Item::PAINTING);
                if let Some(name) = self.base.get_custom_name() {
                    stack.set(CUSTOM_NAME, name);
                }
                hanging::spawn_at_location(self, level, stack, 0.0);
            }
        }
    }
}

impl Hanging for Painting {
    /// `calculateBoundingBox(pos, direction)`.
    fn calculate_bounding_box(&self, pos: BlockPos, direction: Direction) -> AABB {
        let attached_to_wall = Vec3::at_center_of(&pos).relative(direction, -0.46875);
        let horizontal_offset = Self::offset_for_painting_size(self.variant.width);
        let vertical_offset = Self::offset_for_painting_size(self.variant.height);
        let left = direction.get_counter_clockwise();
        let position = attached_to_wall.relative(left, horizontal_offset).relative(Direction::Up, vertical_offset);
        let axis = direction.get_axis();
        let x_size = if axis == Axis::X { 0.0625 } else { f64::from(self.variant.width) };
        let y_size = f64::from(self.variant.height);
        let z_size = if axis == Axis::Z { 0.0625 } else { f64::from(self.variant.width) };
        AABB::of_size(position, x_size, y_size, z_size)
    }
    fn set_direction(&mut self, direction: Direction) {
        hanging::set_direction(self, direction);
    }
    fn play_placement_sound(&self, level: &mut ServerLevel) {
        self.play_sound(level, SoundEvent::ENTITY_PAINTING_PLACE, 1.0, 1.0);
    }
}

impl Entity for Painting {
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
    fn is_pickable(&self) -> bool {
        true
    }
    fn get_direction(&self) -> Direction {
        self.get_hanging_direction()
    }
    fn tick(&mut self, level: &mut ServerLevel) {
        block_attached::tick(self, level);
    }
    fn set_pos(&mut self, x: f64, y: f64, z: f64) {
        block_attached::set_pos(self, x, y, z);
    }
    fn refresh_dimensions(&mut self) {}
    fn thunder_hit(&mut self, _level: &mut ServerLevel) {}
    fn reposition_entity_after_load(&self) -> bool {
        false
    }
    fn kill_in_level(&mut self, level: &mut ServerLevel) {
        block_attached::kill_self(self, level);
    }
    fn skip_attack_interaction(&mut self, level: &mut ServerLevel, source: &dyn Entity) -> bool {
        block_attached::skip_attack_interaction(self, level, source)
    }
    fn ignore_explosion(&self, explosion: &dyn crate::world::level::explosion::Explosion) -> bool {
        block_attached::ignore_explosion(explosion)
    }
    fn r#move(&mut self, level: &mut ServerLevel, _mover: MoverType, delta: Vec3) {
        block_attached::do_move(self, level, delta);
    }
    fn push_in_level(&mut self, level: &mut ServerLevel, impulse: Vec3) {
        block_attached::do_push(self, level, impulse.x, impulse.y, impulse.z);
    }
    fn rotate(&mut self, rotation: mc_core::rotation::Rotation) -> f32 {
        hanging::rotate(self, rotation)
    }
    fn mirror(&mut self, mirror: mc_core::mirror::Mirror) -> f32 {
        hanging::mirror(self, mirror)
    }
    fn get_pick_result(&self) -> Option<ItemStack> {
        Some(ItemStack::new(Item::PAINTING))
    }
    fn resolve_registry_refs(&mut self, registries: &RegistryAccess) {
        if let Some(key) = self.pending_variant.take() {
            let _ = self.set_variant_key(registries, &key);
        }
    }
    fn on_synced_data_updated(&mut self, accessor_id: u8) {
        if accessor_id == DATA_DIRECTION.id() {
            let direction = self.get_hanging_direction();
            self.set_direction(direction);
        }
        if accessor_id == DATA_PAINTING_VARIANT_ID.id() {
            self.recalculate_bounding_box();
        }
        if accessor_id == crate::world::entity::entity::DATA_POSE.id() {
            self.refresh_dimensions();
        }
    }
    fn hurt_server(&mut self, level: &mut ServerLevel, source: &DamageSource, damage: f32) -> bool {
        block_attached::hurt_server(self, level, source, damage)
    }
    /// `snapTo(x, y, z, yRot, xRot)`: only the position.
    fn snap_to(&mut self, x: f64, y: f64, z: f64, _y_rot: f32, _x_rot: f32) {
        self.set_pos(x, y, z);
    }
    fn tracking_position(&self) -> Vec3 {
        Vec3::at_lower_corner_of(&self.attached.pos)
    }
    fn get_add_entity_packet(&self, server_entity: &ServerEntity) -> mc_protocol::packets::play::ClientboundAddEntityPacket {
        hanging::add_entity_packet(server_entity, self, i32::try_from(self.get_hanging_direction().get_3d_data_value()).unwrap_or(0), self.attached.pos)
    }
    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        out.put_byte("facing", self.get_hanging_direction().get_2d_data_value() as i8);
        block_attached::add_additional_save_data(self, out);
        if let Some(key) = &self.variant_key {
            out.put_string("variant", &key.to_string());
        }
    }
    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        let direction = input.get("facing").and_then(|t| t.as_number()).map_or(Direction::South, |n| Direction::from_2d_data_value(i32::from(n.byte_value())));
        block_attached::read_additional_save_data(self, input);
        self.set_direction(direction);
        self.pending_variant = input.get_string("variant").and_then(Identifier::try_parse);
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static PAINTING_FACTORY: EntityRegistration = EntityRegistration { entity_type: EntityType::PAINTING, create: |ty, id, registries| Box::new(Painting::new(ty, id, registries)) };
