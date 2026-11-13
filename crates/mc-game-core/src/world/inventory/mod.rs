//! `net.minecraft.world.inventory`: menus, slots, click handling, remote-slot synchronisation.
//!
//! Ownership model (see ARCHITECTURE.md): the player owns its `Inventory`; an open menu is taken out of the player
//! while it runs (`PlayerMenus::with_current`) and receives the player as `&mut dyn MenuPlayer`. Slots refer to their
//! container by [`ContainerKey`]: the player's inventory, a container local to the menu, or a shared handle (block
//! entity / ender chest) — resolved on every access through a [`SlotEnv`].

pub mod anvil_menu;
mod armor_slot;
mod beacon_menu;
mod brewing_stand_menu;
mod cartography_table_menu;
mod click;
mod container_data;
mod container_menus;
mod container_ref;
mod crafting;
pub mod crafting_menu;
pub mod enchantment_menu;
pub mod grindstone_menu;
mod inventory_menu;
mod item_click;
mod item_combiner_menu;
mod lectern_menu;
mod level_access;
pub mod loom_menu;
mod menu;
mod menu_container;
mod menu_handlers;
mod menu_player;
mod menu_provider;
mod merchant_menu;
mod mount_menu;
mod player_ender_chest;
mod player_menus;
mod recipe_crafting_holder;
mod remote_slot;
mod slot;
mod stonecutter_menu;
mod synchronizer;

pub use anvil_menu::AnvilMenu;
pub use armor_slot::*;
pub use beacon_menu::*;
pub use brewing_stand_menu::BrewingStandMenu;
pub use cartography_table_menu::CartographyTableMenu;
pub use click::*;
pub use container_data::*;
pub use container_menus::*;
pub use container_ref::*;
pub use crafting::*;
pub use crafting_menu::CraftingMenu;
pub use enchantment_menu::EnchantmentMenu;
pub use grindstone_menu::GrindstoneMenu;
pub use inventory_menu::*;
pub use item_combiner_menu::*;
pub use lectern_menu::*;
pub use level_access::*;
pub use loom_menu::LoomMenu;
pub use menu::*;
pub use menu_container::*;
pub use menu_player::*;
pub use menu_provider::*;
pub use merchant_menu::*;
pub use mount_menu::{HorseInventoryMenu, MountInventory, NautilusInventoryMenu};
pub use player_ender_chest::*;
pub use player_menus::*;
pub use recipe_crafting_holder::RecipeCraftingHolder;
pub use remote_slot::*;
pub use slot::*;
pub use stonecutter_menu::StonecutterMenu;
pub use synchronizer::*;
