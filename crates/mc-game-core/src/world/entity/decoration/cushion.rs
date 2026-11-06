//! `net.minecraft.world.entity.decoration.Cushion`.

use super::block_attached::{self, Attached, BlockAttached, Cause};
use crate::define_accessor;
use crate::server::level::ServerLevel;
use crate::world::damagesource::DamageSource;
use crate::world::entity::entity::EntityData;
use crate::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use crate::world::entity::inside_block_effects::collided_with_fluid;
use crate::world::entity::item::ItemEntity;
use crate::world::entity::{Entity, EntityId, MoverType, ServerPlayer};
use crate::world::item::{InteractionHand, InteractionResult, ItemStack};
use crate::world::level::BlockGetter;
use crate::world::level::block::BlockStateExt;
use crate::world::level::collision::CollisionGetter;
use crate::world::level::material::FluidStateExt;
use linkme::distributed_slice;
use mc_core::phys::shapes::{BooleanOp, Shapes};
use mc_core::phys::{AABB, Vec3};
use mc_core::pos::BlockPos;
use mc_data::block::Block;
use mc_data::{EntityType, Item, SoundEvent};
use mc_nbt::CompoundTag;
use mc_protocol::components::{CUSTOM_NAME, DyeColor};
use mc_protocol::packets::play::ParticleOptions;
use std::any::Any;

define_accessor!(pub DATA_COLOR: DyeColor = Cushion);

type DataDyeColor = mc_protocol::packets::play::DyeColor;

const DEFAULT_COLOR: DyeColor = DyeColor::White;

/// `DyeColor` of the synched data (`EntityDataSerializers.DYE_COLOR`) from the component enum (same ids).
fn to_data(color: DyeColor) -> DataDyeColor {
    DataDyeColor::by_id(color.id())
}

fn from_data(color: DataDyeColor) -> DyeColor {
    DyeColor::from_id(color.id()).unwrap_or(DyeColor::White)
}
const LIGHTNING_DROP_INVULNERABLE_TICKS: i32 = 20;

/// `Items.CUSHION.pick(color)`.
pub fn cushion_item(color: DyeColor) -> Item {
    Item::by_name(&format!("minecraft:{}_cushion", color.name())).expect("cushion item")
}

/// `Blocks.WOOL.pick(color)`.
fn wool_block(color: DyeColor) -> Block {
    Block::by_name(&format!("minecraft:{}_wool", color.name())).expect("wool block")
}

pub struct Cushion {
    base: EntityData,
    attached: Attached,
}

impl Cushion {
    /// `Cushion(type, level)`.
    pub fn new(ty: EntityType, id: EntityId) -> Self {
        let base = EntityData::new(
            ty,
            id,
            crate::world::entity::entity::TOTAL_AIR_SUPPLY,
            |b| {
                b.define(DATA_COLOR, to_data(DEFAULT_COLOR));
            },
        );
        let mut cushion = Self {
            base,
            attached: Attached::default(),
        };
        cushion.set_pos(0.0, 0.0, 0.0);
        cushion
    }

    pub fn get_color(&self) -> DyeColor {
        from_data(self.base.entity_data.get(DATA_COLOR))
    }

    pub fn set_color(&mut self, color: DyeColor) {
        self.base.entity_data.set(DATA_COLOR, to_data(color));
    }

    fn cushion_item_stack_with_data(&self) -> ItemStack {
        let mut stack = ItemStack::new(cushion_item(self.get_color()));
        if let Some(name) = self.base.get_custom_name() {
            stack.set(CUSTOM_NAME, name);
        }
        stack
    }

    fn show_breaking_particles(&self, level: &mut ServerLevel) {
        let state = wool_block(self.get_color()).default_state();
        let p = self.base.position();
        level.send_particles(
            ParticleOptions::Block {
                particle_type: mc_data::ParticleType::BLOCK,
                state,
            },
            p.x,
            self.base.get_y_progress(0.6666666666666666),
            p.z,
            10,
            f64::from(self.base.get_bb_width() / 4.0),
            f64::from(self.base.get_bb_height() / 4.0),
            f64::from(self.base.get_bb_width() / 4.0),
            0.05,
        );
    }

