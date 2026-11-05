//! `net.minecraft.world.level.block.GrowingPlantBodyBlock`.
use super::growing_plant_block::GrowingPlant;
use mc_game_core::world::level::block::impls::prelude::*;

pub trait GrowingPlantBody: GrowingPlant {
    /// `updateHeadAfterConvertedFromBody(bodyState, headState)`.
    fn update_head_after_converted_from_body(&self, _body_state: BlockState, head_state: BlockState) -> BlockState {
        head_state
    }
}

/// `GrowingPlantBodyBlock.updateShape`.
pub fn update_shape(
    this: &dyn GrowingPlantBody,
    state: BlockState,
    level: &mut dyn LevelAccessor,
    pos: BlockPos,
    direction_to_neighbour: Direction,
    neighbour_state: BlockState,
) -> BlockState {
    let growth = this.growth_direction();
    if direction_to_neighbour == growth.get_opposite() && !state.can_survive(&*level, pos) {
        helpers::schedule_block_tick(level, pos, state.block(), 1);
    }
    let head_block = this.head_block();
    if direction_to_neighbour == growth && !neighbour_state.is_block(state.block()) && !neighbour_state.is_block(head_block) {
        let head_state = super::growing_plant_head_block::get_state_for_placement(head_block, level.random());
        this.update_head_after_converted_from_body(state, head_state)
    } else {
        if this.schedule_fluid_ticks() {
            helpers::schedule_water_tick(level, pos);
        }
        state
    }
}

/// `BlockUtil.getTopConnectedBlock`: the first block past the run of `body_block`s in `growth_direction`, when it is `head_block`.
pub fn get_top_connected_block(level: &dyn BlockGetter, pos: BlockPos, body_block: Block, growth_direction: Direction, head_block: Block) -> Option<BlockPos> {
    let mut forward_pos = pos;
    loop {
        forward_pos = forward_pos.relative(growth_direction);
        if !level.get_block_state(forward_pos).is_block(body_block) {
            break;
        }
    }
    level.get_block_state(forward_pos).is_block(head_block).then_some(forward_pos)
}

fn get_head_pos(this: &dyn GrowingPlantBody, level: &dyn BlockGetter, pos: BlockPos, body_block: Block) -> Option<BlockPos> {
    get_top_connected_block(level, pos, body_block, this.growth_direction(), this.head_block())
}

/// `GrowingPlantBodyBlock.isValidBonemealTarget`.
pub fn is_valid_bonemeal_target(this: &dyn GrowingPlantBody, level: &dyn LevelReader, pos: BlockPos, state: BlockState) -> bool {
    match get_head_pos(this, level, pos, state.block()) {
        None => false,
        Some(head_pos) => {
            let growth_pos = head_pos.relative(this.growth_direction());
            super::growing_plant_head_block::head(this.head_block()).can_grow_into(level.get_block_state(growth_pos)) && !level.is_outside_build_height(growth_pos.y())
        }
    }
}

/// `GrowingPlantBodyBlock.performBonemeal`: the head block grows.
pub fn perform_bonemeal(this: &dyn GrowingPlantBody, level: &mut mc_game_core::server::level::ServerLevel, pos: BlockPos, state: BlockState, source: mc_game_core::world::level::block::bonemeal::BonemealSource) {
    if let Some(head_pos) = get_head_pos(this, &*level, pos, state.block()) {
        let forward_state = level.get_block_state(head_pos);
        mc_game_core::world::level::block::behaviour(forward_state.block()).perform_bonemeal(level, head_pos, forward_state, source);
    }
}

/// `GrowingPlantBodyBlock.canBeReplaced(state, context)`.
pub fn can_be_replaced(this: &dyn GrowingPlantBody, state: BlockState, context: &mc_game_core::world::item::BlockPlaceContext) -> bool {
    let held = context.use_on.get_item_in_hand();
    let result = mc_game_core::world::level::block::PlainBlock.can_be_replaced(state, context);
    result && (held.is_empty() || Some(held.item()) != this.head_block().item())
}

/// `GrowingPlantBodyBlock.getCloneItemStack`.
pub fn get_clone_item_stack(this: &dyn GrowingPlantBody) -> mc_game_core::world::item::ItemStack {
    match this.head_block().item() {
        Some(item) => mc_game_core::world::item::ItemStack::new(item),
        None => mc_game_core::world::item::ItemStack::empty(),
    }
}

/// The `BlockBehaviour` methods every `GrowingPlantBodyBlock` shares, to be invoked inside the `impl BlockBehaviour`:
/// `growing_plant_body_behaviour!()`, or `growing_plant_body_behaviour!(custom_bonemeal_and_clone)` (cave vines).
macro_rules! growing_plant_body_behaviour {
    () => {
        $crate::world::level::block::impls::growing_plant_body_block::growing_plant_body_behaviour!(@common);
        fn get_clone_item_stack(&self, _state: BlockState, _level: &::mc_game_core::server::level::ServerLevel, _pos: BlockPos, _include_data: bool) -> ::mc_game_core::world::item::ItemStack {
            $crate::world::level::block::impls::growing_plant_body_block::get_clone_item_stack(self)
        }
        fn is_valid_bonemeal_target(&self, level: &dyn LevelReader, pos: BlockPos, state: BlockState, _source: ::mc_game_core::world::level::block::bonemeal::BonemealSource) -> bool {
            $crate::world::level::block::impls::growing_plant_body_block::is_valid_bonemeal_target(self, level, pos, state)
        }
        fn perform_bonemeal(&self, level: &mut ::mc_game_core::server::level::ServerLevel, pos: BlockPos, state: BlockState, source: ::mc_game_core::world::level::block::bonemeal::BonemealSource) {
            $crate::world::level::block::impls::growing_plant_body_block::perform_bonemeal(self, level, pos, state, source)
        }
    };
    (custom_bonemeal_and_clone) => {
        $crate::world::level::block::impls::growing_plant_body_block::growing_plant_body_behaviour!(@common);
    };
    (@common) => {
        fn can_survive(&self, _state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
            $crate::world::level::block::impls::growing_plant_block::can_survive(self, level, pos)
        }
        update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
            $crate::world::level::block::impls::growing_plant_body_block::update_shape(self, state, level, pos, direction, neighbour_state)
        });
        fn tick(&self, state: BlockState, level: &mut ::mc_game_core::server::level::ServerLevel, pos: BlockPos) {
            $crate::world::level::block::impls::growing_plant_block::tick(state, level, pos);
        }
        fn is_bonemealable(&self) -> bool {
            true
        }
        fn can_be_replaced(&self, state: BlockState, context: &::mc_game_core::world::item::BlockPlaceContext) -> bool {
            $crate::world::level::block::impls::growing_plant_body_block::can_be_replaced(self, state, context)
        }
    };
}
pub(crate) use growing_plant_body_behaviour;
