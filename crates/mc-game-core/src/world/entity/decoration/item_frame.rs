//! `net.minecraft.world.entity.decoration.ItemFrame` and `GlowItemFrame` (one type, the glow variant is chosen by the
//! entity type).

use super::block_attached::{self, Attached, BlockAttached, Cause};
use super::hanging::{self, DATA_DIRECTION, Hanging};
use crate::define_accessor;
use crate::server::level::ServerLevel;
use crate::server::level::server_entity::ServerEntity;
use crate::world::damagesource::{DamageSource, DamageTypeTags};
use crate::world::entity::entity::{EntityData, FLAG_INVISIBLE};
use crate::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use crate::world::entity::{Entity, EntityId, MoverType, ServerPlayer};
use crate::world::item::{InteractionHand, InteractionResult, ItemStack, ItemStackExt};
use crate::world::level::BlockGetter;
use crate::world::level::block::impls::diode_block::is_diode;
use linkme::distributed_slice;
use mc_core::direction::{Axis, Direction};
use mc_core::phys::{AABB, Vec3};
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::Block;
use mc_data::{EntityType, GameEvent, Item, SoundEvent};
use mc_nbt::CompoundTag;
use mc_protocol::components::{CUSTOM_NAME, MAP_ID, MapId};
use std::any::Any;

define_accessor!(pub DATA_ITEM: ItemStack = ItemFrame);
define_accessor!(pub DATA_ROTATION: Int = ItemFrame);

/// `ItemFrame.NUM_ROTATIONS`.
pub const NUM_ROTATIONS: i32 = 8;

pub struct ItemFrame {
    base: EntityData,
    attached: Attached,
    drop_chance: f32,
    fixed: bool,
    /// Where the add-item sound `setItem(item, false)` played while loading (it needs the level): played by
    /// [`Entity::read_level_dependent_data`].
    load_sound_pos: Option<Vec3>,
    /// The map of the item an NBT load replaced: `removeFramedMap(currentItem)` needs the level, so the map data forgets
    /// this frame in [`Entity::read_level_dependent_data`].
    replaced_map: Option<MapId>,
}

impl ItemFrame {
    /// `ItemFrame(type, level)`.
    pub fn new(ty: EntityType, id: EntityId) -> Self {
        let base = EntityData::new(
            ty,
            id,
            crate::world::entity::entity::TOTAL_AIR_SUPPLY,
            |b| {
                b.define(DATA_DIRECTION, hanging::DEFAULT_DIRECTION);
                b.define(DATA_ITEM, ItemStack::empty());
                b.define(DATA_ROTATION, 0);
            },
        );
        let mut frame = Self {
            base,
            attached: Attached::default(),
            drop_chance: 1.0,
            fixed: false,
            load_sound_pos: None,
            replaced_map: None,
        };
        frame.set_pos(0.0, 0.0, 0.0); // `Entity(type, level)`: `setPos(0, 0, 0)`
        frame.base.set_shared_flag(FLAG_INVISIBLE, false);
        frame
    }

    /// `ItemFrame(type, level, pos, direction)` (the item frame item and structure pieces).
    pub fn at(ty: EntityType, id: EntityId, pos: BlockPos, direction: Direction) -> Self {
        let mut frame = Self::new(ty, id);
        frame.attached.pos = pos;
        frame.set_direction(direction);
        frame.base.set_shared_flag(FLAG_INVISIBLE, false);
        frame
    }

    pub fn is_glow(&self) -> bool {
        self.base.entity_type == EntityType::GLOW_ITEM_FRAME
    }

    pub fn is_fixed(&self) -> bool {
        self.fixed
    }

    pub fn get_item(&self) -> ItemStack {
        self.base.entity_data.get(DATA_ITEM)
    }

    pub fn get_rotation(&self) -> i32 {
        self.base.entity_data.get(DATA_ROTATION)
    }

