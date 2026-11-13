//! `net.minecraft.world.entity.decoration.ArmorStand`.

use crate::define_accessor;
use crate::server::level::ServerLevel;
use crate::world::damagesource::{DamageSource, DamageTypeTags};
use crate::world::entity::dimensions::EntityDimensions;
use crate::world::entity::entity::{EntityData, FLAG_INVISIBLE, TOTAL_AIR_SUPPLY};
use crate::world::entity::equipment::{EquipmentSlot, EquipmentSlotType};
use crate::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use crate::world::entity::living::{self, LivingEntityData};
use crate::world::entity::living_entity::equipment::on_equip_item;
use crate::world::entity::{Entity, EntityId, RemovalReason, ServerPlayer};
use crate::world::item::{InteractionHand, InteractionResult, ItemStack};
use linkme::distributed_slice;
use mc_core::phys::Vec3;
use mc_data::enums::{Pose, PushReaction};
use mc_data::{Attribute, EntityType, GameEvent, Item, SoundEvent};
use mc_nbt::{CompoundTag, ListTag, Tag};
use mc_protocol::components::CUSTOM_NAME;
use mc_protocol::packets::play::{ParticleOptions, Rotations};
use std::any::Any;

define_accessor!(pub DATA_CLIENT_FLAGS: Byte = ArmorStand);
define_accessor!(pub DATA_HEAD_POSE: Rotations = ArmorStand);
define_accessor!(pub DATA_BODY_POSE: Rotations = ArmorStand);
define_accessor!(pub DATA_LEFT_ARM_POSE: Rotations = ArmorStand);
define_accessor!(pub DATA_RIGHT_ARM_POSE: Rotations = ArmorStand);
define_accessor!(pub DATA_LEFT_LEG_POSE: Rotations = ArmorStand);
define_accessor!(pub DATA_RIGHT_LEG_POSE: Rotations = ArmorStand);

/// `ArmorStand.WOBBLE_TIME`.
pub const WOBBLE_TIME: i32 = 5;
const DISABLE_TAKING_OFFSET: i32 = 8;
const DISABLE_PUTTING_OFFSET: i32 = 16;
const CLIENT_FLAG_SMALL: i8 = 1;
const CLIENT_FLAG_SHOW_ARMS: i8 = 4;
const CLIENT_FLAG_NO_BASEPLATE: i8 = 8;
const CLIENT_FLAG_MARKER: i8 = 16;

pub const DEFAULT_HEAD_POSE: Rotations = Rotations {
    x: 0.0,
    y: 0.0,
    z: 0.0,
};
pub const DEFAULT_BODY_POSE: Rotations = Rotations {
    x: 0.0,
    y: 0.0,
    z: 0.0,
};
pub const DEFAULT_LEFT_ARM_POSE: Rotations = Rotations {
    x: -10.0,
    y: 0.0,
    z: -10.0,
};
pub const DEFAULT_RIGHT_ARM_POSE: Rotations = Rotations {
    x: -15.0,
    y: 0.0,
    z: 10.0,
};
pub const DEFAULT_LEFT_LEG_POSE: Rotations = Rotations {
    x: -1.0,
    y: 0.0,
    z: -1.0,
};
pub const DEFAULT_RIGHT_LEG_POSE: Rotations = Rotations {
    x: 1.0,
    y: 0.0,
    z: 1.0,
};

pub struct ArmorStand {
    living: LivingEntityData,
    /// `invisible` (the value of `setInvisible`, which `updateInvisibilityStatus` restores over potion effects).
    invisible: bool,
    pub last_hit: i64,
    disabled_slots: i32,
}

impl ArmorStand {
    /// `ArmorStand(type, level)`.
    pub fn new(ty: EntityType, id: EntityId) -> Self {
        let living = LivingEntityData::new(ty, id, TOTAL_AIR_SUPPLY, |b| {
            b.define(DATA_CLIENT_FLAGS, 0);
            b.define(DATA_HEAD_POSE, DEFAULT_HEAD_POSE);
            b.define(DATA_BODY_POSE, DEFAULT_BODY_POSE);
            b.define(DATA_LEFT_ARM_POSE, DEFAULT_LEFT_ARM_POSE);
            b.define(DATA_RIGHT_ARM_POSE, DEFAULT_RIGHT_ARM_POSE);
            b.define(DATA_LEFT_LEG_POSE, DEFAULT_LEFT_LEG_POSE);
            b.define(DATA_RIGHT_LEG_POSE, DEFAULT_RIGHT_LEG_POSE);
        });
        Self {
            living,
            invisible: false,
            last_hit: 0,
            disabled_slots: 0,
        }
    }

