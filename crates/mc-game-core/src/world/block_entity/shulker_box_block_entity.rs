//! `ShulkerBoxBlockEntity`: contents, open counter and the lid animation state machine (ticked on the server: the lid
//! progress decides the collision shape, entity pushing and the neighbor updates at the ends of the animation).

use super::base_container::BaseContainer;
use super::randomizable_container::{self, RandomizableContainerData};
use super::{
    BLOCK_ENTITY_TYPES, BlockEntity, BlockEntityData, BlockEntityRegistration, ContainerUser,
};
use crate::server::level::ServerLevel;
use crate::world::container::{Container, helper};
use crate::world::entity::{Entity, EntityId, MoverType};
use crate::world::item::ItemStack;
use crate::world::level::block::BlockStateExt;
use crate::world::level::gameevent::GameEventContext;
use crate::world::level::{BlockGetter, LevelAccessor};
use linkme::distributed_slice;
use mc_core::direction::Direction;
use mc_core::math::mth;
use mc_core::phys::{AABB, Vec3};
use mc_core::pos::BlockPos;
use mc_data::block::BlockState;
use mc_data::{BlockEntityType, GameEvent, SoundEvent, props};
use mc_nbt::CompoundTag;
use mc_protocol::components::map::DataComponentMapBuilder;
use mc_protocol::packets::play::level::SoundSource;
use mc_text::Component;

/// `ShulkerBoxBlockEntity.AnimationStatus`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AnimationStatus {
    Closed,
    Opening,
    Opened,
    Closing,
}

pub struct ShulkerBoxBlockEntity {
    base: BlockEntityData,
    pub container: BaseContainer,
    item_stacks: Vec<ItemStack>,
    loot: RandomizableContainerData,
    open_count: i32,
    animation_status: AnimationStatus,
    progress: f32,
    progress_old: f32,
}

/// `Shulker.getProgressDeltaAabb(size, direction, progressFrom, progressTo, position)`.
pub fn get_progress_delta_aabb(
    size: f32,
    direction: Direction,
    progress_from: f32,
    progress_to: f32,
    position: Vec3,
) -> AABB {
    let size = f64::from(size);
    let bounds_at_bottom_center =
        AABB::new(-size * 0.5, 0.0, -size * 0.5, size * 0.5, size, size * 0.5);
    let max_movement = f64::from(progress_from.max(progress_to));
    let min_movement = f64::from(progress_from.min(progress_to));
    let (sx, sy, sz) = (
        f64::from(direction.get_step_x()),
        f64::from(direction.get_step_y()),
        f64::from(direction.get_step_z()),
    );
    bounds_at_bottom_center
        .expand_towards(
            sx * max_movement * size,
            sy * max_movement * size,
            sz * max_movement * size,
        )
        .contract(
            -sx * (1.0 + min_movement) * size,
            -sy * (1.0 + min_movement) * size,
            -sz * (1.0 + min_movement) * size,
        )
        .move_(position.x, position.y, position.z)
}

/// `Shulker.getProgressAabb(size, direction, progressTo, position)`.
pub fn get_progress_aabb(
    size: f32,
    direction: Direction,
    progress_to: f32,
    position: Vec3,
) -> AABB {
    get_progress_delta_aabb(size, direction, -1.0, progress_to, position)
}

/// `Entity.getPistonPushReaction() == IGNORE_ENTITY` for the entity types that override it.
fn ignores_piston_push(entity: &dyn Entity) -> bool {
    use mc_data::EntityType;
    let ty = entity.get_type();
    ty == EntityType::AREA_EFFECT_CLOUD
        || ty == EntityType::BLOCK_DISPLAY
        || ty == EntityType::ITEM_DISPLAY
        || ty == EntityType::TEXT_DISPLAY
        || ty == EntityType::INTERACTION
        || ty == EntityType::MARKER
        || ty == EntityType::OMINOUS_ITEM_SPAWNER
}

impl ShulkerBoxBlockEntity {
    pub const CONTAINER_SIZE: usize = 27;

    pub fn new(pos: BlockPos, state: BlockState) -> Self {
        Self {
            base: BlockEntityData::new(BlockEntityType::SHULKER_BOX, pos, state),
            container: BaseContainer::default(),
            item_stacks: vec![ItemStack::empty(); Self::CONTAINER_SIZE],
            loot: RandomizableContainerData::default(),
            open_count: 0,
            animation_status: AnimationStatus::Closed,
            progress: 0.0,
            progress_old: 0.0,
        }
    }

    pub fn default_name() -> Component {
        Component::translatable("container.shulkerBox", vec![])
    }

    pub fn get_display_name(&self) -> Component {
        self.container.get_name(Self::default_name())
    }

    pub fn can_open(&self, player: &dyn crate::world::inventory::MenuPlayer) -> bool {
        (self.loot.loot_table.is_none() || !player.is_spectator())
            && self.container.can_open(player)
    }

    pub fn get_animation_status(&self) -> AnimationStatus {
        self.animation_status
    }

