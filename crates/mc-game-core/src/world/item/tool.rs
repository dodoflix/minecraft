//! `Tool.getMiningSpeed` / `isCorrectForDrops` on the typed `minecraft:tool` component.

use super::ItemStack;
use mc_data::block::BlockState;
use mc_protocol::components::TOOL;

/// `Tool.getMiningSpeed(state)`: the first matching rule with a `speed`, else `defaultMiningSpeed`.
pub fn get_mining_speed(stack: &ItemStack, state: BlockState) -> f32 {
    let Some(tool) = stack.get(&TOOL) else { return 1.0 };
    let block = state.block();
    for rule in &tool.rules {
        if let Some(speed) = rule.speed {
            if rule.blocks.contains(block) {
                return speed;
            }
        }
    }
    tool.default_mining_speed
}

/// `Tool.isCorrectForDrops(state)`: the first rule matching with `correct_for_drops` set decides.
pub fn is_correct_for_drops(stack: &ItemStack, state: BlockState) -> bool {
    let Some(tool) = stack.get(&TOOL) else { return false };
    let block = state.block();
    for rule in &tool.rules {
        if let Some(correct) = rule.correct_for_drops {
            if rule.blocks.contains(block) {
                return correct;
            }
        }
    }
    false
}