    pub fn has_framed_map(&self) -> bool {
        self.get_item().has(mc_data::DataComponentType::MAP_ID)
    }

    pub fn get_framed_map_id(stack: &ItemStack) -> Option<MapId> {
        stack.get(&MAP_ID).copied()
    }

    /// `getAnalogOutput()`.
    pub fn get_analog_output(&self) -> i32 {
        if self.get_item().is_empty() {
            0
        } else {
            self.get_rotation() % 8 + 1
        }
    }

    pub fn is_invisible_frame(&self) -> bool {
        self.base.get_shared_flag(FLAG_INVISIBLE)
    }

    fn get_remove_item_sound(&self) -> SoundEvent {
        if self.is_glow() {
            SoundEvent::ENTITY_GLOW_ITEM_FRAME_REMOVE_ITEM
        } else {
            SoundEvent::ENTITY_ITEM_FRAME_REMOVE_ITEM
        }
    }

    fn get_break_sound(&self) -> SoundEvent {
        if self.is_glow() {
            SoundEvent::ENTITY_GLOW_ITEM_FRAME_BREAK
        } else {
            SoundEvent::ENTITY_ITEM_FRAME_BREAK
        }
    }

    fn get_place_sound(&self) -> SoundEvent {
        if self.is_glow() {
            SoundEvent::ENTITY_GLOW_ITEM_FRAME_PLACE
        } else {
            SoundEvent::ENTITY_ITEM_FRAME_PLACE
        }
    }

    fn get_add_item_sound(&self) -> SoundEvent {
        if self.is_glow() {
            SoundEvent::ENTITY_GLOW_ITEM_FRAME_ADD_ITEM
        } else {
            SoundEvent::ENTITY_ITEM_FRAME_ADD_ITEM
        }
    }

    fn get_rotate_item_sound(&self) -> SoundEvent {
        if self.is_glow() {
            SoundEvent::ENTITY_GLOW_ITEM_FRAME_ROTATE_ITEM
        } else {
            SoundEvent::ENTITY_ITEM_FRAME_ROTATE_ITEM
        }
    }

    /// `getFrameItemStack()`.
    fn get_frame_item_stack(&self) -> ItemStack {
        ItemStack::new(if self.is_glow() {
            Item::GLOW_ITEM_FRAME
        } else {
            Item::ITEM_FRAME
        })
    }

    /// `getFrameItemStackWithData()`.
    fn get_frame_item_stack_with_data(&self) -> ItemStack {
        let mut stack = self.get_frame_item_stack();
        if let Some(name) = self.base.get_custom_name() {
            stack.set(CUSTOM_NAME, name);
        }
        stack
    }

    /// `getPickResult()`.
    pub fn get_pick_result_stack(&self) -> ItemStack {
        let framed = self.get_item();
        if framed.is_empty() {
            self.get_frame_item_stack_with_data()
        } else {
            framed.copy()
        }
    }

    fn set_invisible(&mut self, invisible: bool) {
        self.base.set_shared_flag(FLAG_INVISIBLE, invisible);
    }

    /// `createBoundingBox(pos, direction, hasFramedMap)`.
    fn create_bounding_box(pos: BlockPos, direction: Direction, has_framed_map: bool) -> AABB {
        let position = Vec3::at_center_of(&pos).relative(direction, -0.46875);
        let width = if has_framed_map { 1.0f32 } else { 0.75f32 };
        let height = if has_framed_map { 1.0f32 } else { 0.75f32 };
        let axis = direction.get_axis();
        let x_size = if axis == Axis::X {
            0.0625
        } else {
            f64::from(width)
        };
        let y_size = if axis == Axis::Y {
            0.0625
        } else {
            f64::from(height)
        };
        let z_size = if axis == Axis::Z {
            0.0625
        } else {
            f64::from(width)
        };
        AABB::of_size(position, x_size, y_size, z_size)
    }

