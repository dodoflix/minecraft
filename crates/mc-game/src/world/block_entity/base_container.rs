//! The state and helpers of vanilla's `BaseContainerBlockEntity` (`lockKey`, `name`, implicit item components, locked
//! notification). Rust block entities embed a [`BaseContainer`] (composition) and implement `Container` themselves.

use super::BlockEntityData;
use crate::server::level::ServerLevel;
use crate::world::entity::ServerPlayer;
use crate::world::inventory::MenuPlayer;
use crate::world::item::ItemStack;
use crate::world::level::BlockGetter;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_data::block::BlockState;
use mc_nbt::CompoundTag;
use mc_protocol::components::{CONTAINER, CUSTOM_NAME, DataComponentMapBuilder, ItemContainerContents, LOCK, LockCode, component_codec};
use mc_protocol::packets::play::{ClientboundSystemChatPacket, SoundSource};
use mc_text::Component;

/// `BaseContainerBlockEntity.lockKey` + `name`.
#[derive(Clone, Debug, Default)]
pub struct BaseContainer {
    /// `None` = `LockCode.NO_LOCK` (the shared instance; a deserialized empty predicate is a distinct, equal value).
    pub lock_key: Option<LockCode>,
    pub name: Option<Component>,
}

/// `LockCode.NO_LOCK.predicate()`: the empty `ItemPredicate` (matches every stack).
fn no_lock_predicate() -> mc_protocol::components::ItemPredicate {
    mc_dfu::parse_nbt(&mc_protocol::components::ItemPredicate::codec(), &mc_nbt::Tag::Compound(CompoundTag::new())).result().expect("empty item predicate")
}

impl BaseContainer {
    /// `loadAdditional`: `LockCode.fromTag(input)` and `parseCustomNameSafe(input, "CustomName")`.
    pub fn load(&mut self, tag: &CompoundTag) {
        self.lock_key = super::read_codec(tag, "lock", &LockCode::codec());
        self.name = super::read_codec(tag, "CustomName", &component_codec());
    }

    /// `saveAdditional`: `lockKey.addToTag` (not written for `NO_LOCK`) and `storeNullable("CustomName", ..)`.
    pub fn save(&self, tag: &mut CompoundTag) {
        if let Some(lock) = &self.lock_key
            && let Ok(encoded) = mc_dfu::encode_nbt(&LockCode::codec(), lock).into_result()
        {
            tag.put("lock", encoded);
        }
        if let Some(name) = &self.name
            && let Ok(encoded) = mc_dfu::encode_nbt(&component_codec(), name).into_result()
        {
            tag.put("CustomName", encoded);
        }
    }

    /// `getName()`.
    pub fn get_name(&self, default_name: Component) -> Component {
        self.name.clone().unwrap_or(default_name)
    }

    /// `getCustomName()`.
    pub fn get_custom_name(&self) -> Option<&Component> {
        self.name.as_ref()
    }

    /// `isLocked()`: `!lockKey.equals(NO_LOCK)`.
    pub fn is_locked(&self) -> bool {
        self.lock_key.as_ref().is_some_and(|l| l.predicate != no_lock_predicate())
    }

    /// `canOpen(player)`: `lockKey.canUnlock(player)` = `player.isSpectator() || unlocksWith(mainHandItem)`.
    pub fn can_open(&self, player: &dyn MenuPlayer) -> bool {
        match &self.lock_key {
            None => true,
            Some(lock) => player.is_spectator() || lock.predicate.test(player.inventory().get_selected_item()),
        }
    }

    /// `applyImplicitComponents(components)`: `custom_name`, `lock`, `container` (copied into `items`).
    pub fn apply_implicit_components(&mut self, stack: &ItemStack, items: &mut [ItemStack]) {
        self.name = stack.get(&CUSTOM_NAME).cloned();
        self.lock_key = stack.get(&LOCK).cloned();
        match stack.get(&CONTAINER) {
            Some(contents) => contents.copy_into(items),
            None => ItemContainerContents::default().copy_into(items),
        }
    }

    /// `collectImplicitComponents(components)`.
    pub fn collect_implicit_components(&self, mut components: DataComponentMapBuilder, items: &[ItemStack]) -> DataComponentMapBuilder {
        if let Some(name) = &self.name {
            components = components.set(CUSTOM_NAME, name.clone());
        }
        if self.is_locked()
            && let Some(lock) = &self.lock_key
        {
            components = components.set(LOCK, lock.clone());
        }
        components.set(CONTAINER, ItemContainerContents::from_items(items))
    }
}

/// `BaseContainerBlockEntity.sendChestLockedNotifications(pos, player, displayName)`.
pub fn send_chest_locked_notifications(level: &mut ServerLevel, pos: Vec3, player: &mut ServerPlayer, display_name: &Component) {
    player.send(&ClientboundSystemChatPacket { content: Component::translatable_with("container.isLocked", vec![display_name.clone()]), overlay: true });
    level.play_sound_at(None, pos.x, pos.y, pos.z, mc_data::SoundEvent::CHEST_LOCKED, SoundSource::Blocks, 1.0, 1.0);
}

/// `BlockEntity.setChanged(level, pos, state)`: `level.blockEntityChanged(pos)` and, for a non-air state,
/// `level.updateNeighbourForOutputSignal(pos, state.getBlock())`.
pub fn set_changed(level: &mut ServerLevel, pos: BlockPos, state: BlockState) {
    level.block_entity_changed(pos);
    if !state.is_air() {
        level.update_neighbour_for_output_signal(pos, state.block());
    }
}

/// `BlockEntity.setChanged()` for a block entity currently taken out of its chunk: `base` carries the position/state.
pub fn set_changed_of(level: &mut ServerLevel, base: &BlockEntityData) {
    set_changed(level, base.pos, base.block_state);
}
