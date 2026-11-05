//! `WeatheringCopper` + `ChangeOverTimeBlock` (the shared oxidation machinery of every `WeatheringCopper*Block` class)
//! and `HoneycombItem.WAX_OFF_BY_BLOCK` (`WAXABLES.inverse()`).
//!
//! The chain tables come from `mc_data::misc` (`WeatheringCopper.NEXT_BY_BLOCK`, `HoneycombItem.WAXABLES`). A block is a
//! `ChangeOverTimeBlock` exactly when it is part of the chain; its `getAge()` is its depth in the chain (waxed twins are
//! plain blocks that merely keep their weather state in their own properties, they never age). The Java classes
//! `WeatheringCopper{Full,Slab,Stair,Door,TrapDoor,Chain,Bars,Bulb,Chest,GolemStatue}Block`, `WeatheringLanternBlock`,
//! `WeatheringCopperGrateBlock` and `WeatheringLightningRodBlock` only add `randomTick -> changeOverTime` (door: lower
//! half only, chest: not the right half and nobody viewing it) to their parent, so [`random_tick`] is spliced into
//! `BlockStateExt::random_tick` instead of masking the parent behaviours.

use super::BlockStateExt;
use crate::server::level::ServerLevel;
use crate::world::level::{BlockGetter, LevelAccessor};
use mc_core::pos::BlockPos;
use mc_data::block::{Block, BlockState};
use mc_data::enums::{ChestType, DoubleBlockHalf};
use mc_data::misc::{WAXABLES, WEATHERING_NEXT};
use mc_data::props;
use std::collections::HashMap;
use std::sync::LazyLock;

/// `WeatheringCopper.WeatherState`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum WeatherState {
    Unaffected,
    Exposed,
    Weathered,
    Oxidized,
}

impl WeatherState {
    const ALL: [WeatherState; 4] = [Self::Unaffected, Self::Exposed, Self::Weathered, Self::Oxidized];

    pub fn ordinal(self) -> usize {
        self as usize
    }
    /// `WeatherState.next` (`ByIdMap.OutOfBoundsStrategy.CLAMP`).
    pub fn next(self) -> WeatherState {
        Self::ALL[(self.ordinal() + 1).min(3)]
    }
    /// `WeatherState.previous` (clamped).
    pub fn previous(self) -> WeatherState {
        Self::ALL[self.ordinal().saturating_sub(1)]
    }
}

struct Maps {
    previous: HashMap<Block, Block>,
    age: HashMap<Block, WeatherState>,
    wax_off: HashMap<Block, Block>,
}

static MAPS: LazyLock<Maps> = LazyLock::new(|| {
    let previous: HashMap<Block, Block> = WEATHERING_NEXT.iter().map(|(from, to)| (*to, *from)).collect();
    let mut age = HashMap::new();
    for (from, to) in WEATHERING_NEXT.iter() {
        for block in [*from, *to] {
            let mut depth = 0;
            let mut candidate = block;
            while let Some(prev) = previous.get(&candidate) {
                candidate = *prev;
                depth += 1;
            }
            age.insert(block, WeatherState::ALL[depth]);
        }
    }
    let wax_off = WAXABLES.iter().map(|(unwaxed, waxed)| (*waxed, *unwaxed)).collect();
    Maps { previous, age, wax_off }
});

fn maps() -> &'static Maps {
    &MAPS
}

/// `state.getBlock() instanceof WeatheringCopper` (= `ChangeOverTimeBlock`) and its `getAge()`.
pub fn age(block: Block) -> Option<WeatherState> {
    maps().age.get(&block).copied()
}

/// `state.getBlock() instanceof WeatheringCopper`.
pub fn is_weathering_copper(block: Block) -> bool {
    maps().age.contains_key(&block)
}

/// `HoneycombItem.WAX_OFF_BY_BLOCK.get().get(block) != null`.
pub fn is_waxed(block: Block) -> bool {
    maps().wax_off.contains_key(&block)
}

/// `HoneycombItem.WAX_OFF_BY_BLOCK.get().get(block)` (`unwaxables`).
pub fn wax_off(block: Block) -> Option<Block> {
    maps().wax_off.get(&block).copied()
}