    /// `setItem(itemStack, updateNeighbours)` without the add-item sound and the neighbour update, which need the
    /// level (callers run [`ItemFrame::after_set_item`]). Returns the stack that was stored.
    fn set_item_data(&mut self, stack: &ItemStack) -> ItemStack {
        let stored = if stack.is_empty() {
            stack.clone()
        } else {
            stack.copy_with_count(1)
        };
        self.recalculate_bounding_box(); // `onItemChanged(itemStack)`
        if self.base.entity_data.set(DATA_ITEM, stored.clone()) {
            self.on_synced_data_updated(DATA_ITEM.id());
        }
        stored
    }

    /// `setItem(itemStack, updateNeighbours)`.
    pub fn set_item(
        &mut self,
        level: &mut ServerLevel,
        stack: &ItemStack,
        update_neighbours: bool,
    ) {
        let stored = self.set_item_data(stack);
        if !stored.is_empty() {
            let sound = self.get_add_item_sound();
            self.play_sound(level, sound, 1.0, 1.0);
        }
        if update_neighbours {
            level.update_neighbour_for_output_signal(self.attached.pos, Block::AIR);
        }
    }

    /// `setRotation(rotation, updateNeighbours)`.
    pub fn set_rotation(&mut self, level: Option<&mut ServerLevel>, rotation: i32) {
        self.base.entity_data.set(DATA_ROTATION, rotation % 8);
        if let Some(level) = level {
            level.update_neighbour_for_output_signal(self.attached.pos, Block::AIR);
        }
    }

    /// `removeFramedMap(itemStack)`: the map data forgets this frame.
    fn remove_framed_map(&self, level: &mut ServerLevel, stack: &ItemStack) {
        if let Some(map_id) = Self::get_framed_map_id(stack) {
            super::map_tracking::removed_from_frame(level, map_id, self.attached.pos, self.base.id);
        }
    }

    /// `dropItem(level, causedBy, withFrame)`.
    fn drop_item_with_frame(
        &mut self,
        level: &mut ServerLevel,
        caused_by: Option<&Cause>,
        with_frame: bool,
    ) {
        if self.fixed {
            return;
        }
        let mut stack = self.get_item();
        self.set_item(level, &ItemStack::empty(), true);
        if !level.entity_drops() {
            if caused_by.is_none() {
                self.remove_framed_map(level, &stack);
            }
        } else if caused_by.is_some_and(|c| c.is_player && c.instabuild) {
            self.remove_framed_map(level, &stack);
        } else {
            if with_frame {
                let frame = self.get_frame_item_stack_with_data();
                hanging::spawn_at_location(self, level, frame, 0.0);
            }
            if !stack.is_empty() {
                stack = stack.copy();
                self.remove_framed_map(level, &stack);
                if self.base.random.next_float() < self.drop_chance {
                    hanging::spawn_at_location(self, level, stack, 0.0);
                }
            }
        }
    }

    fn can_hurt_when_fixed(level: &ServerLevel, source: &DamageSource) -> bool {
        source.is_tag(level, DamageTypeTags::BYPASSES_INVULNERABILITY)
            || source.is_creative_player()
    }
}

