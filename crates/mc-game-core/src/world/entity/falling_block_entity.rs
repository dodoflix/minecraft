//! `net.minecraft.world.entity.item.FallingBlockEntity`.

use super::entity::{Entity, EntityData, MovementEmission};
use super::factory::{ENTITY_FACTORIES, EntityRegistration};
use super::item::spawn_at_location;
use super::{EntityId, MoverType, movement};
use crate::define_accessor;
use crate::server::level::ServerLevel;
use crate::server::level::server_entity::encode_payload;
use crate::world::damagesource::DamageSource;
use crate::world::item::enchantment::value_codecs::block_state_codec;
use crate::world::item::{BlockPlaceContext, ItemStack};
use crate::world::level::block::{BlockStateExt, behaviour};
use crate::world::level::collision::{BlockGetterExt, ClipBlock, ClipContext, ClipFluid};
use crate::world::level::material::FluidStateExt;
use crate::world::level::{BlockGetter, LevelAccessor};
use linkme::distributed_slice;
use mc_core::direction::Direction;
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::EntityType;
use mc_data::block::{Block, BlockState};
use mc_data::props;
use mc_nbt::{CompoundTag, Tag};
use mc_protocol::packets::play::{
    ClientboundAddEntityPacket, ClientboundAddTransientBlockPacket, ClientboundBlockUpdatePacket,
};
use std::any::Any;

define_accessor!(pub DATA_START_POS: BlockPos = FallingBlockEntity);

const DEFAULT_FALL_DAMAGE_MAX: i32 = 40;

pub struct FallingBlockEntity {
    base: EntityData,
    block_state: BlockState,
    pub time: i32,
    pub drop_item: bool,
    pub cancel_drop: bool,
    pub hurt_entities: bool,
    fall_damage_max: i32,
    fall_damage_per_distance: f32,
    pub block_data: Option<CompoundTag>,
    pub force_tick_after_teleport_to_duplicate: bool,
}

impl FallingBlockEntity {
    /// `FallingBlockEntity(type, level)`.
    pub fn new(ty: EntityType, id: EntityId) -> Self {
        let base = EntityData::new(ty, id, super::entity::TOTAL_AIR_SUPPLY, |b| {
            b.define(DATA_START_POS, BlockPos::new(0, 0, 0));
        });
        Self {
            base,
            block_state: Block::SAND.default_state(),
            time: 0,
            drop_item: true,
            cancel_drop: false,
            hurt_entities: false,
            fall_damage_max: DEFAULT_FALL_DAMAGE_MAX,
            fall_damage_per_distance: 0.0,
            block_data: None,
            force_tick_after_teleport_to_duplicate: false,
        }
    }

    /// The private `FallingBlockEntity(level, x, y, z, blockState)`.
    fn at(level: &ServerLevel, x: f64, y: f64, z: f64, state: BlockState) -> Self {
        let mut e = Self::new(EntityType::FALLING_BLOCK, level.next_entity_id());
        e.block_state = state;
        e.base.blocks_building = true;
        e.set_pos(x, y, z);
        e.base.set_delta_movement(Vec3::ZERO);
        e.base.xo = x;
        e.base.yo = y;
        e.base.zo = z;
        let start = e.base.block_position();
        e.set_start_pos(start);
        e
    }

    /// `fall(level, pos, state)`: the block at `pos` becomes its fluid and a falling entity takes its place.
    /// `before_add` runs on the entity before it enters the level (`FallingBlock.falling(entity)`).
    pub fn fall_with(
        level: &mut ServerLevel,
        pos: BlockPos,
        state: BlockState,
        before_add: impl FnOnce(&mut FallingBlockEntity),
    ) -> EntityId {
        let entity_state = if state.has_property(props::WATERLOGGED) {
            state.set(props::WATERLOGGED, false)
        } else {
            state
        };
        let mut entity = Self::at(
            level,
            f64::from(pos.x()) + 0.5,
            f64::from(pos.y()),
            f64::from(pos.z()) + 0.5,
            entity_state,
        );
        level.set_block_and_update(pos, state.get_fluid_state().create_legacy_block());
        before_add(&mut entity);
        let id = entity.base.id;
        level.add_fresh_entity(Box::new(entity));
        id
    }