/// `HoneycombItem.WAXABLES.get().get(block)`.
pub fn wax_on(block: Block) -> Option<Block> {
    mc_data::misc::waxed(block)
}

/// `WeatheringCopper.getNext(block)`.
pub fn get_next(block: Block) -> Option<Block> {
    mc_data::misc::weathering_next(block)
}

/// `WeatheringCopper.getNext(state)` / `getNext(BlockState)` of the instance.
pub fn get_next_state(state: BlockState) -> Option<BlockState> {
    get_next(state.block()).map(|b| b.default_state().with_properties_of(state))
}

/// `WeatheringCopper.getPrevious(block)`.
pub fn get_previous_block(block: Block) -> Option<Block> {
    maps().previous.get(&block).copied()
}

/// `WeatheringCopper.getPrevious(state)`.
pub fn get_previous(state: BlockState) -> Option<BlockState> {
    get_previous_block(state.block()).map(|b| b.default_state().with_properties_of(state))
}

/// `WeatheringCopper.getFirst(block)`.
pub fn get_first_block(block: Block) -> Block {
    let mut candidate = block;
    while let Some(prev) = maps().previous.get(&candidate) {
        candidate = *prev;
    }
    candidate
}

/// `WeatheringCopper.getFirst(state)`.
pub fn get_first(state: BlockState) -> BlockState {
    get_first_block(state.block()).default_state().with_properties_of(state)
}

/// `WeatheringCopper.getChanceModifier`.
fn chance_modifier(age: WeatherState) -> f32 {
    if age == WeatherState::Unaffected { 0.75 } else { 1.0 }
}

/// `ChangeOverTimeBlock.getNextState`.
pub fn get_next_state_scanning(state: BlockState, level: &mut ServerLevel, pos: BlockPos) -> Option<BlockState> {
    let own_age = age(state.block())?.ordinal();
    let mut same_age_count = 0i32;
    let mut older_count = 0i32;
    for block_pos in BlockPos::within_manhattan(&pos, 4) {
        if block_pos == pos {
            continue;
        }
        let Some(found) = age(level.get_block_state(block_pos).block()) else {
            continue;
        };
        let found_age = found.ordinal();
        if found_age < own_age {
            return None;
        }
        if found_age > own_age {
            older_count += 1;
        } else {
            same_age_count += 1;
        }
    }
    let chance = (older_count + 1) as f32 / (older_count + same_age_count + 1) as f32;
    let actual_chance = chance * chance * chance_modifier(WeatherState::ALL[own_age]);
    if level.random().next_float() < actual_chance {
        get_next_state(state)
    } else {
        None
    }
}

/// `ChangeOverTimeBlock.changeOverTime`.
pub fn change_over_time(state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
    if level.random().next_float() < 0.05688889_f32
        && let Some(weathered_state) = get_next_state_scanning(state, level, pos)
    {
        level.set_block_and_update(pos, weathered_state);
    }
}

/// The `randomTick` of the `WeatheringCopper*Block` classes; `false` when `state` is not a `ChangeOverTimeBlock`.
pub fn random_tick(state: BlockState, level: &mut ServerLevel, pos: BlockPos) -> bool {
    if age(state.block()).is_none() {
        return false;
    }
    let tickable = if state.has_property(props::DOUBLE_BLOCK_HALF) {
        // WeatheringCopperDoorBlock
        state.get(props::DOUBLE_BLOCK_HALF) == DoubleBlockHalf::Lower
    } else if state.has_property(props::CHEST_TYPE) {
        // WeatheringCopperChestBlock
        state.get(props::CHEST_TYPE) != ChestType::Right && !chest_in_use(level, pos)
    } else {
        true
    };
    if tickable {
        change_over_time(state, level, pos);
    }
    true
}

/// `!(level.getBlockEntity(pos) instanceof ChestBlockEntity c && c.getEntitiesWithContainerOpen().isEmpty())`.
fn chest_in_use(_level: &ServerLevel, _pos: BlockPos) -> bool {
    false
}