impl BlockAttached for ItemFrame {
    fn attached(&self) -> &Attached {
        &self.attached
    }
    fn attached_mut(&mut self) -> &mut Attached {
        &mut self.attached
    }
    fn recalculate_bounding_box(&mut self) {
        hanging::recalculate_bounding_box(self);
        // `syncPacketPositionCodec(getX(), getY(), getZ())`
        let p = self.base.position();
        self.base.packet_position_codec.set_base(p);
    }
    /// `survives()`.
    fn survives(&self, level: &ServerLevel) -> bool {
        if self.fixed {
            return true;
        }
        if hanging::has_level_collision(self, level, &self.get_pop_box()) {
            return false;
        }
        let direction = self.get_hanging_direction();
        let state = level.get_block_state(self.attached.pos.relative(direction.get_opposite()));
        if state.is_solid() || (direction.get_axis().is_horizontal() && is_diode(state)) {
            hanging::can_coexist(self, level, true)
        } else {
            false
        }
    }
    /// `dropItem(level, causedBy)`.
    fn drop_item(&mut self, level: &mut ServerLevel, caused_by: Option<&Cause>) {
        let sound = self.get_break_sound();
        self.play_sound(level, sound, 1.0, 1.0);
        self.drop_item_with_frame(level, caused_by, true);
        level.game_event_from_entity(
            caused_by.map(|c| c.id),
            GameEvent::BLOCK_CHANGE,
            self.base.position(),
        );
    }
    /// `onKilled()`.
    fn on_killed(&mut self, level: &mut ServerLevel) {
        let item = self.get_item();
        self.remove_framed_map(level, &item);
    }
}

impl Hanging for ItemFrame {
    fn calculate_bounding_box(&self, pos: BlockPos, direction: Direction) -> AABB {
        Self::create_bounding_box(pos, direction, self.has_framed_map())
    }
    fn get_pop_box(&self) -> AABB {
        Self::create_bounding_box(self.attached.pos, self.get_hanging_direction(), false)
    }
    /// `ItemFrame.setDirection(direction)`: any direction.
    fn set_direction(&mut self, direction: Direction) {
        hanging::set_direction_raw(self, direction);
        let b = &mut self.base;
        if direction.get_axis().is_horizontal() {
            b.set_x_rot(0.0);
            b.set_y_rot((direction.get_2d_data_value() * 90) as f32);
        } else {
            b.set_x_rot((-90 * direction.get_axis_direction().get_step()) as f32);
            b.set_y_rot(0.0);
        }
        b.x_rot_o = b.x_rot();
        b.y_rot_o = b.y_rot();
        self.recalculate_bounding_box();
    }
    fn play_placement_sound(&self, level: &mut ServerLevel) {
        let sound = self.get_place_sound();
        self.play_sound(level, sound, 1.0, 1.0);
    }
}