    /// `destroyIfInFire(level)`.
    pub fn destroy_if_in_fire(&mut self, level: &mut ServerLevel) {
        if !self.is_removed() {
            let aabb = self.base.bounding_box().next_deflated();
            if super::any_block_in(level, &aabb, |state| state.is_tag("fire")) {
                let source = level.damage_sources().in_fire();
                self.hurt_server(level, &source, 1.0);
            }
        }
    }

    /// `canBePlacedAt(level, boundingBox)`.
    pub fn can_be_placed_at(level: &ServerLevel, bounding_box: &AABB) -> bool {
        Self::would_survive_at(level, bounding_box) && !Self::is_anchor_buried(level, bounding_box)
    }

    /// `wouldSurviveAt(level, boundingBox)`.
    pub fn would_survive_at(level: &ServerLevel, bounding_box: &AABB) -> bool {
        Self::has_anchor_below(level, bounding_box)
            && !Self::is_covered_by_suffocating_blocks(level, bounding_box)
    }

    fn has_anchor_below(level: &ServerLevel, b: &AABB) -> bool {
        let anchor_box = AABB::new(
            b.min_x,
            b.min_y - 0.015625,
            b.min_z,
            next_down(b.max_x),
            b.min_y,
            next_down(b.max_z),
        );
        let search = anchor_box.expand_towards(0.0, -0.125, 0.0);
        // `forEachUntil` aborting = a shape whose bounds touch the anchor box was found
        let from = BlockPos::containing_position(&search.get_min_position());
        let to = BlockPos::containing_position(&search.get_max_position());
        BlockPos::between_closed(&from, &to).any(|pos| {
            let shape = level.get_block_state(pos).get_shape(level, pos);
            !shape.is_empty()
                && shape
                    .bounds()
                    .move_block_pos(&pos)
                    .intersects_aabb(&anchor_box)
        })
    }

    fn is_anchor_buried(level: &ServerLevel, b: &AABB) -> bool {
        let resting_slice = AABB::new(
            b.min_x,
            b.min_y,
            b.min_z,
            b.max_x,
            b.min_y + 0.015625,
            b.max_z,
        )
        .next_deflated();
        let mut exposed_surface = Shapes::create(resting_slice);
        for collider in level.get_block_collisions(None, &resting_slice) {
            exposed_surface = Shapes::join(&exposed_surface, &collider, BooleanOp::ONLY_FIRST);
            if exposed_surface.is_empty() {
                return true;
            }
        }
        false
    }

    fn is_covered_by_suffocating_blocks(level: &ServerLevel, b: &AABB) -> bool {
        let deflated = b.next_deflated();
        let from = BlockPos::containing_position(&deflated.get_min_position());
        let to = BlockPos::containing_position(&deflated.get_max_position());
        BlockPos::between_closed(&from, &to)
            .all(|pos| level.get_block_state(pos).is_suffocating_at(level, pos))
    }
}

/// `Math.nextDown(double)`.
fn next_down(value: f64) -> f64 {
    value.next_down()
}