    /// `fall(level, pos, state)`.
    pub fn fall(level: &mut ServerLevel, pos: BlockPos, state: BlockState) -> EntityId {
        Self::fall_with(level, pos, state, |_| {})
    }

    pub fn set_start_pos(&mut self, pos: BlockPos) {
        self.base.entity_data.set(DATA_START_POS, pos);
    }

    pub fn get_start_pos(&self) -> BlockPos {
        self.base.entity_data.get(DATA_START_POS)
    }

    pub fn get_block_state(&self) -> BlockState {
        self.block_state
    }

    /// `setHurtsEntities(damagePerDistance, damageMax)`.
    pub fn set_hurts_entities(&mut self, damage_per_distance: f32, damage_max: i32) {
        self.hurt_entities = true;
        self.fall_damage_per_distance = damage_per_distance;
        self.fall_damage_max = damage_max;
    }

    /// `hurtEntities`.
    pub fn hurts_entities(&self) -> bool {
        self.hurt_entities
    }

    /// `cancelDrop`.
    pub fn is_drop_cancelled(&self) -> bool {
        self.cancel_drop
    }

    /// `disableDrop()`.
    pub fn disable_drop(&mut self) {
        self.cancel_drop = true;
    }

    /// `callOnBrokenAfterFall(block, pos)`.
    pub fn call_on_broken_after_fall(&self, level: &mut ServerLevel, block: Block, pos: BlockPos) {
        behaviour(block).on_broken_after_fall(level, pos, self);
    }

    /// `spawnAtLocation(level, block)` (`ItemStack(block)` of the falling block).
    fn spawn_block_item(&self, level: &mut ServerLevel, block: Block) {
        spawn_at_location(
            level,
            self,
            block.item().map_or_else(ItemStack::empty, ItemStack::new),
            Vec3::ZERO,
        );
    }

    fn drops_enabled(&self, level: &ServerLevel) -> bool {
        self.drop_item && level.entity_drops()
    }
}

