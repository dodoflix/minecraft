//! `net.minecraft.world.level.block.LiquidBlock` (water and lava).
use mc_game_core::{register_block_class, update_shape};
use super::bubble_column_block;
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::Entity;
use mc_game_core::world::item::ItemStack;
use mc_game_core::world::level::collision::CollisionContext;
use mc_game_core::world::level::material::{fluid_behaviour, fluid_is_tag};
use mc_game_core::world::level::redstone::Orientation;
use mc_core::phys::shapes::Shapes;
use mc_core::phys::shapes::VoxelShape;
use mc_data::fluid::Fluid;

/// `LiquidBlock` with its `fluid` (the flowing fluid of the block: water or lava).
pub struct LiquidBlock {
    fluid: Fluid,
}

/// `LiquidBlock.POSSIBLE_FLOW_DIRECTIONS`.
const POSSIBLE_FLOW_DIRECTIONS: [Direction; 5] = [Direction::Down, Direction::South, Direction::North, Direction::East, Direction::West];

/// `LiquidBlock.shouldBubbleColumnOccupy`.
fn should_bubble_column_occupy(state: BlockState) -> bool {
    let fluid = state.get_fluid_state();
    fluid.is_tag("bubble_column_can_occupy") && fluid.is_source() && fluid.is_full()
}

/// `fizz(level, pos)`.
fn fizz(level: &mut dyn LevelAccessor, pos: BlockPos) {
    level.level_event(None, 1501, pos, 0);
}

impl LiquidBlock {
    /// `tryScheduleBubbleBlockColumn(ticks, pos, stateBelow)`.
    fn try_schedule_bubble_block_column(&self, level: &mut dyn LevelAccessor, pos: BlockPos, block: Block, state_below: BlockState) {
        if state_below.is_tag("enables_bubble_column_drag_down") || state_below.is_tag("enables_bubble_column_push_up") {
            helpers::schedule_block_tick(level, pos, block, 20);
        }
    }

    /// `shouldSpreadLiquid(level, pos, state)`: lava next to water / blue ice over soul soil turns to stone-like blocks.
    fn should_spread_liquid(&self, level: &mut ServerLevel, pos: BlockPos) -> bool {
        if fluid_is_tag(self.fluid, "lava") {
            let is_over_soul_soil = BlockGetter::get_block_state(level, pos.below()).is_block(Block::SOUL_SOIL);
            for direction in POSSIBLE_FLOW_DIRECTIONS {
                let neighbour_pos = pos.relative(direction.get_opposite());
                if BlockGetter::get_fluid_state(level, neighbour_pos).is_tag("water") {
                    let convert_to = if BlockGetter::get_fluid_state(level, pos).is_source() { Block::OBSIDIAN } else { Block::COBBLESTONE };
                    LevelAccessor::set_block_and_update(level, pos, convert_to.default_state());
                    fizz(level, pos);
                    return false;
                }
                if is_over_soul_soil && BlockGetter::get_block_state(level, neighbour_pos).is_block(Block::BLUE_ICE) {
                    LevelAccessor::set_block_and_update(level, pos, Block::BASALT.default_state());
                    fizz(level, pos);
                    return false;
                }
            }
        }
        true
    }

    /// The body shared by `onPlace` and `neighborChanged`.
    fn schedule_after_change(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        if self.should_spread_liquid(level, pos) {
            let delay = fluid_behaviour(self.fluid).get_tick_delay(&*level);
            level.schedule_fluid_tick(pos, state.get_fluid_state().get_type(), delay, mc_world::ticks::TickPriority::Normal);
        }
        if should_bubble_column_occupy(state) {
            let state_below = BlockGetter::get_block_state(level, pos.below());
            self.try_schedule_bubble_block_column(level, pos, state.block(), state_below);
        }
    }
}

impl BlockBehaviour for LiquidBlock {
    fn get_collision_shape(&self, state: BlockState, level: &dyn BlockGetter, pos: BlockPos, context: &CollisionContext) -> VoxelShape {
        if context.always_collide_with_fluid() {
            Shapes::block()
        } else if state.get(props::LEVEL) != 0 || !context.is_living_entity() {
            Shapes::empty()
        } else {
            let shape = context.liquid_collision_shape();
            if context.is_above(&shape, pos, true) && context.can_stand_on_fluid(level.get_fluid_state(pos.above()), state.get_fluid_state()) {
                shape
            } else {
                Shapes::empty()
            }
        }
    }

    fn is_pathfindable(&self, _state: BlockState, _ty: mc_data::block::PathComputationType) -> bool {
        !fluid_is_tag(self.fluid, "lava")
    }

    fn random_tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        state.get_fluid_state().random_tick(level, pos);
    }

    fn on_place(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, _old_state: BlockState, _moved_by_piston: bool) {
        self.schedule_after_change(state, level, pos);
    }

    fn tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        if should_bubble_column_occupy(state) {
            let state_below = BlockGetter::get_block_state(level, pos.below());
            bubble_column_block::update_column(Block::BUBBLE_COLUMN, level, pos, state_below);
        }
    }

    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if state.get_fluid_state().is_source() || neighbour_state.get_fluid_state().is_source() {
            let delay = fluid_behaviour(self.fluid).get_tick_delay(&*level);
            level.schedule_fluid_tick(pos, state.get_fluid_state().get_type(), delay, mc_world::ticks::TickPriority::Normal);
        }
        if direction == Direction::Down && should_bubble_column_occupy(state) {
            self.try_schedule_bubble_block_column(level, pos, state.block(), neighbour_state);
        }
        state
    });

    fn neighbor_changed(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, _block: Block, _orientation: Option<Orientation>, _moved_by_piston: bool) {
        self.schedule_after_change(state, level, pos);
    }

    fn is_bucket_pickup(&self) -> bool {
        true
    }

    fn pickup_block(&self, _user: Option<&dyn Entity>, level: &mut dyn LevelAccessor, pos: BlockPos, state: BlockState) -> ItemStack {
        if state.get(props::LEVEL) == 0 {
            level.set_block_flags(pos, helpers::air(), 11);
            match self.fluid.data().bucket {
                Some(item) => ItemStack::new(item),
                None => ItemStack::empty(),
            }
        } else {
            ItemStack::empty()
        }
    }

    fn get_pickup_sound(&self) -> Option<mc_data::SoundEvent> {
        self.fluid.data().pickup_sound
    }
}

register_block_class!("LiquidBlock", |block| Box::new(LiquidBlock { fluid: if block == Block::LAVA { Fluid::LAVA } else { Fluid::WATER } }));