impl BlockAttached for Cushion {
    fn attached(&self) -> &Attached {
        &self.attached
    }
    fn attached_mut(&mut self) -> &mut Attached {
        &mut self.attached
    }
    /// `recalculateBoundingBox()`: `makeBoundingBox()`.
    fn recalculate_bounding_box(&mut self) {
        let bb = self.make_bounding_box(self.base.position());
        self.base.set_bounding_box(bb);
    }
    fn survives(&self, level: &ServerLevel) -> bool {
        Self::would_survive_at(level, &self.base.bounding_box())
    }
    /// `dropItem(level, causedBy)`.
    fn drop_item(&mut self, level: &mut ServerLevel, caused_by: Option<&Cause>) {
        self.play_sound(level, SoundEvent::ENTITY_CUSHION_BREAK, 1.0, 1.0);
        self.show_breaking_particles(level);
        if level.entity_drops() && !caused_by.is_some_and(|c| c.is_player && c.instabuild) {
            let stack = self.cushion_item_stack_with_data();
            let p = self.base.position();
            let mut item = ItemEntity::with_stack(level, p.x, p.y, p.z, stack);
            item.set_default_pick_up_delay();
            if caused_by.is_some_and(|c| c.is_lightning) {
                item.base_mut().invulnerable_time = LIGHTNING_DROP_INVULNERABLE_TICKS;
            }
            level.add_fresh_entity(Box::new(item));
        }
    }
    /// `tickAtCheckInterval()`.
    fn tick_at_check_interval(&mut self, level: &mut ServerLevel) {
        let block_pos = self.base.block_position();
        let fluid_state = level.get_block_state(block_pos).get_fluid_state();
        let position = self.base.position();
        if collided_with_fluid(self, level, fluid_state, block_pos, position, position) {
            let mut collector = std::mem::take(&mut self.base.inside_effect_collector);
            crate::world::level::material::fluid_behaviour(fluid_state.get_type()).entity_inside(
                level,
                block_pos,
                self,
                &mut collector,
            );
            collector.apply_and_clear(self, level);
            self.base.inside_effect_collector = collector;
        }
        self.destroy_if_in_fire(level);
    }
}

impl Entity for Cushion {
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
    fn tick(&mut self, level: &mut ServerLevel) {
        block_attached::tick(self, level);
    }
    /// `setPos(x, y, z)`: `setPosRaw` then the block-attached one.
    fn set_pos(&mut self, x: f64, y: f64, z: f64) {
        self.base.set_pos_raw(x, y, z);
        block_attached::set_pos(self, x, y, z);
    }
    fn refresh_dimensions(&mut self) {}
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
    fn get_pick_result(&self) -> Option<ItemStack> {
        Some(ItemStack::new(cushion_item(self.get_color())))
    }
    /// `thunderHit(level, lightningBolt)` without the bolt: unreachable (lightning calls `thunder_hit_by`).
    fn thunder_hit(&mut self, _level: &mut ServerLevel) {}
    /// `thunderHit(level, lightningBolt)`.
    fn thunder_hit_by(&mut self, level: &mut ServerLevel, bolt: EntityId) {
        if !self.is_removed() {
            block_attached::kill(self, level, Some(bolt));
            self.drop_item(level, Some(&Cause::lightning(bolt)));
        }
    }
    fn hurt_server(&mut self, level: &mut ServerLevel, source: &DamageSource, damage: f32) -> bool {
        let cause = source.causing_info();
        if cause.is_some_and(|c| c.is_player && !c.may_build)
            || super::breaking_denied_at(level, cause, self.attached.pos)
        {
            false
        } else {
            block_attached::hurt_server(self, level, source, damage)
        }
    }
    /// `interact(player, hand, location)`.
    fn interact(
        &mut self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        _hand: InteractionHand,
        _location: Vec3,
    ) -> InteractionResult {
        if player.is_shift_key_down() || self.base.is_vehicle() {
            InteractionResult::PASS
        } else if level.start_riding_held(player, self, false, true) {
            self.play_sound(level, SoundEvent::ENTITY_CUSHION_SIT, 1.0, 1.0);
            InteractionResult::SUCCESS_SERVER
        } else {
            InteractionResult::CONSUME
        }
    }
    /// `removePassenger(passenger)`: the get-up sound.
    fn on_passenger_removed(&mut self, level: &mut ServerLevel, _passenger: EntityId) {
        if !self.is_removed() {
            self.play_sound(level, SoundEvent::ENTITY_CUSHION_GET_UP, 1.0, 1.0);
        }
    }
    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        block_attached::add_additional_save_data(self, out);
        out.put_string("color", self.get_color().name());
    }
    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        block_attached::read_additional_save_data(self, input);
        let color = input
            .get_string("color")
            .and_then(DyeColor::from_name)
            .unwrap_or(DEFAULT_COLOR);
        self.set_color(color);
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static CUSHION_FACTORY: EntityRegistration = EntityRegistration {
    entity_type: EntityType::CUSHION,
    create: |ty, id, _registries| Box::new(Cushion::new(ty, id)),
};
