//! `net.minecraft.world.level.block.BubbleColumnBlock`.
use mc_data::fluid::Fluid;
use mc_data::{Item, SoundEvent};
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::Entity;
use mc_game_core::world::item::ItemStack;
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::{register_block_class, update_shape};

pub struct BubbleColumnBlock;

/// `BubbleColumnBlock.canOccupy`.
fn can_occupy(bubble_column: Block, occupy_state: BlockState) -> bool {
    if occupy_state.is_block(bubble_column) {
        true
    } else {
        let occupy_fluid = occupy_state.get_fluid_state();
        occupy_fluid.is_tag("bubble_column_can_occupy")
            && helpers::is_instance_of(occupy_state, "LiquidBlock")
            && occupy_fluid.is_source()
            && occupy_fluid.amount() >= 8
    }
}

/// `getColumnState`.
fn get_column_state(
    bubble_column: Block,
    below_state: BlockState,
    occupy_state: BlockState,
) -> BlockState {
    if below_state.is_block(bubble_column) {
        below_state
    } else if below_state.is_tag("enables_bubble_column_push_up") {
        bubble_column.default_state().set(props::DRAG, false)
    } else if below_state.is_tag("enables_bubble_column_drag_down") {
        bubble_column.default_state().set(props::DRAG, true)
    } else if occupy_state.is_block(bubble_column) {
        Block::WATER.default_state()
    } else {
        occupy_state
    }
}

/// `updateColumn(bubbleColumn, level, occupyAt, belowState)`.
pub fn update_column(
    bubble_column: Block,
    level: &mut dyn LevelAccessor,
    occupy_at: BlockPos,
    below_state: BlockState,
) {
    let occupy_state = level.get_block_state(occupy_at);
    update_column_with_state(bubble_column, level, occupy_at, occupy_state, below_state);
}

/// `updateColumn(bubbleColumn, level, occupyAt, occupyState, belowState)`.
pub fn update_column_with_state(
    bubble_column: Block,
    level: &mut dyn LevelAccessor,
    occupy_at: BlockPos,
    occupy_state: BlockState,
    below_state: BlockState,
) {
    if can_occupy(bubble_column, occupy_state) {
        let column_state = get_column_state(bubble_column, below_state, occupy_state);
        level.set_block_flags(occupy_at, column_state, 2);
        let mut pos = occupy_at.relative(Direction::Up);
        while can_occupy(bubble_column, level.get_block_state(pos)) {
            if !level.set_block_flags(pos, column_state, 2) {
                return;
            }
            pos = pos.relative(Direction::Up);
        }
    }
}

impl BlockBehaviour for BubbleColumnBlock {
    fn tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        let below_state = level.get_block_state(pos.below());
        update_column_with_state(state.block(), level, pos, state, below_state);
    }
    fn is_bucket_pickup(&self) -> bool {
        true
    }
    fn pickup_block(
        &self,
        _user: Option<&dyn Entity>,
        level: &mut dyn LevelAccessor,
        pos: BlockPos,
        _state: BlockState,
    ) -> ItemStack {
        level.set_block_flags(pos, air(), 11);
        ItemStack::new(Item::WATER_BUCKET)
    }
    fn get_pickup_sound(&self) -> Option<SoundEvent> {
        Fluid::WATER.data().pickup_sound
    }
    fn entity_inside(
        &self,
        state: BlockState,
        level: &mut mc_game_core::server::level::ServerLevel,
        pos: BlockPos,
        entity: &mut dyn mc_game_core::world::entity::Entity,
        _effect_applier: &mut dyn mc_game_core::world::entity::inside_block_effects::InsideBlockEffectApplier,
        is_precise: bool,
    ) {
        if is_precise {
            let state_above = level.get_block_state(pos.above());
            let nothing_above = state_above.get_collision_shape(&*level, pos).is_empty()
                && state_above.get_fluid_state().is_empty();
            let drag_down = state.get(props::DRAG);
            if nothing_above {
                entity.on_above_bubble_column(level, drag_down, pos);
            } else {
                entity.on_inside_bubble_column(level, drag_down);
            }
        }
    }
    fn can_survive(&self, state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        let below_state = level.get_block_state(pos.below());
        below_state.is_block(state.block())
            || below_state.is_tag("enables_bubble_column_push_up")
            || below_state.is_tag("enables_bubble_column_drag_down")
    }
    update_shape!(
        |self, state, level, pos, direction, neighbour_pos, neighbour_state| {
            helpers::schedule_water_tick(level, pos);
            if !state.can_survive(&*level, pos)
                || direction == Direction::Down
                || (direction == Direction::Up
                    && !neighbour_state.is_block(state.block())
                    && can_occupy(state.block(), neighbour_state))
            {
                helpers::schedule_block_tick(level, pos, state.block(), 5);
            }
            state
        }
    );
}

register_block_class!("BubbleColumnBlock", |_| Box::new(BubbleColumnBlock));
