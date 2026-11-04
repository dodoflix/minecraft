//! `net.minecraft.world.inventory`: menus, slots, click handling, remote-slot synchronisation.
//!
//! Ownership model (see ARCHITECTURE.md): the player owns its `Inventory`; an open menu is taken out of the player
//! while it runs (`PlayerMenus::with_current`) and receives the player as `&mut dyn MenuPlayer`. Slots refer to their
//! container by [`ContainerKey`]: the player's inventory, a container local to the menu, or a shared handle (block
//! entity / ender chest) — resolved on every access through a [`SlotEnv`].

mod armor_slot;
mod click;
mod container_data;
mod crafting;
mod inventory_menu;
mod item_click;
mod level_access;
mod menu;
mod menu_player;
mod player_menus;
mod remote_slot;
mod slot;

pub use armor_slot::*;
pub use click::*;
pub use container_data::*;
pub use crafting::*;
pub use inventory_menu::*;
pub use level_access::*;
pub use menu::*;
pub use menu_player::*;
pub use player_menus::*;
pub use remote_slot::*;
pub use slot::*;
