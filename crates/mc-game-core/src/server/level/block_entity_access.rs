//! Typed access to a level's block entities for block code that needs the level while it holds the entity
//! (`level.getBlockEntity(pos) instanceof FooBlockEntity foo` followed by calls that read or mutate the level).

use super::ServerLevel;
use crate::world::block_entity::BlockEntity;
use crate::world::level::LevelAccessor;
use mc_core::pos::BlockPos;
use mc_data::block::BlockState;
use mc_nbt::CompoundTag;

impl ServerLevel {
    /// `getBlockEntity(pos) instanceof T`: runs `f` with the block entity taken out of its chunk (pending NBT is promoted
    /// first, like `Level.getBlockEntity`); `None` when there is no block entity of type `T` at `pos`.
    /// `f` must not change the block at `pos` (the entity is not in the chunk to be updated or kept).
    pub fn with_typed_block_entity<T: BlockEntity, R>(
        &mut self,
        pos: BlockPos,
        f: impl FnOnce(&mut ServerLevel, &mut T) -> R,
    ) -> Option<R> {
        if !self.get_block_entity_immediate(pos)?.as_any().is::<T>() {
            return None;
        }
        self.with_block_entity(pos, |level, be| {
            f(
                level,
                be.as_any_mut()
                    .downcast_mut::<T>()
                    .expect("type checked above"),
            )
        })
    }

    /// `BlockEntity.setChanged()` of the entity at `pos` whose cached block state is `state`:
    /// `blockEntityChanged(pos)` and the comparator update.
    pub fn block_entity_set_changed(&mut self, pos: BlockPos, state: BlockState) {
        self.block_entity_changed(pos);
        if !state.is_air() {
            self.update_neighbour_for_output_signal(pos, state.block());
        }
    }

    /// `blockEntity.loadAdditional(input)` for the block entity at `pos` of a level (`this.level != null`): the type's load
    /// with its level side effects ([`BlockEntity::load_additional_in_level`]) and the block state change it asks for.
    /// Returns whether a block entity existed.
    pub fn load_block_entity_additional_in_level(
        &mut self,
        pos: BlockPos,
        tag: &CompoundTag,
    ) -> bool {
        if self.get_block_entity_immediate(pos).is_none() {
            return false;
        }
        let follow_up = self
            .with_block_entity(pos, |level, be| be.load_additional_in_level(level, tag))
            .flatten();
        if let Some(state) = follow_up {
            self.set_block(pos, state, 2, 512);
        }
        true
    }

    /// `blockEntity.loadWithComponents(input)` of a block entity attached to a level (`loadAdditional` as above, then the
    /// `components`).
    pub fn load_block_entity_in_level(&mut self, pos: BlockPos, tag: &CompoundTag) -> bool {
        if !self.load_block_entity_additional_in_level(pos, tag) {
            return false;
        }
        if let Some(be) = self.get_block_entity_immediate(pos) {
            be.base_mut().components = crate::world::block_entity::read_codec(
                tag,
                "components",
                &mc_protocol::components::DataComponentMap::codec(),
            )
            .unwrap_or_default();
        }
        true
    }
}
