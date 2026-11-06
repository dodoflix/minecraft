//! `ContainerRef`: the container a menu is built around — a single container (block entity, entity, ender chest) or
//! the ordered parts of a `CompoundContainer` (double chest).

use super::{ContainerKey, MenuPlayer};
use mc_core::pos::BlockPos;

#[derive(Clone)]
pub struct ContainerRef {
    /// `(container, size)`; the compound's slot `i` lives in the first part whose cumulative size exceeds `i`.
    parts: Vec<(ContainerKey, usize)>,
    /// Block entity instance each part was bound to by `start_open` (`stillValidBlockEntity` compares the object, not
    /// the position): a part whose block entity was replaced meanwhile is no longer valid.
    bound: std::cell::RefCell<Vec<Option<u64>>>,
}

/// The instance id of the block entity at `pos` as seen by `player`.
fn instance_at(player: &dyn MenuPlayer, key: &ContainerKey) -> Option<u64> {
    use crate::world::level::BlockGetter;
    match key {
        ContainerKey::BlockEntity(pos) => player
            .level_ref()?
            .get_block_entity(*pos)
            .map(|be| be.base().instance_id),
        _ => None,
    }
}

impl ContainerRef {
    pub fn single(key: ContainerKey, size: usize) -> Self {
        Self {
            parts: vec![(key, size)],
            bound: std::cell::RefCell::new(vec![None]),
        }
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
        let parts: Vec<_> = parts.into_iter().flat_map(|p| p.parts).collect();
        let bound = std::cell::RefCell::new(vec![None; parts.len()]);
        Self { parts, bound }
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
        assert!(
            self.size() >= min_size,
            "Container size {} is smaller than expected {}",
            self.size(),
            min_size
        );
    }

    /// `container.stillValid(player)` (`CompoundContainer`: every part).
    pub fn still_valid(&self, player: &dyn MenuPlayer) -> bool {
        let bound = self.bound.borrow();
        self.parts
            .iter()
            .zip(bound.iter())
            .all(|((key, _), bound)| {
                key.still_valid(player)
                    && bound.is_none_or(|id| instance_at(player, key) == Some(id))
            })
    }

    /// `container.startOpen(player)` (every part).
    pub fn start_open(&self, player: &mut dyn MenuPlayer) {
        for (i, (key, _)) in self.parts.iter().enumerate() {
            key.start_open(player);
            self.bound.borrow_mut()[i] = instance_at(&*player, key);
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