    /// `ArmorStand(level, x, y, z)`.
    pub fn at(id: EntityId, x: f64, y: f64, z: f64) -> Self {
        let mut stand = Self::new(EntityType::ARMOR_STAND, id);
        stand.set_pos(x, y, z);
        stand
    }

    fn data(&self) -> &EntityData {
        &self.living.entity
    }

    fn flags(&self) -> i8 {
        self.living.entity.entity_data.get(DATA_CLIENT_FLAGS)
    }

    fn set_flag(&mut self, bit: i8, value: bool) {
        let data = self.flags();
        let data = if value { data | bit } else { data & !bit };
        if self.living.entity.entity_data.set(DATA_CLIENT_FLAGS, data) {
            self.on_synced_data_updated(DATA_CLIENT_FLAGS.id());
        }
    }

    pub fn is_small(&self) -> bool {
        self.flags() & CLIENT_FLAG_SMALL != 0
    }

    fn set_small(&mut self, value: bool) {
        self.set_flag(CLIENT_FLAG_SMALL, value);
    }

    pub fn show_arms(&self) -> bool {
        self.flags() & CLIENT_FLAG_SHOW_ARMS != 0
    }

    pub fn set_show_arms(&mut self, value: bool) {
        self.set_flag(CLIENT_FLAG_SHOW_ARMS, value);
    }

    pub fn show_base_plate(&self) -> bool {
        self.flags() & CLIENT_FLAG_NO_BASEPLATE == 0
    }

    pub fn set_no_base_plate(&mut self, value: bool) {
        self.set_flag(CLIENT_FLAG_NO_BASEPLATE, value);
    }

    pub fn is_marker(&self) -> bool {
        self.flags() & CLIENT_FLAG_MARKER != 0
    }

    fn set_marker(&mut self, value: bool) {
        self.set_flag(CLIENT_FLAG_MARKER, value);
    }

    /// `setInvisible(invisible)`.
    pub fn set_invisible(&mut self, invisible: bool) {
        self.invisible = invisible;
        self.living
            .entity
            .set_shared_flag(FLAG_INVISIBLE, invisible);
    }

    fn has_physics(&self) -> bool {
        !self.is_marker() && !self.data().is_no_gravity()
    }

    pub fn get_head_pose(&self) -> Rotations {
        self.data().entity_data.get(DATA_HEAD_POSE)
    }
    pub fn get_body_pose(&self) -> Rotations {
        self.data().entity_data.get(DATA_BODY_POSE)
    }
    pub fn get_left_arm_pose(&self) -> Rotations {
        self.data().entity_data.get(DATA_LEFT_ARM_POSE)
    }
    pub fn get_right_arm_pose(&self) -> Rotations {
        self.data().entity_data.get(DATA_RIGHT_ARM_POSE)
    }
    pub fn get_left_leg_pose(&self) -> Rotations {
        self.data().entity_data.get(DATA_LEFT_LEG_POSE)
    }
    pub fn get_right_leg_pose(&self) -> Rotations {
        self.data().entity_data.get(DATA_RIGHT_LEG_POSE)
    }
    pub fn set_head_pose(&mut self, pose: Rotations) {
        self.living.entity.entity_data.set(DATA_HEAD_POSE, pose);
    }
    pub fn set_body_pose(&mut self, pose: Rotations) {
        self.living.entity.entity_data.set(DATA_BODY_POSE, pose);
    }
    pub fn set_left_arm_pose(&mut self, pose: Rotations) {
        self.living.entity.entity_data.set(DATA_LEFT_ARM_POSE, pose);
    }
    pub fn set_right_arm_pose(&mut self, pose: Rotations) {
        self.living
            .entity
            .entity_data
            .set(DATA_RIGHT_ARM_POSE, pose);
    }
    pub fn set_left_leg_pose(&mut self, pose: Rotations) {
        self.living.entity.entity_data.set(DATA_LEFT_LEG_POSE, pose);
    }
    pub fn set_right_leg_pose(&mut self, pose: Rotations) {
        self.living
            .entity
            .entity_data
            .set(DATA_RIGHT_LEG_POSE, pose);
    }

