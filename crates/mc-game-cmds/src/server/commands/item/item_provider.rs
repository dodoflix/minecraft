//! `net.minecraft.world.item.ItemProvider` / `ArrayItemProvider`: the items a `/item replace|fill|override` hands out, in
//! order. As an `Iterator` of fresh copies: `next()` is `hasNext() ? next() : none`.
use mc_protocol::components::ItemStack;

/// `ItemProvider.of(items)` / `ItemProvider.cycle(items)`, optionally `orElseProvide(fallback)`.
pub struct ItemProvider {
    items: Vec<ItemStack>,
    position: usize,
    cycled: bool,
    fallback: Option<ItemStack>,
}

impl ItemProvider {
    /// `ItemProvider.of(items)`.
    pub fn of(items: Vec<ItemStack>) -> Self {
        Self {
            items,
            position: 0,
            cycled: false,
            fallback: None,
        }
    }

    /// `ItemProvider.cycle(items)`.
    pub fn cycle(items: Vec<ItemStack>) -> Self {
        Self {
            items,
            position: 0,
            cycled: true,
            fallback: None,
        }
    }

    /// `orElseProvide(fallbackItem)` (a provider that already falls back keeps its fallback).
    pub fn or_else_provide(mut self, fallback: ItemStack) -> Self {
        if self.fallback.is_none() {
            self.fallback = Some(fallback);
        }
        self
    }

    /// `restart()`.
    pub fn restart(&mut self) {
        self.position = 0;
    }
}

impl Iterator for ItemProvider {
    type Item = ItemStack;

    fn next(&mut self) -> Option<ItemStack> {
        let has_next = if self.cycled {
            !self.items.is_empty()
        } else {
            self.position < self.items.len()
        };
        if has_next {
            let index = if self.cycled {
                self.position % self.items.len()
            } else {
                self.position
            };
            self.position += 1;
            return Some(self.items[index].copy());
        }
        self.fallback.as_ref().map(ItemStack::copy)
    }
}
