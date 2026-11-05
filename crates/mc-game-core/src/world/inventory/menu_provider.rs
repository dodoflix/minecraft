//! `MenuProvider` / `SimpleMenuProvider` (`MenuConstructor`): what `ServerPlayer::open_menu` opens.

use super::{AbstractContainerMenu, MenuPlayer};
use mc_text::Component;

/// `MenuProvider` (+ `MenuConstructor.createMenu`): the player's inventory is reached through the `MenuPlayer`
/// (menus address it with `ContainerKey::PlayerInventory`). `None` = vanilla's `null` menu (not allowed to open).
pub trait MenuProvider {
    /// `getDisplayName()`.
    fn get_display_name(&self) -> Component;
    fn create_menu(&self, container_id: i32, player: &mut dyn MenuPlayer) -> Option<Box<dyn AbstractContainerMenu>>;
}

/// `SimpleMenuProvider(menuConstructor, title)`.
pub struct SimpleMenuProvider<F> {
    title: Component,
    constructor: F,
}

impl<F: Fn(i32, &mut dyn MenuPlayer) -> Option<Box<dyn AbstractContainerMenu>>> SimpleMenuProvider<F> {
    pub fn new(title: Component, constructor: F) -> Self {
        Self { title, constructor }
    }
}

impl<F: Fn(i32, &mut dyn MenuPlayer) -> Option<Box<dyn AbstractContainerMenu>>> MenuProvider for SimpleMenuProvider<F> {
    fn get_display_name(&self) -> Component {
        self.title.clone()
    }

    fn create_menu(&self, container_id: i32, player: &mut dyn MenuPlayer) -> Option<Box<dyn AbstractContainerMenu>> {
        (self.constructor)(container_id, player)
    }
}
