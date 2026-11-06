//! `net.minecraft.world.level.block.ChestBlock` (extends `AbstractChestBlock`), `TrappedChestBlock` and, through
//! [`ChestKind::Copper`], `CopperChestBlock` / `WeatheringCopperChestBlock` (see `copper_chest_block`).
use super::copper_chest_block;
use super::prelude::*;
use crate::server::level::ServerLevel;
use crate::world::block_entity::chest_block_entity::ChestBlockEntity;
use crate::world::item::BlockPlaceContext;
use crate::world::level::block::double_block_combiner::{self, BlockType, NeighborCombineResult};
use mc_core::phys::AABB;
use mc_data::BlockEntityType;
use mc_data::SoundEvent;
use mc_data::enums::ChestType;
use mc_data::fluid::Fluid;

/// Which Java class a registered block is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ChestKind {
    Chest,
    /// `TrappedChestBlock`.
    Trapped,
    /// `CopperChestBlock` / `WeatheringCopperChestBlock`.
    Copper,
}

pub struct ChestBlock {
    pub kind: ChestKind,
}

/// `ChestType.getOpposite`.
fn opposite(ty: ChestType) -> ChestType {
    match ty {
        ChestType::Single => ChestType::Single,
        ChestType::Left => ChestType::Right,
        ChestType::Right => ChestType::Left,
    }
}

/// `ChestBlock.getConnectedDirection`.
pub fn get_connected_direction(state: BlockState) -> Direction {
    let facing = state.get(props::HORIZONTAL_FACING);
    if state.get(props::CHEST_TYPE) == ChestType::Left {
        facing.get_clockwise()
    } else {
        facing.get_counter_clockwise()
    }
}

/// `ChestBlock.getConnectedBlockPos`.
pub fn get_connected_block_pos(pos: BlockPos, state: BlockState) -> BlockPos {
    pos.relative(get_connected_direction(state))
}

/// `ChestBlock.getBlockType`.
pub fn get_block_type(state: BlockState) -> BlockType {
    match state.get(props::CHEST_TYPE) {
        ChestType::Single => BlockType::Single,
        ChestType::Right => BlockType::First,
        ChestType::Left => BlockType::Second,
    }
}

/// `ChestBlock.chestCanConnectTo(blockState)` for the chest class `kind` of block `this`.
fn chest_can_connect_to(kind: ChestKind, this: Block, state: BlockState) -> bool {
    match kind {
        ChestKind::Copper => copper_chest_block::chest_can_connect_to(state),
        _ => state.block() == this,
    }
}

/// `ChestBlock.updateShape` (`can_connect` is the virtual `chestCanConnectTo`).
pub fn update_shape(
    state: BlockState,
    level: &mut dyn LevelAccessor,
    pos: BlockPos,
    direction_to_neighbour: Direction,
    neighbour_state: BlockState,
    can_connect: &dyn Fn(BlockState) -> bool,
) -> BlockState {
    if state.get(props::WATERLOGGED) {
        helpers::schedule_water_tick(level, pos);
    }
    if can_connect(neighbour_state) && direction_to_neighbour.get_axis().is_horizontal() {
        let neighbour_type = neighbour_state.get(props::CHEST_TYPE);
        if state.get(props::CHEST_TYPE) == ChestType::Single
            && neighbour_type != ChestType::Single
            && state.get(props::HORIZONTAL_FACING) == neighbour_state.get(props::HORIZONTAL_FACING)
            && get_connected_direction(neighbour_state) == direction_to_neighbour.get_opposite()
        {
            return state.set(props::CHEST_TYPE, opposite(neighbour_type));
        }
    } else if get_connected_direction(state) == direction_to_neighbour {
        return state.set(props::CHEST_TYPE, ChestType::Single);
    }
    state
}

/// `candidatePartnerFacing(level, pos, neighbourDirection)`.
fn candidate_partner_facing(
    kind: ChestKind,
    this: Block,
    level: &dyn BlockGetter,
    pos: BlockPos,
    neighbour_direction: Direction,
) -> Option<Direction> {
    let state = level.get_block_state(pos.relative(neighbour_direction));
    if chest_can_connect_to(kind, this, state) && state.get(props::CHEST_TYPE) == ChestType::Single
    {
        Some(state.get(props::HORIZONTAL_FACING))
    } else {
        None
    }
}

