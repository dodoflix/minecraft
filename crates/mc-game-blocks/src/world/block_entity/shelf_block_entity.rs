//! `ShelfBlockEntity` (`ListBackedContainer` of 3 items and the `align_items_to_bottom` flag).

use linkme::distributed_slice;
use mc_core::pos::BlockPos;
use mc_data::block::BlockState;
use mc_data::{BlockEntityType, GameEvent};
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::block_entity::ComponentReader;
use mc_game_core::world::block_entity::{
    BLOCK_ENTITY_TYPES, BlockEntity, BlockEntityData, BlockEntityRegistration, deferred,
};
use mc_game_core::world::container::{Container, helper};
use mc_game_core::world::item::ItemStack;
use mc_game_core::world::level::gameevent::GameEventContext;
use mc_game_core::{block_entity_base, impl_items_container};
use mc_nbt::CompoundTag;
use mc_protocol::components::{CONTAINER, ItemContainerContents, map::DataComponentMapBuilder};
use mc_protocol::packets::play::level::ClientboundBlockEntityDataPacket;

const MAX_ITEMS: usize = 3;

pub struct ShelfBlockEntity {
    base: BlockEntityData,
    items: Vec<ItemStack>,
    align_items_to_bottom: bool,
}

impl ShelfBlockEntity {
    pub fn new(pos: BlockPos, state: BlockState) -> Self {
        Self {
            base: BlockEntityData::new(BlockEntityType::SHELF, pos, state),
            items: vec![ItemStack::empty(); MAX_ITEMS],
            align_items_to_bottom: false,
        }
    }

    /// `swapItemNoUpdate(slot, heldItemStack)`.
    pub fn swap_item_no_update(&mut self, slot: usize, held: ItemStack) -> ItemStack {
        let retrieved = self.remove_item_no_update(slot);
        self.set_item_no_update(slot, held);
        retrieved
    }

    /// `ListBackedContainer.setItemNoUpdate(slot, stack)`.
    pub fn set_item_no_update(&mut self, slot: usize, mut stack: ItemStack) {
        let max = self.get_max_stack_size_for(&stack);
        stack.limit_size(max);
        self.items[slot] = stack;
    }

    /// `setChanged(event)`: `BlockEntity.setChanged()`, the game event, then `sendBlockUpdated`. Only valid inside
    /// `with_block_entity`: the level side runs once the entity is back in its chunk (comparators read it, see `deferred`).
    pub fn set_changed_with_event(&mut self, event: Option<GameEvent>) {
        self.base.changed = false;
        let (pos, state) = (self.base.pos, self.base.block_state);
        deferred::push(move |level| notify_changed(level, pos, state, event));
    }
}

fn notify_changed(
    level: &mut ServerLevel,
    pos: BlockPos,
    state: BlockState,
    event: Option<GameEvent>,
) {
    mc_game_core::world::block_entity::set_changed(level, pos, state);
    if let Some(event) = event {
        level.game_event_at_block(event, pos, &GameEventContext::of_state(Some(state)));
    }
    level.send_block_updated(pos, state, state, 3);
}

impl_items_container!(ShelfBlockEntity, items {
    /// `ListBackedContainer.canPlaceItem`.
    fn can_place_item(&self, slot: usize, stack: &ItemStack) -> bool {
        self.get_item(slot).is_empty()
            || self.get_item(slot).count() < self.get_max_stack_size_for(stack)
    }
});

impl BlockEntity for ShelfBlockEntity {
    block_entity_base!();

    fn load_additional(&mut self, tag: &CompoundTag) {
        self.items = vec![ItemStack::empty(); MAX_ITEMS];
        helper::load_all_items(tag, &mut self.items);
        self.align_items_to_bottom = tag.get_boolean_or("align_items_to_bottom", false);
    }

    fn save_additional(&self, tag: &mut CompoundTag) {
        if let Some(list) = helper::save_all_items(&self.items, true) {
            tag.put("Items", mc_nbt::Tag::List(list));
        }
        tag.put_boolean("align_items_to_bottom", self.align_items_to_bottom);
    }

    /// `getUpdateTag`: the items and the alignment flag, without the block entity metadata.
    fn get_update_tag(&self) -> CompoundTag {
        let mut tag = CompoundTag::new();
        self.save_additional(&mut tag);
        tag
    }

    fn get_update_packet(&self) -> Option<ClientboundBlockEntityDataPacket> {
        Some(ClientboundBlockEntityDataPacket {
            pos: self.base.pos,
            r#type: self.base.ty,
            tag: self.get_update_tag(),
        })
    }

    /// `removeComponentsFromTag(output)`.
    fn remove_components_from_tag(&self, tag: &mut CompoundTag) {
        tag.remove("Items");
    }

    fn apply_implicit_components(&mut self, components: &ComponentReader) {
        match components.get(&CONTAINER) {
            Some(c) => c.copy_into(&mut self.items),
            None => ItemContainerContents::default().copy_into(&mut self.items),
        }
    }

    fn collect_implicit_components(&self, builder: &mut DataComponentMapBuilder) {
        *builder = std::mem::take(builder).set(
            CONTAINER,
            ItemContainerContents::from_item_stacks(&self.items),
        );
    }

    fn as_container(&self) -> Option<&dyn Container> {
        Some(self)
    }

    fn as_container_mut(&mut self) -> Option<&mut dyn Container> {
        Some(self)
    }

    fn container_pending(&self) -> bool {
        self.base.changed
    }

    /// `setChanged()` = `setChanged(BLOCK_ACTIVATE)`.
    fn container_accessed(&mut self, _level: &mut ServerLevel) {
        if self.base.changed {
            self.set_changed_with_event(Some(GameEvent::BLOCK_ACTIVATE));
        }
    }
}

#[distributed_slice(BLOCK_ENTITY_TYPES)]
static SHELF: BlockEntityRegistration = BlockEntityRegistration {
    ty: BlockEntityType::SHELF,
    create: |pos, state| Box::new(ShelfBlockEntity::new(pos, state)),
};