    /// `setArmorStandPose(pose)` (`ArmorStandPose.CODEC`).
    fn set_armor_stand_pose(&mut self, pose: [Rotations; 6]) {
        self.set_head_pose(pose[0]);
        self.set_body_pose(pose[1]);
        self.set_left_arm_pose(pose[2]);
        self.set_right_arm_pose(pose[3]);
        self.set_left_leg_pose(pose[4]);
        self.set_right_leg_pose(pose[5]);
    }

    fn has_item_in_slot(&self, slot: EquipmentSlot) -> bool {
        !self.living.equipment.get(slot).is_empty()
    }

    /// `LivingEntity.setItemSlot(slot, stack)`.
    fn set_item_slot(&mut self, level: &mut ServerLevel, slot: EquipmentSlot, stack: ItemStack) {
        let old = self.living.equipment.set(slot, stack.clone());
        on_equip_item(self, level, slot, &old, &stack);
    }

    fn is_disabled(&self, slot: EquipmentSlot) -> bool {
        (self.disabled_slots & (1 << slot.get_id())) != 0
            || (slot.get_type() == EquipmentSlotType::Hand && !self.show_arms())
    }

    /// `getScale() * getAgeScale()` (`SCALE` attribute; armor stands are not ageable).
    fn scale(&self) -> f32 {
        self.living.get_attribute_value(Attribute::SCALE) as f32
    }

    /// `getClickedSlot(location)`.
    fn get_clicked_slot(&self, location: Vec3) -> EquipmentSlot {
        let mut slot_clicked = EquipmentSlot::MainHand;
        let small = self.is_small();
        // `location.y / (getScale() * getAgeScale())`: float product widened to double
        let click_y = location.y / f64::from(self.scale() * Entity::get_age_scale(self));
        if click_y >= 0.1
            && click_y < 0.1 + if small { 0.8 } else { 0.45 }
            && self.has_item_in_slot(EquipmentSlot::Feet)
        {
            slot_clicked = EquipmentSlot::Feet;
        } else if click_y >= 0.9 + if small { 0.3 } else { 0.0 }
            && click_y < 0.9 + if small { 1.0 } else { 0.7 }
            && self.has_item_in_slot(EquipmentSlot::Chest)
        {
            slot_clicked = EquipmentSlot::Chest;
        } else if click_y >= 0.4
            && click_y < 0.4 + if small { 1.0 } else { 0.8 }
            && self.has_item_in_slot(EquipmentSlot::Legs)
        {
            slot_clicked = EquipmentSlot::Legs;
        } else if click_y >= 1.6 && self.has_item_in_slot(EquipmentSlot::Head) {
            slot_clicked = EquipmentSlot::Head;
        } else if !self.has_item_in_slot(EquipmentSlot::MainHand)
            && self.has_item_in_slot(EquipmentSlot::OffHand)
        {
            slot_clicked = EquipmentSlot::OffHand;
        }
        slot_clicked
    }

    /// `swapItem(player, slot, playerItemStack, hand)`.
    fn swap_item(
        &mut self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        slot: EquipmentSlot,
        mut player_stack: ItemStack,
        hand: InteractionHand,
    ) -> bool {
        let item_stack = self.living.equipment.get(slot).clone();
        if (!item_stack.is_empty()
            && (self.disabled_slots & (1 << (slot.get_id() + DISABLE_TAKING_OFFSET))) != 0)
            || (item_stack.is_empty()
                && (self.disabled_slots & (1 << (slot.get_id() + DISABLE_PUTTING_OFFSET))) != 0)
        {
            false
        } else if player.has_infinite_materials()
            && item_stack.is_empty()
            && !player_stack.is_empty()
        {
            self.set_item_slot(level, slot, player_stack.copy_with_count(1));
            true
        } else if player_stack.is_empty() || player_stack.count() <= 1 {
            self.set_item_slot(level, slot, player_stack);
            player.set_item_in_hand_slot(level, hand, item_stack);
            true
        } else if !item_stack.is_empty() {
            false
        } else {
            let one = player_stack.split(1);
            self.set_item_slot(level, slot, one);
            player.set_item_in_hand(hand, player_stack);
            true
        }
    }