impl Entity for FallingBlockEntity {
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
        MovementEmission::None
    }
    fn get_default_gravity(&self) -> f64 {
        0.04
    }
    fn is_attackable(&self) -> bool {
        false
    }
    fn is_pickable(&self) -> bool {
        !self.base.is_removed()
    }
    fn move_simulation_type(&self) -> super::MoveSimulationType {
        super::MoveSimulationType::ServerAndClient
    }

    /// `getAddEntityPacket(serverEntity)`: the data is the block state id.
    fn get_add_entity_packet(
        &self,
        server_entity: &crate::server::level::ServerEntity,
    ) -> ClientboundAddEntityPacket {
        server_entity.add_entity_packet(
            self.base.id,
            self.base.uuid,
            self.get_type(),
            self.block_state.id() as i32,
        )
    }

    fn hurt_server(
        &mut self,
        level: &mut ServerLevel,
        source: &DamageSource,
        _damage: f32,
    ) -> bool {
        if !self.is_invulnerable_to_base(level, source) {
            self.mark_hurt();
        }
        false
    }

    fn tick(&mut self, level: &mut ServerLevel) {
        if self.block_state.is_air() {
            self.discard();
            return;
        }
        let block = self.block_state.block();
        self.time += 1;
        self.apply_gravity();
        let dm = self.base.delta_movement();
        self.r#move(level, MoverType::SelfMover, dm);
        self.apply_effects_from_blocks(level);
        self.handle_portal(level);
        if self.is_alive() || self.force_tick_after_teleport_to_duplicate {
            let mut pos = self.base.block_position();
            let is_concrete = crate::world::level::block::helpers::is_instance_of(
                self.block_state,
                "ConcretePowderBlock",
            );
            let mut is_stuck_in_water = is_concrete && level.get_fluid_state(pos).is_tag("water");
            let move_vec = self.base.delta_movement().length_sqr();
            if is_concrete && move_vec > 1.0 {
                let clip = level.clip(&ClipContext::new(
                    Vec3::new(self.base.xo, self.base.yo, self.base.zo),
                    self.base.position(),
                    ClipBlock::Collider,
                    ClipFluid::SourceOnly,
                    movement::collision_context(self),
                ));
                if !clip.miss && level.get_fluid_state(clip.block_pos).is_tag("water") {
                    pos = clip.block_pos;
                    is_stuck_in_water = true;
                }
            }
            if !self.base.on_ground() && !is_stuck_in_water {
                if self.time > 100 && (pos.y() <= level.get_min_y() || pos.y() > level.get_max_y())
                    || self.time > 600
                {
                    if self.drops_enabled(level) {
                        self.spawn_block_item(level, block);
                    }
                    self.discard();
                }
            } else {
                let current_state = level.get_block_state(pos);
                let m = self.base.delta_movement();
                self.base.set_delta_movement(m.multiply_xyz(0.7, -0.5, 0.7));
                if !current_state.is_block(Block::MOVING_PISTON) {
                    if !self.cancel_drop {
                        let may_replace =
                            current_state.can_be_replaced_by(&BlockPlaceContext::directional(
                                pos,
                                Direction::Down,
                                ItemStack::empty(),
                                Direction::Up,
                                current_state,
                            ));
                        let would_continue_falling =
                            crate::world::level::block::impls::falling_block::is_free(
                                level.get_block_state(pos.below()),
                            ) && (!is_concrete || !is_stuck_in_water);
                        let would_survive =
                            self.block_state.can_survive(level, pos) && !would_continue_falling;
                        if may_replace && would_survive {
                            if self.block_state.has_property(props::WATERLOGGED)
                                && level.get_fluid_state(pos).get_type()
                                    == mc_data::fluid::Fluid::WATER
                            {
                                self.block_state = self.block_state.set(props::WATERLOGGED, true);
                            }
                            if level.set_block_and_update(pos, self.block_state) {
                                let placed = level.get_block_state(pos);
                                let id = self.base.id;
                                level.entity_tracker.send_to_tracking_players(
                                    id,
                                    &encode_payload(&ClientboundBlockUpdatePacket {
                                        pos,
                                        block_state: placed,
                                    }),
                                );
                                level.entity_tracker.send_to_tracking_players(
                                    id,
                                    &encode_payload(&ClientboundAddTransientBlockPacket {
                                        pos,
                                        block_state: placed,
                                    }),
                                );
                                self.discard();
                                behaviour(block).on_land(
                                    level,
                                    pos,
                                    self.block_state,
                                    current_state,
                                    self,
                                );
                                if let Some(block_data) = &self.block_data
                                    && self.block_state.has_block_entity()
                                {
                                    let changed = level
                                        .with_block_entity(pos, |_, be| {
                                            let mut merged = CompoundTag::new();
                                            be.save_without_metadata(&mut merged);
                                            for (name, tag) in block_data.iter() {
                                                merged.put(name, tag.clone());
                                            }
                                            be.load_with_components(&merged);
                                        })
                                        .is_some();
                                    if changed {
                                        level.block_entity_changed(pos);
                                    }
                                }
                            } else if self.drops_enabled(level) {
                                self.discard();
                                self.call_on_broken_after_fall(level, block, pos);
                                self.spawn_block_item(level, block);
                            }
                        } else {
                            self.discard();
                            if self.drops_enabled(level) {
                                self.call_on_broken_after_fall(level, block, pos);
                                self.spawn_block_item(level, block);
                            }
                        }
                    } else {
                        self.discard();
                        self.call_on_broken_after_fall(level, block, pos);
                    }
                }
            }
        }
        let m = self.base.delta_movement();
        self.base
            .set_delta_movement(m.scale(f64::from(self.get_air_drag())));
        self.base.requires_precise_position =
            self.base.horizontal_collision || self.base.vertical_collision;
    }

    fn cause_fall_damage(
        &mut self,
        level: &mut ServerLevel,
        fall_distance: f64,
        _damage_modifier: f32,
        _source: &DamageSource,
    ) -> bool {
        if !self.hurt_entities {
            return false;
        }
        let fall_distance_int = mth::ceil_f64(fall_distance - 1.0);
        if fall_distance_int < 0 {
            return false;
        }
        let actual_source = behaviour(self.block_state.block()).get_fall_damage_source(level, self);
        let damage = mth::floor_f32(fall_distance_int as f32 * self.fall_damage_per_distance)
            .min(self.fall_damage_max) as f32;
        let bb = self.base.bounding_box();
        // `EntitySelector.NO_CREATIVE_OR_SPECTATOR.and(LIVING_ENTITY_STILL_ALIVE)`
        let victims = level.get_entities(Some(self.base.id), &bb, |e| {
            let not_creative_or_spectator = !(e.is_player()
                && (e.is_spectator() || e.player().is_some_and(|p| p.abilities.instabuild)));
            not_creative_or_spectator && e.is_alive() && e.living().is_some()
        });
        for id in victims {
            level.with_entity(id, |level, e| e.hurt_server(level, &actual_source, damage));
        }
        let is_anvil = self.block_state.is_tag("anvil");
        if is_anvil
            && damage > 0.0
            && self.base.random.next_float() < 0.05f32 + fall_distance_int as f32 * 0.05f32
        {
            match crate::world::level::block::impls::anvil_block::damage(self.block_state) {
                None => self.cancel_drop = true,
                Some(new_state) => self.block_state = new_state,
            }
        }
        false
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        // `BlockState.CODEC`: a bare block id for the default state, else `{id, properties}`
        if let Some(tag) = block_state_codec()
            .encode_start(&mc_dfu::NBT, &self.block_state)
            .result()
            .and_then(|d| d.to_tag().ok())
        {
            out.put("BlockState", tag);
        }
        out.put_int("Time", self.time);
        out.put_boolean("DropItem", self.drop_item);
        out.put_boolean("HurtEntities", self.hurt_entities);
        out.put_float("FallHurtAmount", self.fall_damage_per_distance);
        out.put_int("FallHurtMax", self.fall_damage_max);
        if let Some(data) = &self.block_data {
            out.put("TileEntityData", Tag::Compound(data.clone()));
        }
        out.put_boolean("CancelDrop", self.cancel_drop);
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        self.block_state = input
            .get("BlockState")
            .and_then(|t| {
                block_state_codec()
                    .parse(&mc_dfu::NBT, &mc_dfu::Dyn::from(t))
                    .result()
            })
            .unwrap_or_else(|| Block::SAND.default_state());
        self.time = input.get_int_or("Time", 0);
        let default_hurt_entities = self.block_state.is_tag("anvil");
        self.hurt_entities = input.get_boolean_or("HurtEntities", default_hurt_entities);
        self.fall_damage_per_distance = input.get_float_or("FallHurtAmount", 0.0);
        self.fall_damage_max = input.get_int_or("FallHurtMax", DEFAULT_FALL_DAMAGE_MAX);
        self.drop_item = input.get_boolean_or("DropItem", true);
        self.block_data = input.get_compound("TileEntityData").cloned();
        self.cancel_drop = input.get_boolean_or("CancelDrop", false);
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static FALLING_BLOCK_FACTORY: EntityRegistration = EntityRegistration {
    entity_type: EntityType::FALLING_BLOCK,
    create: |ty, id, _registries| Box::new(FallingBlockEntity::new(ty, id)),
};