/// `getChestType(level, pos, facingDirection)`.
pub fn get_chest_type(
    kind: ChestKind,
    this: Block,
    level: &dyn BlockGetter,
    pos: BlockPos,
    facing_direction: Direction,
) -> ChestType {
    if Some(facing_direction)
        == candidate_partner_facing(kind, this, level, pos, facing_direction.get_clockwise())
    {
        ChestType::Left
    } else if Some(facing_direction)
        == candidate_partner_facing(
            kind,
            this,
            level,
            pos,
            facing_direction.get_counter_clockwise(),
        )
    {
        ChestType::Right
    } else {
        ChestType::Single
    }
}

/// `ChestBlock.isChestBlockedAt(level, pos)`.
pub fn is_chest_blocked_at(level: &ServerLevel, pos: BlockPos) -> bool {
    is_blocked_chest_by_block(level, pos) || is_cat_sitting_on_chest(level, pos)
}

fn is_blocked_chest_by_block(level: &dyn BlockGetter, pos: BlockPos) -> bool {
    level.get_block_state(pos.above()).is_redstone_conductor()
}

fn is_cat_sitting_on_chest(level: &ServerLevel, pos: BlockPos) -> bool {
    let (x, y, z) = (f64::from(pos.x()), f64::from(pos.y()), f64::from(pos.z()));
    let area = AABB::new(x, y + 1.0, z, x + 1.0, y + 2.0, z + 1.0);
    !level
        .get_entities_of_class(
            &area,
            |e| e.get_type() == mc_data::EntityType::CAT,
            |e| e.is_in_sitting_pose(),
        )
        .is_empty()
}

/// `AbstractChestBlock.combine(state, level, pos, ignoreBeingBlocked)` of a chest block (`ChestBlock.combine`).
pub fn combine(
    level: &ServerLevel,
    state: BlockState,
    pos: BlockPos,
    ignore_being_blocked: bool,
) -> NeighborCombineResult {
    let has_entity = |level: &ServerLevel, pos: BlockPos| {
        level
            .get_block_entity(pos)
            .is_some_and(|be| be.as_any().is::<ChestBlockEntity>())
    };
    let blocked = |level: &ServerLevel, pos: BlockPos| {
        !ignore_being_blocked && is_chest_blocked_at(level, pos)
    };
    double_block_combiner::combine_with_neighbour(
        &has_entity,
        get_block_type,
        get_connected_direction,
        props::HORIZONTAL_FACING,
        state,
        level,
        pos,
        &blocked,
    )
}

/// The chest block entities `result` refers to (`first`, optional `second`).
pub fn combined_positions(result: NeighborCombineResult) -> Option<(BlockPos, Option<BlockPos>)> {
    match result {
        NeighborCombineResult::None => None,
        NeighborCombineResult::Single(pos) => Some((pos, None)),
        NeighborCombineResult::Double { first, second } => Some((first, Some(second))),
    }
}

/// `AbstractContainerMenu.getRedstoneSignalFromContainer(ChestBlock.getContainer(block, state, level, pos, false))`:
/// the `CompoundContainer` of a double chest has 54 slots and the first chest's max stack size.
pub fn get_container_signal(level: &ServerLevel, state: BlockState, pos: BlockPos) -> i32 {
    let Some((first, second)) = combined_positions(combine(level, state, pos, false)) else {
        return 0;
    };
    let chests: Vec<&ChestBlockEntity> = [Some(first), second]
        .into_iter()
        .flatten()
        .filter_map(|p| {
            level
                .get_block_entity(p)?
                .as_any()
                .downcast_ref::<ChestBlockEntity>()
        })
        .collect();
    if chests.is_empty() {
        return 0;
    }
    let size: usize = chests.len() * ChestBlockEntity::CONTAINER_SIZE;
    let mut total_percent = 0.0f32;
    for chest in &chests {
        for stack in chest.items() {
            if !stack.is_empty() {
                total_percent += stack.count() as f32 / stack.get_max_stack_size().min(99) as f32;
            }
        }
    }
    total_percent /= size as f32;
    mc_core::math::mth::lerp_discrete(total_percent, 0, 15)
}