    /// `showBreakingParticles()`.
    fn show_breaking_particles(&self, level: &mut ServerLevel) {
        let state = mc_data::block::Block::OAK_PLANKS.default_state();
        let p = self.data().position();
        level.send_particles(
            ParticleOptions::Block {
                particle_type: mc_data::ParticleType::BLOCK,
                state,
            },
            p.x,
            self.data().get_y_progress(0.6666666666666666),
            p.z,
            10,
            f64::from(self.data().get_bb_width() / 4.0),
            f64::from(self.data().get_bb_height() / 4.0),
            f64::from(self.data().get_bb_width() / 4.0),
            0.05,
        );
    }

    /// `causeDamage(level, source, dmg)`.
    fn cause_damage(&mut self, level: &mut ServerLevel, source: &DamageSource, dmg: f32) {
        let health = self.living.get_health() - dmg;
        if health <= 0.5 {
            self.broken_by_anything(level, source);
            self.kill_attributed(level, source.get_entity());
        } else {
            self.living.set_health(health);
            level.game_event_from_entity(
                source.get_entity(),
                GameEvent::ENTITY_DAMAGE,
                self.data().position(),
            );
        }
    }

    /// `brokenByPlayer(level, source)`.
    fn broken_by_player(&mut self, level: &mut ServerLevel, source: &DamageSource) {
        let mut result = ItemStack::new(Item::ARMOR_STAND);
        if let Some(name) = self.data().get_custom_name() {
            result.set(CUSTOM_NAME, name);
        }
        crate::world::level::block::helpers::pop_resource(
            level,
            self.data().block_position(),
            result,
        );
        self.broken_by_anything(level, source);
    }

    /// `brokenByAnything(level, source)`.
    fn broken_by_anything(&mut self, level: &mut ServerLevel, source: &DamageSource) {
        self.play_broken_sound(level);
        self.drop_all_death_loot(level, source);
        for slot in EquipmentSlot::VALUES {
            let item_stack = self.living.equipment.set(slot, ItemStack::empty());
            if !item_stack.is_empty()
                && !crate::world::item::enchantment_helper::has(
                    &level.registries,
                    &item_stack,
                    crate::world::item::enchantment::enchantment_effect_components::EffectComponent::prevent_equipment_drop,
                )
            {
                crate::world::level::block::helpers::pop_resource(level, self.data().block_position().above(), item_stack);
            }
        }
    }

    fn play_broken_sound(&self, level: &mut ServerLevel) {
        let p = self.data().position();
        level.play_sound_at(
            None,
            p.x,
            p.y,
            p.z,
            SoundEvent::ENTITY_ARMOR_STAND_BREAK,
            self.get_sound_source(),
            1.0,
            1.0,
        );
    }

    /// `kill(level, attributedTo)`.
    pub fn kill_attributed(&mut self, level: &mut ServerLevel, attributed_to: Option<EntityId>) {
        level.remove_held_entity(self.as_dyn_mut(), RemovalReason::Killed);
        let position = self.data().position();
        match attributed_to {
            Some(source) => {
                level.game_event_from_entity(Some(source), GameEvent::ENTITY_DIE, position)
            }
            None => {
                level.game_event_from_entity_ref(self.as_dyn(), GameEvent::ENTITY_DIE, position)
            }
        }
    }

    fn dimensions_marker(&self, is_marker: bool) -> EntityDimensions {
        if is_marker {
            EntityDimensions::fixed(0.0, 0.0)
        } else if self.is_small() {
            EntityDimensions::of_type(EntityType::ARMOR_STAND)
                .scale(0.5)
                .with_eye_height(0.9875)
        } else {
            EntityDimensions::of_type(EntityType::ARMOR_STAND)
        }
    }
}

fn rotations_to_nbt(r: Rotations) -> Tag {
    let mut list = ListTag::new();
    list.push(r.x);
    list.push(r.y);
    list.push(r.z);
    Tag::List(list)
}

