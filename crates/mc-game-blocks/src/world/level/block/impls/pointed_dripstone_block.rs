//! `net.minecraft.world.level.block.PointedDripstoneBlock` (extends `SpeleothemBlock`): fluid transfer into cauldrons /
//! mud -> clay, growth conditions. The speleothem logic itself is [`super::speleothem_block`].
use super::speleothem_block::{
    Speleothem, SpeleothemClass, find_block_vertical, find_tip, is_stalactite,
    is_stalactite_start_pos,
};
use mc_core::phys::shapes::{BooleanOp, Shapes, VoxelShape};
use mc_data::GameEvent;
use mc_data::enums::SpeleothemThickness;
use mc_data::fluid::Fluid;
use mc_game_core::register_block_class;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::Entity;
use mc_game_core::world::level::block::behaviour;
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::world::level::gameevent::GameEventContext;
use std::sync::LazyLock;

const MAX_SEARCH_LENGTH_WHEN_CHECKING_DRIP_TYPE: i32 = 11;
const MAX_SEARCH_LENGTH_BETWEEN_STALACTITE_TIP_AND_CAULDRON: i32 = 11;
const WATER_TRANSFER_PROBABILITY_PER_RANDOM_TICK: f32 = 0.17578125;
const LAVA_TRANSFER_PROBABILITY_PER_RANDOM_TICK: f32 = 0.05859375;

static REQUIRED_SPACE_TO_DRIP_THROUGH_NON_SOLID_BLOCK: LazyLock<VoxelShape> =
    LazyLock::new(|| mc_game_core::world::level::shapes::column(4.0, 0.0, 16.0));

pub struct PointedDripstoneBlock;

/// `PointedDripstoneBlock.FluidInfo`.
struct FluidInfo {
    pos: BlockPos,
    fluid: Fluid,
    source_state: BlockState,
}

impl SpeleothemClass for PointedDripstoneBlock {
    fn block_to_grow_on(&self) -> Block {
        Block::DRIPSTONE_BLOCK
    }
    fn stalactite_landing_sound(&self) -> i32 {
        1045
    }
    fn can_grow(&self, level: &dyn LevelReader, pos: BlockPos) -> bool {
        let fluid_state = level.get_block_state(pos.above_n(2)).get_fluid_state();
        level
            .get_block_state(pos.above())
            .is_block(self.block_to_grow_on())
            && fluid_state.get_type() == Fluid::WATER
            && fluid_state.is_source()
    }
    fn blocks_stalagmite_scan(
        &self,
        level: &dyn LevelReader,
        pos: BlockPos,
        state: BlockState,
    ) -> bool {
        !can_drip_through(level, pos, state)
    }
    fn fall_on(
        &self,
        level: &mut ServerLevel,
        state: BlockState,
        entity: &mut dyn Entity,
        fall_distance: f64,
    ) -> bool {
        if state.get(props::VERTICAL_DIRECTION) == Direction::Up
            && state.get(props::SPELEOTHEM_THICKNESS) == SpeleothemThickness::Tip
        {
            let source = level.damage_sources().stalagmite();
            entity.cause_fall_damage(level, fall_distance + 2.5, 2.0, &source);
            true
        } else {
            false
        }
    }
    fn before_random_tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        let random_value = level.random().next_float();
        maybe_transfer_fluid(state, level, pos, random_value);
    }
}

/// `maybeTransferFluid` (`randomValue` = the `random.nextFloat()` drawn by `randomTick`).
pub fn maybe_transfer_fluid(
    state: BlockState,
    level: &mut ServerLevel,
    pos: BlockPos,
    random_value: f32,
) {
    if random_value > WATER_TRANSFER_PROBABILITY_PER_RANDOM_TICK
        && random_value > LAVA_TRANSFER_PROBABILITY_PER_RANDOM_TICK
    {
        return;
    }
    if !is_stalactite_start_pos(state, level, pos) {
        return;
    }
    let Some(fluid_info) = get_fluid_above_stalactite(level, pos, state) else {
        return;
    };
    let fluid = fluid_info.fluid;
    let transfer_probability = if fluid == Fluid::WATER {
        WATER_TRANSFER_PROBABILITY_PER_RANDOM_TICK
    } else if fluid == Fluid::LAVA {
        LAVA_TRANSFER_PROBABILITY_PER_RANDOM_TICK
    } else {
        return;
    };
    if random_value >= transfer_probability {
        return;
    }
    let Some(stalactite_tip_pos) = find_tip(
        state,
        level,
        pos,
        MAX_SEARCH_LENGTH_WHEN_CHECKING_DRIP_TYPE,
        false,
    ) else {
        return;
    };
    if fluid_info.source_state.is_block(Block::MUD) && fluid == Fluid::WATER {
        let new_state = Block::CLAY.default_state();
        level.set_block_and_update(fluid_info.pos, new_state);
        helpers::push_entities_up(level, fluid_info.source_state, new_state, fluid_info.pos);
        level.game_event_at_block(
            GameEvent::BLOCK_CHANGE,
            fluid_info.pos,
            &GameEventContext::of_state(Some(new_state)),
        );
        level.level_event(None, 1504, stalactite_tip_pos, 0);
    } else if let Some(cauldron_pos) =
        find_fillable_cauldron_below_stalactite_tip(level, stalactite_tip_pos, fluid)
    {
        level.level_event(None, 1504, stalactite_tip_pos, 0);
        let fall_distance = stalactite_tip_pos.y() - cauldron_pos.y();
        let delay = 50 + fall_distance;
        let cauldron_state = level.get_block_state(cauldron_pos);
        level.schedule_tick_block(cauldron_pos, cauldron_state.block(), delay);
    }
}

