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

    /// `place(level, pos, flags)`: `setBlock` with the defined properties applied, then the block entity NBT.
    ///
    /// Gap: `loadWithComponents` also reads `components` of the tag; only `load_additional` is applied.
    pub fn place(&self, level: &mut crate::server::level::ServerLevel, pos: mc_core::pos::BlockPos, flags: u32) -> bool {
        use crate::world::level::LevelAccessor;
        let mut state = if flags & 16 != 0 {
            self.state
        } else {
            crate::world::level::block::helpers::update_from_neighbour_shapes(self.state, level, pos)
        };
        if state.is_air() {
            state = self.state;
        }
        state = self.overwrite_with_defined_properties(state);
        let mut affected = level.set_block(pos, state, flags, 512);
        if let Some(tag) = &self.tag {
            let changed = level.with_block_entity(pos, |_, entity| {
                let mut before = CompoundTag::new();
                entity.save_additional(&mut before);
                entity.load_additional(tag);
                let mut after = CompoundTag::new();
                entity.save_additional(&mut after);
                after != before
            });
            if changed == Some(true) {
                affected = true;
                level.block_changed(pos);
            }
        }
        affected
    }
}