/// `Rotations.CODEC`: exactly three numbers.
fn rotations_from_nbt(tag: Option<&Tag>) -> Option<Rotations> {
    let list = tag?.as_list()?;
    if list.len() != 3 {
        return None;
    }
    let n = |i: usize| {
        list.get(i)
            .and_then(|t| t.as_number())
            .map(|n| n.float_value())
    };
    Some(Rotations::new(n(0)?, n(1)?, n(2)?))
}

impl Entity for ArmorStand {
    fn base(&self) -> &EntityData {
        &self.living.entity
    }
    fn base_mut(&mut self) -> &mut EntityData {
        &mut self.living.entity
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
    fn living(&self) -> Option<&LivingEntityData> {
        Some(&self.living)
    }
    fn living_mut(&mut self) -> Option<&mut LivingEntityData> {
        Some(&mut self.living)
    }

    /// `canUseSlot(slot)`: no body armor, no saddle, nothing disabled.
    fn can_use_slot(&self, slot: EquipmentSlot) -> bool {
        slot != EquipmentSlot::Body && slot != EquipmentSlot::Saddle && !self.is_disabled(slot)
    }

    /// `refreshDimensions()`: the position is put back after the size change (no push out of blocks).
    fn refresh_dimensions(&mut self) {
        let pose = self.living.entity.get_pose();
        let new_dim = self.get_dimensions(pose);
        self.living.entity.dimensions = new_dim.clone();
        self.living.entity.eye_height = new_dim.eye_height;
        self.reapply_position();
        self.living.entity.pending_fudge_from = None;
    }

    /// `isEffectiveAi()`.
    fn is_effective_ai(&self) -> bool {
        self.has_physics()
    }

    fn is_pushable(&self, _level: &ServerLevel) -> bool {
        false
    }
    fn do_push(&mut self, _level: &mut ServerLevel, _other: &mut dyn Entity) {}
    /// `pushEntities()`: rideable minecarts in the box are pushed away.
    fn push_entities(&mut self, level: &mut ServerLevel) {
        let bb = self.data().bounding_box();
        let id = self.data().id;
        for target in level.get_entities(Some(id), &bb, |e| {
            e.as_abstract_minecart().is_some_and(|m| m.is_rideable())
        }) {
            let dist = level
                .get_entity(target)
                .map(|e| self.data().distance_to_sqr_vec(e.base().position()));
            if dist.is_some_and(|d| d <= 0.2) {
                level.with_entity(target, |level, e| e.push_entity(level, self));
            }
        }
    }

    /// `interact(player, hand, location)`.
    fn interact(
        &mut self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        hand: InteractionHand,
        location: Vec3,
    ) -> InteractionResult {
        let item_stack = player.get_item_in_hand(hand);
        if self.is_marker() || item_stack.is(Item::NAME_TAG) {
            return InteractionResult::PASS;
        }
        if player.is_spectator() {
            return InteractionResult::SUCCESS;
        }
        let item_in_hand_slot = self.get_equipment_slot_for_item(&item_stack);
        if item_stack.is_empty() {
            let clicked_slot = self.get_clicked_slot(location);
            let target_slot = if self.is_disabled(clicked_slot) {
                item_in_hand_slot
            } else {
                clicked_slot
            };
            if self.has_item_in_slot(target_slot)
                && self.swap_item(level, player, target_slot, item_stack.clone(), hand)
            {
                return InteractionResult::SUCCESS_SERVER;
            }
        } else {
            if self.is_disabled(item_in_hand_slot) {
                return InteractionResult::FAIL;
            }
            if item_in_hand_slot.get_type() == EquipmentSlotType::Hand && !self.show_arms() {
                return InteractionResult::FAIL;
            }
            if self.swap_item(level, player, item_in_hand_slot, item_stack.clone(), hand) {
                return InteractionResult::SUCCESS_SERVER;
            }
        }
        InteractionResult::PASS
    }

    fn hurt_server(
        &mut self,
        level: &mut ServerLevel,
        source: &DamageSource,
        _damage: f32,
    ) -> bool {
        if self.is_removed() {
            return false;
        }
        if !level.mob_griefing() && super::caused_by_mob(source.causing_info()) {
            return false;
        }
        if source.is_tag(level, DamageTypeTags::BYPASSES_INVULNERABILITY) {
            self.kill_attributed(level, source.get_entity());
            return false;
        }
        if self.is_invulnerable_to(level, source) || self.invisible || self.is_marker() {
            return false;
        }
        if source.is_tag(level, DamageTypeTags::IS_EXPLOSION) {
            self.broken_by_anything(level, source);
            self.kill_attributed(level, source.get_entity());
            return false;
        }
        if source.is_tag(level, DamageTypeTags::IGNITES_ARMOR_STANDS) {
            if self.is_on_fire() {
                self.cause_damage(level, source, 0.15);
            } else {
                self.ignite_for_seconds(5.0);
            }
            return false;
        }
        if source.is_tag(level, DamageTypeTags::BURNS_ARMOR_STANDS)
            && self.living.get_health() > 0.5
        {
            self.cause_damage(level, source, 4.0);
            return false;
        }
        let allow_incremental_breaking =
            source.is_tag(level, DamageTypeTags::CAN_BREAK_ARMOR_STAND);
        let should_kill = source.is_tag(level, DamageTypeTags::ALWAYS_KILLS_ARMOR_STANDS);
        if !allow_incremental_breaking && !should_kill {
            return false;
        }
        if source
            .causing_info()
            .is_some_and(|c| c.is_player && !c.may_build)
        {
            return false;
        }
        if source.is_creative_player() {
            self.play_broken_sound(level);
            self.show_breaking_particles(level);
            self.kill_attributed(level, source.get_entity());
            return true;
        }
        let time = level.game_time();
        if time - self.last_hit > 5 && !should_kill {
            crate::world::entity::living_entity::util::broadcast_entity_event(
                level,
                self.data().id,
                32,
            );
            level.game_event_from_entity(
                source.get_entity(),
                GameEvent::ENTITY_DAMAGE,
                self.data().position(),
            );
            self.last_hit = time;
        } else {
            self.broken_by_player(level, source);
            self.show_breaking_particles(level);
            self.kill_attributed(level, source.get_entity());
        }
        true
    }

    /// `tickHeadTurn(yBodyRotT)`: the body follows the rotation exactly.
    fn tick_head_turn(&mut self, _y_body_rot_t: f32) {
        let y_rot_o = self.living.entity.y_rot_o;
        let y_rot = self.living.entity.y_rot();
        self.living.y_body_rot_o = y_rot_o;
        self.living.y_body_rot = y_rot;
    }

    fn travel(&mut self, level: &mut ServerLevel, input: Vec3) {
        if self.has_physics() {
            crate::world::entity::living_entity::travel::travel(self, level, input);
        }
    }

    fn set_y_body_rot(&mut self, y_body_rot: f32) {
        self.living.y_body_rot_o = y_body_rot;
        self.living.entity.y_rot_o = y_body_rot;
        self.living.y_body_rot = y_body_rot;
        self.living.y_head_rot_o = y_body_rot;
        self.living.y_head_rot = y_body_rot;
    }

    fn set_y_head_rot(&mut self, y_head_rot: f32) {
        self.set_y_body_rot(y_head_rot);
    }

    /// `updateInvisibilityStatus()`.
    fn update_invisibility_status(&mut self) {
        self.set_invisible(self.invisible);
    }

    fn is_baby(&self) -> bool {
        self.is_small()
    }

    fn ignore_explosion(&self, explosion: &dyn crate::world::level::explosion::Explosion) -> bool {
        if explosion.should_affect_blocklike_entities() {
            self.is_invisible()
        } else {
            true
        }
    }

    fn get_piston_push_reaction(&self) -> PushReaction {
        if self.is_marker() {
            PushReaction::IgnoreEntity
        } else {
            PushReaction::PushPull
        }
    }

    fn is_ignoring_block_triggers(&self) -> bool {
        self.is_marker()
    }

    fn is_pickable(&self) -> bool {
        !self.is_removed() && !self.is_marker()
    }

    fn skip_attack_interaction(&mut self, level: &mut ServerLevel, source: &dyn Entity) -> bool {
        source.is_player()
            && !level.may_interact_player_uuid(source.uuid(), self.data().block_position())
    }

    fn get_fall_sounds(&self) -> (SoundEvent, SoundEvent) {
        (
            SoundEvent::ENTITY_ARMOR_STAND_FALL,
            SoundEvent::ENTITY_ARMOR_STAND_FALL,
        )
    }
    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_ARMOR_STAND_HIT)
    }
    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_ARMOR_STAND_BREAK)
    }
    fn thunder_hit(&mut self, _level: &mut ServerLevel) {}

    fn is_affected_by_potions(&self) -> bool {
        false
    }

    fn on_synced_data_updated(&mut self, accessor_id: u8) {
        if accessor_id == DATA_CLIENT_FLAGS.id() {
            self.refresh_dimensions();
            let marker = self.is_marker();
            self.living.entity.blocks_building = !marker;
        }
        if accessor_id == crate::world::entity::entity::DATA_POSE.id() {
            self.refresh_dimensions();
        }
    }

    fn attackable(&self) -> bool {
        false
    }

    /// `getDefaultDimensions(pose)` scaled by `getScale()`.
    fn get_dimensions(&self, _pose: Pose) -> EntityDimensions {
        let dimensions = self.dimensions_marker(self.is_marker());
        let scale = self.scale();
        dimensions.scale(scale)
    }

    fn get_pick_result(&self) -> Option<ItemStack> {
        Some(ItemStack::new(Item::ARMOR_STAND))
    }

    fn can_be_seen_by_anyone(&self) -> bool {
        !self.is_invisible() && !self.is_marker()
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        living::add_additional_save_data(
            &self.living,
            self.living.absorption_amount,
            &self.living.equipment,
            out,
        );
        out.put_boolean("Invisible", self.is_invisible());
        out.put_boolean("Small", self.is_small());
        out.put_boolean("ShowArms", self.show_arms());
        out.put_int("DisabledSlots", self.disabled_slots);
        out.put_boolean("NoBasePlate", !self.show_base_plate());
        if self.is_marker() {
            out.put_boolean("Marker", true);
        }
        // `ArmorStandPose.CODEC`: `optionalFieldOf(name, default)` leaves out the poses that are the default
        let mut pose = CompoundTag::new();
        for (name, value, default) in [
            ("Head", self.get_head_pose(), DEFAULT_HEAD_POSE),
            ("Body", self.get_body_pose(), DEFAULT_BODY_POSE),
            ("LeftArm", self.get_left_arm_pose(), DEFAULT_LEFT_ARM_POSE),
            (
                "RightArm",
                self.get_right_arm_pose(),
                DEFAULT_RIGHT_ARM_POSE,
            ),
            ("LeftLeg", self.get_left_leg_pose(), DEFAULT_LEFT_LEG_POSE),
            (
                "RightLeg",
                self.get_right_leg_pose(),
                DEFAULT_RIGHT_LEG_POSE,
            ),
        ] {
            if value != default {
                pose.put(name, rotations_to_nbt(value));
            }
        }
        out.put("Pose", Tag::Compound(pose));
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        let (absorption, equipment) = living::read_additional_save_data(&mut self.living, input);
        self.living.absorption_amount = absorption;
        self.living.equipment = equipment;
        self.set_invisible(input.get_boolean_or("Invisible", false));
        self.set_small(input.get_boolean_or("Small", false));
        self.set_show_arms(input.get_boolean_or("ShowArms", false));
        self.disabled_slots = input.get_int_or("DisabledSlots", 0);
        self.set_no_base_plate(input.get_boolean_or("NoBasePlate", false));
        self.set_marker(input.get_boolean_or("Marker", false));
        self.living.entity.no_physics = !self.has_physics();
        if let Some(pose) = input.get_compound("Pose") {
            let read = |key: &str, default: Rotations| {
                rotations_from_nbt(pose.get(key)).unwrap_or(default)
            };
            self.set_armor_stand_pose([
                read("Head", DEFAULT_HEAD_POSE),
                read("Body", DEFAULT_BODY_POSE),
                read("LeftArm", DEFAULT_LEFT_ARM_POSE),
                read("RightArm", DEFAULT_RIGHT_ARM_POSE),
                read("LeftLeg", DEFAULT_LEFT_LEG_POSE),
                read("RightLeg", DEFAULT_RIGHT_LEG_POSE),
            ]);
        }
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static ARMOR_STAND_FACTORY: EntityRegistration = EntityRegistration {
    entity_type: EntityType::ARMOR_STAND,
    create: |ty, id, _registries| Box::new(ArmorStand::new(ty, id)),
};