impl Entity for ItemFrame {
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
    /// The part of `readAdditionalSaveData` that needs the level: the add-item sound `setItem(item, false)` plays at the
    /// position the frame had while loading, and `removeFramedMap(currentItem)` of a replaced item (`/data merge entity`).
    fn read_level_dependent_data(&mut self, level: &mut ServerLevel) {
        if let Some(map_id) = self.replaced_map.take() {
            super::map_tracking::removed_from_frame(level, map_id, self.attached.pos, self.base.id);
        }
        if let Some(p) = self.load_sound_pos.take()
            && !self.base.is_silent()
        {
            let sound = self.get_add_item_sound();
            let source = self.get_sound_source();
            level.play_sound_at(None, p.x, p.y, p.z, sound, source, 1.0, 1.0);
        }
    }
    /// `getSlot(slot)`: slot 0 is the framed item.
    fn get_slot(&self, slot: i32) -> Option<ItemStack> {
        if slot == 0 {
            return Some(self.get_item());
        }
        crate::advancements::predicates::entity::support::get_slot(self, slot)
    }
    fn set_slot(&mut self, level: &mut ServerLevel, slot: i32, stack: ItemStack) {
        if slot == 0 {
            self.set_item(level, &stack, true);
        } else {
            crate::advancements::predicates::entity::support::set_slot(level, self, slot, stack);
        }
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
    fn ignore_explosion_in_level(
        &self,
        level: &ServerLevel,
        explosion: &dyn crate::world::level::explosion::Explosion,
    ) -> bool {
        block_attached::ignore_explosion(level, explosion)
    }
    fn r#move(&mut self, level: &mut ServerLevel, _mover: MoverType, delta: Vec3) {
        if !self.fixed {
            block_attached::do_move(self, level, delta);
        }
    }
    fn push_in_level(&mut self, level: &mut ServerLevel, impulse: Vec3) {
        if !self.fixed {
            block_attached::do_push(self, level, impulse.x, impulse.y, impulse.z);
        }
    }
    fn rotate(&mut self, rotation: mc_core::rotation::Rotation) -> f32 {
        hanging::rotate(self, rotation)
    }
    fn mirror(&mut self, mirror: mc_core::mirror::Mirror) -> f32 {
        hanging::mirror(self, mirror)
    }
    fn get_pick_result(&self) -> Option<ItemStack> {
        Some(self.get_pick_result_stack())
    }
    /// `trackingPosition()` is the entity position (only paintings override it).
    fn on_synced_data_updated(&mut self, accessor_id: u8) {
        if accessor_id == DATA_DIRECTION.id() {
            let direction = self.get_hanging_direction();
            self.set_direction(direction);
        }
        if accessor_id == DATA_ITEM.id() {
            self.recalculate_bounding_box(); // `onItemChanged(getItem())`
        }
        if accessor_id == crate::world::entity::entity::DATA_POSE.id() {
            self.refresh_dimensions();
        }
    }
    fn hurt_server(&mut self, level: &mut ServerLevel, source: &DamageSource, damage: f32) -> bool {
        if !self.fixed {
            if self.is_invulnerable_to_base(level, source) {
                return false;
            }
            if !source.is_tag(level, DamageTypeTags::IS_EXPLOSION) && !self.get_item().is_empty() {
                self.drop_item_with_frame(
                    level,
                    source.causing_info().map(Cause::of).as_ref(),
                    false,
                );
                level.game_event_from_entity(
                    source.get_entity(),
                    GameEvent::BLOCK_CHANGE,
                    self.base.position(),
                );
                let sound = self.get_remove_item_sound();
                self.play_sound(level, sound, 1.0, 1.0);
                true
            } else {
                block_attached::hurt_server(self, level, source, damage)
            }
        } else {
            Self::can_hurt_when_fixed(level, source)
                && block_attached::hurt_server(self, level, source, damage)
        }
    }
    fn get_add_entity_packet(
        &self,
        server_entity: &ServerEntity,
    ) -> mc_protocol::packets::play::ClientboundAddEntityPacket {
        hanging::add_entity_packet(
            server_entity,
            self,
            i32::try_from(self.get_hanging_direction().get_3d_data_value()).unwrap_or(0),
            self.attached.pos,
        )
    }
    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        block_attached::add_additional_save_data(self, out);
        let item = self.get_item();
        if !item.is_empty()
            && let Ok(tag) = item.to_tag().into_result()
        {
            out.put("Item", tag);
        }
        out.put_byte("ItemRotation", self.get_rotation() as i8);
        out.put_float("ItemDropChance", self.drop_chance);
        out.put_byte(
            "Facing",
            self.get_hanging_direction().get_3d_data_value() as i8,
        );
        out.put_boolean("Invisible", self.is_invisible_frame());
        out.put_boolean("Fixed", self.fixed);
    }
    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        block_attached::read_additional_save_data(self, input);
        let stack = input
            .get("Item")
            .and_then(|t| ItemStack::from_tag(t).into_result().ok())
            .unwrap_or_else(ItemStack::empty);
        let current = self.get_item();
        if !current.is_empty() && !ItemStack::matches(&stack, &current) {
            self.replaced_map = Self::get_framed_map_id(&current);
        }
        let stored = self.set_item_data(&stack);
        if !stored.is_empty() {
            self.load_sound_pos = Some(self.base.position());
        }
        let rotation = i32::from(input.get_byte_or("ItemRotation", 0));
        self.set_rotation(None, rotation);
        self.drop_chance = input.get_float_or("ItemDropChance", 1.0);
        let facing = input
            .get("Facing")
            .and_then(facing_from_tag)
            .unwrap_or(Direction::Down);
        self.set_direction(facing);
        let invisible = input.get_boolean_or("Invisible", false);
        self.set_invisible(invisible);
        self.fixed = input.get_boolean_or("Fixed", false);
    }

    /// `interact(player, hand, location)`.
    fn interact(
        &mut self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        hand: InteractionHand,
        _location: Vec3,
    ) -> InteractionResult {
        let mut held = player.get_item_in_hand(hand);
        if self.fixed {
            return InteractionResult::PASS;
        }
        if self.get_item().is_empty() {
            if !held.is_empty() && !self.is_removed() {
                if let Some(over_limit) =
                    super::map_tracking::tracked_count_over_limit(level, &held, 256)
                    && over_limit
                {
                    return InteractionResult::FAIL;
                }
                self.set_item(level, &held, true);
                level.game_event_from_entity_ref(
                    &*player,
                    GameEvent::BLOCK_CHANGE,
                    self.base.position(),
                );
                held.consume(1, player.has_infinite_materials());
                player.set_item_in_hand(hand, held);
                InteractionResult::SUCCESS
            } else {
                InteractionResult::PASS
            }
        } else {
            let sound = self.get_rotate_item_sound();
            self.play_sound(level, sound, 1.0, 1.0);
            let rotation = self.get_rotation() + 1;
            self.set_rotation(Some(level), rotation);
            level.game_event_from_entity_ref(
                &*player,
                GameEvent::BLOCK_CHANGE,
                self.base.position(),
            );
            InteractionResult::SUCCESS
        }
    }

    /// `getVisualRotationYInDegrees()` is client-only; kept off the wire.
    fn tracking_position(&self) -> Vec3 {
        self.base.position()
    }
}