    pub fn is_closed(&self) -> bool {
        self.animation_status == AnimationStatus::Closed
    }

    pub fn get_progress(&self, a: f32) -> f32 {
        mth::lerp_f32(a, self.progress_old, self.progress)
    }

    /// `getBoundingBox(state)`.
    pub fn get_bounding_box(&self, state: BlockState) -> AABB {
        let bottom_center = Vec3::new(0.5, 0.0, 0.5);
        get_progress_aabb(
            1.0,
            state.get(props::FACING),
            0.5 * self.get_progress(1.0),
            bottom_center,
        )
    }

    pub fn unpack(
        &mut self,
        level: &ServerLevel,
        player: Option<&dyn crate::world::entity::Entity>,
    ) {
        let mut loot = std::mem::take(&mut self.loot);
        let pos = self.base.pos;
        randomizable_container::unpack_loot_table(level, pos, &mut loot, self, player);
        self.loot = loot;
    }

    /// `updateAnimation(level, pos, blockState)`.
    fn update_animation(&mut self, level: &mut ServerLevel, pos: BlockPos, state: BlockState) {
        self.progress_old = self.progress;
        match self.animation_status {
            AnimationStatus::Closed => self.progress = 0.0,
            AnimationStatus::Opening => {
                self.progress += 0.1;
                if self.progress_old == 0.0 {
                    do_neighbor_updates(level, pos, state);
                }
                if self.progress >= 1.0 {
                    self.animation_status = AnimationStatus::Opened;
                    self.progress = 1.0;
                    do_neighbor_updates(level, pos, state);
                }
                self.move_collided_entities(level, pos, state);
            }
            AnimationStatus::Opened => self.progress = 1.0,
            AnimationStatus::Closing => {
                self.progress -= 0.1;
                if self.progress_old == 1.0 {
                    do_neighbor_updates(level, pos, state);
                }
                if self.progress <= 0.0 {
                    self.animation_status = AnimationStatus::Closed;
                    self.progress = 0.0;
                    do_neighbor_updates(level, pos, state);
                }
            }
        }
    }

    /// `moveCollidedEntities(level, pos, state)`. The pushes are deferred until the entity is back in its chunk: the
    /// entities collide with this box's own (animated) shape.
    fn move_collided_entities(&self, level: &ServerLevel, pos: BlockPos, state: BlockState) {
        if !state.block().data().classes.contains(&"ShulkerBoxBlock") {
            return;
        }
        let direction = state.get(props::FACING);
        let aabb = get_progress_delta_aabb(
            1.0,
            direction,
            self.progress_old,
            self.progress,
            Vec3::at_bottom_center_of(&pos),
        );
        let entities = level.get_entities(None, &aabb, |e| !e.is_spectator());
        if entities.is_empty() {
            return;
        }
        let delta = Vec3::new(
            (aabb.get_xsize() + 0.01) * f64::from(direction.get_step_x()),
            (aabb.get_ysize() + 0.01) * f64::from(direction.get_step_y()),
            (aabb.get_zsize() + 0.01) * f64::from(direction.get_step_z()),
        );
        super::deferred::push(move |level| {
            for id in entities {
                level.with_entity(id, |level, entity| {
                    if !ignores_piston_push(&*entity) {
                        entity.r#move(level, MoverType::ShulkerBox, delta);
                    }
                });
            }
        });
    }
}

/// `doNeighborUpdates(level, pos, blockState)`.
fn do_neighbor_updates(level: &mut ServerLevel, pos: BlockPos, state: BlockState) {
    state.update_neighbour_shapes(level, pos, 3, 512);
    level.update_neighbors_at(pos, state.block());
}

impl_items_container!(ShulkerBoxBlockEntity, item_stacks);

impl BlockEntity for ShulkerBoxBlockEntity {
    block_entity_base!();

    fn load_additional(&mut self, tag: &CompoundTag) {
        self.container.load(tag);
        self.load_from_tag(tag);
    }

    fn save_additional(&self, tag: &mut CompoundTag) {
        self.container.save(tag);
        if !self.loot.try_save_loot_table(tag) {
            if let Some(list) = helper::save_all_items(&self.item_stacks, false) {
                tag.put("Items", mc_nbt::Tag::List(list));
            }
        }
    }

    fn apply_components_from_item_stack(&mut self, stack: &ItemStack) {
        self.container
            .apply_implicit_components(stack, &mut self.item_stacks);
        if let Some(loot) = stack.get(&mc_protocol::components::CONTAINER_LOOT) {
            self.loot.set_loot_table(loot.loot_table.clone(), loot.seed);
        }
    }

    fn collect_implicit_components(&self, builder: &mut DataComponentMapBuilder) {
        *builder = self
            .container
            .collect_implicit_components(std::mem::take(builder), &self.item_stacks);
        if let Some(table) = &self.loot.loot_table {
            *builder = std::mem::take(builder).set(
                mc_protocol::components::CONTAINER_LOOT,
                mc_protocol::components::SeededContainerLoot {
                    loot_table: table.clone(),
                    seed: self.loot.loot_table_seed,
                },
            );
        }
    }