/// `chest instanceof ChestBlockEntity && chest.getEntitiesWithContainerOpen().isEmpty()` (`WeatheringCopperChestBlock.randomTick`).
pub fn chest_idle(level: &ServerLevel, pos: BlockPos) -> bool {
    level
        .get_block_entity(pos)
        .and_then(|be| be.as_any().downcast_ref::<ChestBlockEntity>())
        .is_some_and(|c| c.entities_with_container_open(level).is_empty())
}

/// `ChestBlock.getOpenChestSound()` / `getCloseChestSound()` of the chest block `block`: `None` when it is not a chest.
pub fn chest_sounds(block: Block) -> Option<(SoundEvent, SoundEvent)> {
    let data = block.data();
    if !data.classes.contains(&"ChestBlock") {
        return None;
    }
    let name = data.name.strip_prefix("minecraft:").unwrap_or(data.name);
    Some(match copper_chest_block::weather_state(name) {
        _ if !name.contains("copper_chest") => {
            (SoundEvent::BLOCK_CHEST_OPEN, SoundEvent::BLOCK_CHEST_CLOSE)
        }
        // `CopperChestBlock.getHingeSound`
        2 => (
            SoundEvent::BLOCK_COPPER_CHEST_WEATHERED_OPEN,
            SoundEvent::BLOCK_COPPER_CHEST_WEATHERED_CLOSE,
        ),
        3 => (
            SoundEvent::BLOCK_COPPER_CHEST_OXIDIZED_OPEN,
            SoundEvent::BLOCK_COPPER_CHEST_OXIDIZED_CLOSE,
        ),
        _ => (
            SoundEvent::BLOCK_COPPER_CHEST_OPEN,
            SoundEvent::BLOCK_COPPER_CHEST_CLOSE,
        ),
    })
}

impl BlockBehaviour for ChestBlock {
    crate::simple_waterlogged!();
    fn get_state_for_placement(
        &self,
        block: Block,
        level: &mut ServerLevel,
        context: &BlockPlaceContext,
    ) -> Option<BlockState> {
        let mut ty = ChestType::Single;
        let mut facing_direction = context.get_horizontal_direction().get_opposite();
        let replaced_fluid_state = level.get_fluid_state(context.get_clicked_pos());
        let secondary_use = context.is_secondary_use_active();
        let clicked_face = context.get_clicked_face();
        let pos = context.get_clicked_pos();
        if clicked_face.get_axis().is_horizontal() && secondary_use {
            let neighbour_facing = candidate_partner_facing(
                self.kind,
                block,
                &*level,
                pos,
                clicked_face.get_opposite(),
            );
            if let Some(neighbour_facing) = neighbour_facing
                && neighbour_facing.get_axis() != clicked_face.get_axis()
            {
                facing_direction = neighbour_facing;
                ty = if neighbour_facing.get_counter_clockwise() == clicked_face.get_opposite() {
                    ChestType::Right
                } else {
                    ChestType::Left
                };
            }
        }
        if ty == ChestType::Single && !secondary_use {
            ty = get_chest_type(self.kind, block, &*level, pos, facing_direction);
        }
        let state = block
            .default_state()
            .set(props::HORIZONTAL_FACING, facing_direction)
            .set(props::CHEST_TYPE, ty)
            .set(
                props::WATERLOGGED,
                replaced_fluid_state.fluid() == Fluid::WATER,
            );
        Some(if self.kind == ChestKind::Copper {
            copper_chest_block::get_least_oxidized_chest_of_connected_blocks(state, &*level, pos)
        } else {
            state
        })
    }

    update_shape!(
        |self, state, level, pos, direction, neighbour_pos, neighbour_state| {
            let this = state.block();
            let kind = self.kind;
            let updated = update_shape(state, level, pos, direction, neighbour_state, &|n| {
                chest_can_connect_to(kind, this, n)
            });
            if kind == ChestKind::Copper {
                copper_chest_block::update_shape_tail(updated, direction, neighbour_state)
            } else {
                updated
            }
        }
    );

