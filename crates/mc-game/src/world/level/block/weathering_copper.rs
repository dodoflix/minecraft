//! `WeatheringCopper` (the weathering chain of the copper block families) and `HoneycombItem.WAX_OFF_BY_BLOCK`.
//!
//! The mappings are derived from the registry names: a family `X` has the blocks `X`, `exposed_X`, `weathered_X`,
//! `oxidized_X` (`copper_block` = `copper_block`, `exposed_copper`, ...), each with a `waxed_` twin.

use super::BlockStateExt;
use mc_data::block::{Block, BlockState};
use std::collections::HashMap;
use std::sync::OnceLock;

/// The families listed by `WeatheringCopper.NEXT_BY_BLOCK`.
const FAMILIES: [&str; 15] = [
    "copper_block",
    "cut_copper",
    "chiseled_copper",
    "cut_copper_slab",
    "cut_copper_stairs",
    "copper_door",
    "copper_trapdoor",
    "copper_bars",
    "copper_grate",
    "copper_bulb",
    "copper_lantern",
    "copper_chest",
    "copper_golem_statue",
    "lightning_rod",
    "copper_chain",
];

struct Maps {
    previous: HashMap<Block, Block>,
    wax_off: HashMap<Block, Block>,
}

fn maps() -> &'static Maps {
    static MAPS: OnceLock<Maps> = OnceLock::new();
    MAPS.get_or_init(|| {
        let mut previous = HashMap::new();
        let mut wax_off = HashMap::new();
        for family in FAMILIES {
            let names: [String; 4] = if family == "copper_block" {
                ["copper_block", "exposed_copper", "weathered_copper", "oxidized_copper"].map(String::from)
            } else {
                [family.to_string(), format!("exposed_{family}"), format!("weathered_{family}"), format!("oxidized_{family}")]
            };
            let blocks: Vec<Option<Block>> = names.iter().map(|n| Block::by_name(&format!("minecraft:{n}"))).collect();
            for (i, name) in names.iter().enumerate() {
                if let (Some(block), Some(waxed)) = (blocks[i], Block::by_name(&format!("minecraft:waxed_{name}"))) {
                    wax_off.insert(waxed, block);
                }
                if i > 0 && let (Some(prev), Some(block)) = (blocks[i - 1], blocks[i]) {
                    previous.insert(block, prev);
                }
            }
        }
        Maps { previous, wax_off }
    })
}

/// `state.getBlock() instanceof WeatheringCopper`.
pub fn is_weathering_copper(block: Block) -> bool {
    let m = maps();
    m.previous.contains_key(&block) || m.previous.values().any(|b| *b == block)
}

/// `HoneycombItem.WAX_OFF_BY_BLOCK.get().get(block) != null`.
pub fn is_waxed(block: Block) -> bool {
    maps().wax_off.contains_key(&block)
}

/// `WeatheringCopper.getPrevious(state)`.
pub fn get_previous(state: BlockState) -> Option<BlockState> {
    maps().previous.get(&state.block()).map(|b| b.default_state().with_properties_of(state))
}

/// `WeatheringCopper.getFirst(state)`.
pub fn get_first(state: BlockState) -> BlockState {
    let m = maps();
    let mut candidate = state.block();
    while let Some(prev) = m.previous.get(&candidate) {
        candidate = *prev;
    }
    candidate.default_state().with_properties_of(state)
}