/// `findRootBlock`.
fn find_root_block(
    level: &dyn LevelReader,
    pos: BlockPos,
    dripstone_state: BlockState,
    max_search_length: i32,
) -> Option<BlockPos> {
    let tip_direction = dripstone_state.get(props::VERTICAL_DIRECTION);
    find_block_vertical(
        level,
        pos,
        tip_direction.get_opposite(),
        &|_, state| {
            state.is_block(dripstone_state.block())
                && state.get(props::VERTICAL_DIRECTION) == tip_direction
        },
        &|state| !state.is_block(dripstone_state.block()),
        max_search_length,
    )
}

/// `findFillableCauldronBelowStalactiteTip`.
fn find_fillable_cauldron_below_stalactite_tip(
    level: &dyn LevelReader,
    stalactite_tip_pos: BlockPos,
    fluid: Fluid,
) -> Option<BlockPos> {
    find_block_vertical(
        level,
        stalactite_tip_pos,
        Direction::Down,
        &|pos, state| can_drip_through(level, pos, state),
        &|state| {
            helpers::is_instance_of(state, "AbstractCauldronBlock")
                && behaviour(state.block()).can_receive_stalactite_drip(fluid)
        },
        MAX_SEARCH_LENGTH_BETWEEN_STALACTITE_TIP_AND_CAULDRON,
    )
}

/// `findStalactiteTipAboveCauldron`.
pub fn find_stalactite_tip_above_cauldron(
    level: &dyn LevelReader,
    cauldron_pos: BlockPos,
) -> Option<BlockPos> {
    find_block_vertical(
        level,
        cauldron_pos,
        Direction::Up,
        &|pos, state| can_drip_through(level, pos, state),
        &super::speleothem_block::is_free_hanging_stalactite,
        MAX_SEARCH_LENGTH_BETWEEN_STALACTITE_TIP_AND_CAULDRON,
    )
}

/// `getCauldronFillFluidType` (`Fluids.EMPTY` when nothing can fill a cauldron).
pub fn get_cauldron_fill_fluid_type(level: &ServerLevel, stalactite_pos: BlockPos) -> Fluid {
    get_fluid_above_stalactite(level, stalactite_pos, level.get_block_state(stalactite_pos))
        .map(|info| info.fluid)
        .filter(|f| can_fill_cauldron(*f))
        .unwrap_or(Fluid::EMPTY)
}

/// `getFluidAboveStalactite`.
fn get_fluid_above_stalactite(
    level: &ServerLevel,
    stalactite_pos: BlockPos,
    stalactite_state: BlockState,
) -> Option<FluidInfo> {
    if !is_stalactite(stalactite_state) {
        return None;
    }
    find_root_block(
        level,
        stalactite_pos,
        stalactite_state,
        MAX_SEARCH_LENGTH_WHEN_CHECKING_DRIP_TYPE,
    )
    .map(|root_pos| {
        let above_pos = root_pos.above();
        let above_state = level.get_block_state(above_pos);
        let fluid = if above_state.is_block(Block::MUD)
            && !level.attribute_bool(
                mc_game_core::world::attribute::attributes::WATER_EVAPORATES,
                above_pos,
            ) {
            Fluid::WATER
        } else {
            level.get_fluid_state(above_pos).get_type()
        };
        FluidInfo {
            pos: above_pos,
            fluid,
            source_state: above_state,
        }
    })
}

fn can_fill_cauldron(fluid_above: Fluid) -> bool {
    fluid_above == Fluid::LAVA || fluid_above == Fluid::WATER
}

/// `canDripThrough`.
fn can_drip_through(level: &dyn BlockGetter, pos: BlockPos, state: BlockState) -> bool {
    if state.is_air() {
        true
    } else if state.is_solid_render() {
        false
    } else if !state.get_fluid_state().is_empty() {
        false
    } else {
        let collision_shape = state.get_collision_shape(level, pos);
        !Shapes::join_is_not_empty(
            &REQUIRED_SPACE_TO_DRIP_THROUGH_NON_SOLID_BLOCK,
            &collision_shape,
            BooleanOp::AND,
        )
    }
}

register_block_class!("PointedDripstoneBlock", |block| Box::new(Speleothem {
    block,
    class: PointedDripstoneBlock
}));
