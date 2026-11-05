//! `ContainerRef`: the container a menu is built around — a single container (block entity, entity, ender chest) or
//! the ordered parts of a `CompoundContainer` (double chest).

use super::{ContainerKey, MenuPlayer};
use mc_core::pos::BlockPos;

#[derive(Clone)]
pub struct ContainerRef {
    /// `(container, size)`; the compound's slot `i` lives in the first part whose cumulative size exceeds `i`.
    parts: Vec<(ContainerKey, usize)>,
}

impl ContainerRef {
    pub fn single(key: ContainerKey, size: usize) -> Self {
        Self { parts: vec![(key, size)] }
    }

    /// The `Container` of the block entity at `pos` (`size` = `getContainerSize()`).
    pub fn block_entity(pos: BlockPos, size: usize) -> Self {
        Self::single(ContainerKey::BlockEntity(pos), size)
    }

    /// The `Container` of an entity.
    pub fn entity(id: crate::world::entity::EntityId, size: usize) -> Self {
        Self::single(ContainerKey::Entity(id), size)
    }

    /// `player.getEnderChestInventory()`.
    pub fn player_ender_chest() -> Self {
        Self::single(ContainerKey::PlayerEnderChest, 27)
    }

    /// `new CompoundContainer(first, second, ...)`.
    pub fn compound(parts: impl IntoIterator<Item = ContainerRef>) -> Self {
        Self { parts: parts.into_iter().flat_map(|p| p.parts).collect() }
    }

    /// `getContainerSize()`.
    pub fn size(&self) -> usize {
        self.parts.iter().map(|(_, n)| n).sum()
    }

    /// Container and in-container index of compound slot `index`.
    pub fn locate(&self, index: usize) -> (ContainerKey, usize) {
        let mut rest = index;
        for (key, size) in &self.parts {
            if rest < *size {
                return (key.clone(), rest);
            }
            rest -= size;
        }
        panic!("slot {index} outside a container of {} slots", self.size());
    }

    /// `checkContainerSize(container, minSize)`.
    pub fn check_size(&self, min_size: usize) {
        assert!(self.size() >= min_size, "Container size {} is smaller than expected {}", self.size(), min_size);
    }

    /// `container.stillValid(player)` (`CompoundContainer`: every part).
    pub fn still_valid(&self, player: &dyn MenuPlayer) -> bool {
        self.parts.iter().all(|(key, _)| key.still_valid(player))
    }

    /// `container.startOpen(player)` (every part).
    pub fn start_open(&self, player: &mut dyn MenuPlayer) {
        for (key, _) in &self.parts {
            key.start_open(player);
        }
    }

    /// `container.stopOpen(player)` (every part).
    pub fn stop_open(&self, player: &mut dyn MenuPlayer) {
        for (key, _) in &self.parts {
            key.stop_open(player);
        }
    }

    /// Whether `key` is one of the parts (`AbstractContainerMenu`-level "is this menu showing that container").
    pub fn contains(&self, key: &ContainerKey) -> bool {
        self.parts.iter().any(|(k, _)| k.same_container(key))
    }
}
