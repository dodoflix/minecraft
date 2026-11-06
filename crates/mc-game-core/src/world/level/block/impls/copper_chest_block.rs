//! `net.minecraft.world.level.block.CopperChestBlock` (extends `ChestBlock`) and `WeatheringCopperChestBlock`: the
//! class-specific parts; the behavior object is `chest_block::ChestBlock { kind: Copper }`.
use super::chest_block::{ChestBlock, ChestKind, get_connected_direction};
use super::prelude::*;
use crate::world::level::block::weathering_copper;
use mc_data::enums::ChestType;

/// `chestCanConnectTo`: `blockState.is(BlockTags.COPPER_CHESTS) && blockState.hasProperty(ChestBlock.TYPE)`.
pub fn chest_can_connect_to(state: BlockState) -> bool {
    state.is_tag("copper_chests") && state.has_property(props::CHEST_TYPE)
}

/// `WeatheringCopper.WeatherState` ordinal of a copper chest block name (`waxed_` blocks share their twin's state).
pub fn weather_state(name: &str) -> i32 {
    let name = name.strip_prefix("waxed_").unwrap_or(name);
    if name.starts_with("exposed_") {
        1
    } else if name.starts_with("weathered_") {
        2
    } else if name.starts_with("oxidized_") {
        3
    } else {
        0
    }
}

fn state_of(block: Block) -> i32 {
    weather_state(
        block
            .name()
            .strip_prefix("minecraft:")
            .unwrap_or(block.name()),
    )
}

/// `isWaxed()` (the non-weathering `CopperChestBlock`).
fn is_waxed(block: Block) -> bool {
    weathering_copper::is_waxed(block)
}

/// `unwaxBlock(copperChestBlock, state)`.
fn unwax_block(block: Block, state: BlockState) -> BlockState {
    if !is_waxed(block) {
        return state;
    }
    let name = block
        .name()
        .strip_prefix("minecraft:")
        .unwrap_or(block.name());
    match name
        .strip_prefix("waxed_")
        .and_then(|n| Block::by_name(&format!("minecraft:{n}")))
    {
        Some(b) => b.default_state().with_properties_of(state),
        None => state,
    }
}

/// `getLeastOxidizedChestOfConnectedBlocks(state, level, pos)`.
pub fn get_least_oxidized_chest_of_connected_blocks(
    state: BlockState,
    level: &dyn BlockGetter,
    pos: BlockPos,
) -> BlockState {
    let connected_state = level.get_block_state(pos.relative(get_connected_direction(state)));
    if state.get(props::CHEST_TYPE) != ChestType::Single
        && state.block().data().classes.contains(&"CopperChestBlock")
        && connected_state
            .block()
            .data()
            .classes
            .contains(&"CopperChestBlock")
    {
        let (this, connected) = (state.block(), connected_state.block());
        let mut updated = state;
        let mut connected_predicted = connected_state;
        if is_waxed(this) != is_waxed(connected) {
            updated = unwax_block(this, state);
            connected_predicted = unwax_block(connected, connected_state);
        }
        let least_oxidized = if state_of(this) <= state_of(connected) {
            updated.block()
        } else {
            connected_predicted.block()
        };
        return least_oxidized.default_state().with_properties_of(updated);
    }
    state
}

/// The part of `CopperChestBlock.updateShape` after `super.updateShape`.
pub fn update_shape_tail(
    block_state: BlockState,
    direction_to_neighbour: Direction,
    neighbour_state: BlockState,
) -> BlockState {
    if chest_can_connect_to(neighbour_state) {
        let chest_type = block_state.get(props::CHEST_TYPE);
        if chest_type != ChestType::Single
            && get_connected_direction(block_state) == direction_to_neighbour
        {
            return neighbour_state
                .block()
                .default_state()
                .with_properties_of(block_state);
        }
    }
    block_state
}

/// `CopperChestBlock.getFromCopperBlock(copperBlock, facing, level, pos)` (used by the copper golem's chest statue
/// conversions): the chest of the copper block's weathering stage placed against its neighbours.
pub fn get_from_copper_block(
    copper_block: Block,
    facing: Direction,
    level: &dyn BlockGetter,
    pos: BlockPos,
) -> BlockState {
    let name = copper_block
        .name()
        .strip_prefix("minecraft:")
        .unwrap_or(copper_block.name());
    let (waxed, base) = name
        .strip_prefix("waxed_")
        .map_or((false, name), |n| (true, n));
    let stage = match base {
        "copper_block" => Some(""),
        "exposed_copper" => Some("exposed_"),
        "weathered_copper" => Some("weathered_"),
        "oxidized_copper" => Some("oxidized_"),
        _ => None,
    };
    // `getOrDefault(copperBlock, COPPER_CHEST.weathering().unaffected())`
    let chest_name = stage.map_or_else(
        || "copper_chest".to_string(),
        |s| format!("{}{s}copper_chest", if waxed { "waxed_" } else { "" }),
    );
    let block = Block::by_name(&format!("minecraft:{chest_name}"))
        .unwrap_or_else(|| Block::by_name("minecraft:copper_chest").expect("copper chest"));
    let chest_type =
        super::chest_block::get_chest_type(ChestKind::Copper, block, level, pos, facing);
    let state = block
        .default_state()
        .set(props::HORIZONTAL_FACING, facing)
        .set(props::CHEST_TYPE, chest_type);
    get_least_oxidized_chest_of_connected_blocks(state, level, pos)
}

register_block_class!("CopperChestBlock", |_| Box::new(ChestBlock {
    kind: ChestKind::Copper
}));
