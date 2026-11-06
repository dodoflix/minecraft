//! `exploration_map` (`level.findNearestMapStructure` + `MapItem.applyNewSavedData` / `renderBiomePreviewMap` /
//! `addTargetDecoration`). Not implemented: the stack is returned untouched (vanilla also returns it unchanged when no
//! structure is found).

use mc_core::phys::Vec3;
use mc_protocol::components::ItemStack;

use super::functions::items::ExplorationMapFunction;
use crate::server::level::ServerLevel;

pub fn explore(
    _level: &ServerLevel,
    stack: ItemStack,
    _origin: Vec3,
    _function: &ExplorationMapFunction,
) -> ItemStack {
    stack
}