    fn use_without_item(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        player: &mut crate::world::entity::ServerPlayer,
        _hit: &mc_core::phys::BlockHitResult,
    ) -> crate::world::item::InteractionResult {
        use crate::world::block_entity::container_menus::{
            BlockEntityMenuProvider, ContainerBlockKind,
        };
        if let Some((first, second)) = combined_positions(combine(level, state, pos, false)) {
            let name_of = |p: BlockPos| {
                level
                    .get_block_entity(p)
                    .and_then(|be| be.as_any().downcast_ref::<ChestBlockEntity>())
                    .map(|c| (c.container.get_custom_name().cloned(), c.get_display_name()))
            };
            let title = match second {
                None => name_of(first)
                    .map(|n| n.1)
                    .unwrap_or_else(ChestBlockEntity::default_name),
                Some(second) => match (name_of(first), name_of(second)) {
                    (Some((Some(_), n)), _) => n,
                    (_, Some((Some(_), n))) => n,
                    _ => mc_text::Component::translatable("container.chestDouble", vec![]),
                },
            };
            let provider = BlockEntityMenuProvider {
                kind: ContainerBlockKind::Chest,
                first,
                second,
                title,
            };
            if player.open_menu(level, &provider).is_some() {
                let stat = if self.kind == ChestKind::Trapped {
                    crate::stats::stats::Stats::TRIGGER_TRAPPED_CHEST
                } else {
                    crate::stats::stats::Stats::OPEN_CHEST
                };
                player.award_stat_once(&*level, stat);
            }
        }
        crate::world::item::InteractionResult::SUCCESS
    }

    fn affect_neighbors_after_removal(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        _moved_by_piston: bool,
    ) {
        // `Containers.updateNeighboursAfterDestroy`
        level.update_neighbour_for_output_signal(pos, state.block());
    }

    fn trigger_event(
        &self,
        _state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        b0: i32,
        b1: i32,
    ) -> bool {
        helpers::base_entity_trigger_event(level, pos, b0, b1)
    }

    fn tick(&self, _state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        level.with_block_entity(pos, |level, be| {
            if let Some(chest) = be.as_any_mut().downcast_mut::<ChestBlockEntity>() {
                chest.recheck_open(level);
            }
        });
    }

    fn get_analog_output_signal(
        &self,
        state: BlockState,
        level: &ServerLevel,
        pos: BlockPos,
        _direction: Direction,
    ) -> i32 {
        get_container_signal(level, state, pos)
    }

    fn own_signal(&self, _state: BlockState, level: &dyn BlockGetter, pos: BlockPos) -> i32 {
        if self.kind == ChestKind::Trapped {
            // `TrappedChestBlock.ownSignal`: `Mth.clamp(ChestBlockEntity.getOpenCount(level, pos), 0, 15)`
            ChestBlockEntity::get_open_count(level, pos).clamp(0, 15)
        } else {
            0
        }
    }

    fn get_direct_signal(
        &self,
        state: BlockState,
        level: &dyn BlockGetter,
        pos: BlockPos,
        direction: Direction,
    ) -> i32 {
        if self.kind == ChestKind::Trapped && direction == Direction::Up {
            state.get_signal(level, pos, direction)
        } else {
            0
        }
    }

    fn should_changed_state_keep_block_entity(&self, old_state: BlockState) -> bool {
        self.kind == ChestKind::Copper && old_state.is_tag("copper_chests")
    }

    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }

    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::HORIZONTAL_FACING, mirror)
    }
}

/// `BlockEntityType` of the chest blocks (`AbstractChestBlock.blockEntityType`).
pub fn block_entity_type(block: Block) -> BlockEntityType {
    if block.data().classes.contains(&"TrappedChestBlock") {
        BlockEntityType::TRAPPED_CHEST
    } else {
        BlockEntityType::CHEST
    }
}

register_block_class!("ChestBlock", |_| Box::new(ChestBlock {
    kind: ChestKind::Chest
}));
register_block_class!("TrappedChestBlock", |_| Box::new(ChestBlock {
    kind: ChestKind::Trapped
}));
