//! `BlockInput`: a parsed block state (+ the properties that were written explicitly, + block entity NBT).

use super::block_state_parser::PropertyValue;
use mc_data::block::BlockState;
use mc_nbt::CompoundTag;

#[derive(Clone, Debug)]
pub struct BlockInput {
    state: BlockState,
    properties: Vec<PropertyValue>,
    tag: Option<CompoundTag>,
}

impl BlockInput {
    pub fn new(state: BlockState, properties: Vec<PropertyValue>, tag: Option<CompoundTag>) -> Self {
        Self { state, properties, tag }
    }

    pub fn get_state(&self) -> BlockState {
        self.state
    }

    pub fn get_defined_properties(&self) -> &[PropertyValue] {
        &self.properties
    }

    pub fn tag(&self) -> Option<&CompoundTag> {
        self.tag.as_ref()
    }

    /// The state part of `test(BlockInWorld)`: same block and every defined property equal.
    pub fn matches_state(&self, state: BlockState) -> bool {
        state.block() == self.state.block()
            && self.properties.iter().all(|(property, _)| state.get_property_str(property.name) == self.state.get_property_str(property.name))
    }

    /// `overwriteWithDefinedProperties(state)`: copies the explicitly written properties onto `state`.
    pub fn overwrite_with_defined_properties(&self, state: BlockState) -> BlockState {
        if state == self.state {
            return state;
        }
        let mut state = state;
        for (property, index) in &self.properties {
            if let Some(s) = state.with_property_str(property.name, property.values[*index]) {
                state = s;
            }
        }
        state
    }
}