    fn get_custom_name(&self) -> Option<Component> {
        self.container.get_custom_name().cloned()
    }

    fn is_ticking(&self) -> bool {
        true
    }

    fn server_tick(&mut self, level: &mut ServerLevel) {
        let (pos, state) = (self.base.pos, self.base.block_state);
        self.update_animation(level, pos, state);
    }

    fn trigger_event(&mut self, _level: &mut ServerLevel, b0: i32, b1: i32) -> bool {
        if b0 == 1 {
            self.open_count = b1;
            if b1 == 0 {
                self.animation_status = AnimationStatus::Closing;
            }
            if b1 == 1 {
                self.animation_status = AnimationStatus::Opening;
            }
            true
        } else {
            false
        }
    }

    /// `preRemoveSideEffects`: shulker boxes drop through their loot table (`contents` dynamic drop), not here.
    fn pre_remove_side_effects(
        &mut self,
        _level: &mut ServerLevel,
        _pos: BlockPos,
        _state: BlockState,
    ) {
    }

    fn as_container(&self) -> Option<&dyn Container> {
        Some(self)
    }

    fn as_container_mut(&mut self) -> Option<&mut dyn Container> {
        Some(self)
    }

    fn container_pending(&self) -> bool {
        self.base.changed
    }

    fn container_accessed(&mut self, level: &mut ServerLevel) {
        self.base.changed = false;
        super::set_changed(level, self.base.pos, self.base.block_state);
    }

    fn start_open(&mut self, level: &mut ServerLevel, user: &ContainerUser) {
        if !self.base.removed && !user.spectator {
            if self.open_count < 0 {
                self.open_count = 0;
            }
            self.open_count += 1;
            let (pos, state) = (self.base.pos, self.base.block_state);
            level.block_event(pos, state.block(), 1, self.open_count);
            if self.open_count == 1 {
                level.game_event_at_block(
                    GameEvent::CONTAINER_OPEN,
                    pos,
                    &GameEventContext::of_entity(Some(user.id)),
                );
                let pitch = LevelAccessor::random(level).next_float() * 0.1 + 0.9;
                level.play_sound(
                    None,
                    pos,
                    SoundEvent::BLOCK_SHULKER_BOX_OPEN,
                    SoundSource::Blocks,
                    0.5,
                    pitch,
                );
            }
        }
    }

    fn stop_open(&mut self, level: &mut ServerLevel, user: &ContainerUser) {
        if !self.base.removed && !user.spectator {
            self.open_count -= 1;
            let (pos, state) = (self.base.pos, self.base.block_state);
            level.block_event(pos, state.block(), 1, self.open_count);
            if self.open_count <= 0 {
                level.game_event_at_block(
                    GameEvent::CONTAINER_CLOSE,
                    pos,
                    &GameEventContext::of_entity(Some(user.id)),
                );
                let pitch = LevelAccessor::random(level).next_float() * 0.1 + 0.9;
                level.play_sound(
                    None,
                    pos,
                    SoundEvent::BLOCK_SHULKER_BOX_CLOSE,
                    SoundSource::Blocks,
                    0.5,
                    pitch,
                );
            }
        }
    }

    fn unpack_loot_table(
        &mut self,
        level: &ServerLevel,
        player: Option<&dyn crate::world::entity::Entity>,
    ) {
        self.unpack(level, player);
    }

    fn set_loot_table(&mut self, table: mc_core::ident::Identifier, seed: i64) {
        self.loot.set_loot_table(table, seed);
    }
}

impl ShulkerBoxBlockEntity {
    /// `loadFromTag(input)`.
    pub fn load_from_tag(&mut self, tag: &CompoundTag) {
        self.item_stacks = vec![ItemStack::empty(); Self::CONTAINER_SIZE];
        if !self.loot.try_load_loot_table(tag) {
            helper::load_all_items(tag, &mut self.item_stacks);
        }
    }
}

#[distributed_slice(BLOCK_ENTITY_TYPES)]
static SHULKER_BOX: BlockEntityRegistration = BlockEntityRegistration {
    ty: BlockEntityType::SHULKER_BOX,
    create: |pos, state| Box::new(ShulkerBoxBlockEntity::new(pos, state)),
};

/// `ShulkerBoxBlock.canOpen(state, level, pos, blockEntity)` helper: whether the lid has room to open (`noCollision`).
pub fn lid_open_box(state: BlockState, pos: BlockPos) -> AABB {
    get_progress_delta_aabb(
        1.0,
        state.get(props::FACING),
        0.0,
        0.5,
        Vec3::at_bottom_center_of(&pos),
    )
    .deflate_scalar(1.0E-6)
}

/// `getBlockEntity` helper for blocks: the shulker box entity at `pos`.
pub fn shulker_at(level: &dyn BlockGetter, pos: BlockPos) -> Option<&ShulkerBoxBlockEntity> {
    level
        .get_block_entity(pos)?
        .as_any()
        .downcast_ref::<ShulkerBoxBlockEntity>()
}
