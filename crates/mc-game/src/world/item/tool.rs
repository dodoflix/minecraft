//! The `minecraft:tool` component (`Tool.getMiningSpeed`, `isCorrectForDrops`), read from the stack's component data.
//! Block lists are resolved against the vanilla data pack tags (`mc_data::tags`).

use super::ItemStack;
use super::ItemStackExt;
use mc_data::block::{Block, BlockState};
use serde_json::Value;

/// Does `blocks` (a block id, `#tag`, or list of those — `HolderSet` codec) contain `block`?
fn holder_set_contains(blocks: &Value, block: Block) -> bool {
    match blocks {
        Value::String(s) => match s.strip_prefix('#') {
            Some(tag) => mc_data::tags::get::<Block>(tag).is_some_and(|t| t.contains(block)),
            None => Block::by_name(s) == Some(block),
        },
        Value::Array(a) => a.iter().any(|v| holder_set_contains(v, block)),
        _ => false,
    }
}

/// `Tool.getMiningSpeed(state)`: the first matching rule with a `speed`, else `defaultMiningSpeed` (1.0).
pub fn get_mining_speed(stack: &ItemStack, state: BlockState) -> f32 {
    let Some(tool) = stack.get_json("minecraft:tool") else { return 1.0 };
    let block = state.block();
    if let Some(rules) = tool.get("rules").and_then(Value::as_array) {
        for rule in rules {
            if let (Some(blocks), Some(speed)) = (rule.get("blocks"), rule.get("speed").and_then(Value::as_f64)) {
                if holder_set_contains(blocks, block) {
                    return speed as f32;
                }
            }
        }
    }
    tool.get("default_mining_speed").and_then(Value::as_f64).map_or(1.0, |s| s as f32)
}

/// `Tool.isCorrectForDrops(state)`: the first rule matching with `correct_for_drops` set decides.
pub fn is_correct_for_drops(stack: &ItemStack, state: BlockState) -> bool {
    let Some(tool) = stack.get_json("minecraft:tool") else { return false };
    let block = state.block();
    if let Some(rules) = tool.get("rules").and_then(Value::as_array) {
        for rule in rules {
            if let (Some(blocks), Some(correct)) = (rule.get("blocks"), rule.get("correct_for_drops").and_then(Value::as_bool)) {
                if holder_set_contains(blocks, block) {
                    return correct;
                }
            }
        }
    }
    false
}