/// `Direction.LEGACY_ID_CODEC`: a byte holding the 3D data value.
fn facing_from_tag(tag: &mc_nbt::Tag) -> Option<Direction> {
    tag.as_number()
        .map(|n| Direction::from_3d_data_value(i32::from(n.byte_value())))
}

/// `entity instanceof ItemFrame`: its facing and `getAnalogOutput()` (ComparatorBlock.getItemFrame).
pub fn analog_output_of(entity: &dyn Entity) -> Option<(Direction, i32)> {
    entity
        .as_any()
        .downcast_ref::<ItemFrame>()
        .map(|frame| (frame.get_hanging_direction(), frame.get_analog_output()))
}

/// `entity instanceof ItemFrame frame && frame.getItem().getItem() instanceof MapItem`: the framed map's id, the stack and
/// what `MapItemSavedData.tickCarriedBy` records of the frame.
pub fn framed_map_of(
    entity: &dyn Entity,
) -> Option<(
    MapId,
    ItemStack,
    crate::world::level::saveddata::maps::FrameInfo,
)> {
    let frame = entity.as_any().downcast_ref::<ItemFrame>()?;
    let framed = frame.base.entity_data.get_ref(DATA_ITEM);
    if framed.item() != Item::FILLED_MAP {
        return None;
    }
    let stack = framed.clone();
    let map_id = ItemFrame::get_framed_map_id(&stack)?;
    let info = crate::world::level::saveddata::maps::FrameInfo {
        pos: frame.attached.pos,
        direction_2d: frame.get_hanging_direction().get_2d_data_value(),
        entity_id: frame.base.id,
    };
    Some((map_id, stack, info))
}

#[distributed_slice(ENTITY_FACTORIES)]
static ITEM_FRAME_FACTORY: EntityRegistration = EntityRegistration {
    entity_type: EntityType::ITEM_FRAME,
    create: |ty, id, _registries| Box::new(ItemFrame::new(ty, id)),
};
#[distributed_slice(ENTITY_FACTORIES)]
static GLOW_ITEM_FRAME_FACTORY: EntityRegistration = EntityRegistration {
    entity_type: EntityType::GLOW_ITEM_FRAME,
    create: |ty, id, _registries| Box::new(ItemFrame::new(ty, id)),
};
