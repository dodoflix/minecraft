//! `ServerPlayer.openMenu/closeContainer/initMenu` and the lifecycle hooks that close menus (per-tick `stillValid`,
//! removal, dimension change). `doCloseContainer` itself lives in `item_use.rs`.

use super::ServerPlayer;
use super::menu_ctx::{PlayerCtx, ServerPlayerMenuInit};
use crate::server::level::ServerLevel;
use crate::translatable;
use crate::world::entity::Entity;
use crate::world::inventory::{AbstractContainerMenu, MenuProvider, PlayerMenus};
use mc_protocol::packets::play::{ClientboundContainerClosePacket, ClientboundMountScreenOpenPacket, ClientboundOpenScreenPacket, ClientboundSystemChatPacket};
use mc_text::ChatFormatting;

impl ServerPlayer {
    /// `ServerPlayer.openMenu(provider)`: the new container id (`OptionalInt`), `None` when the provider declined.
    pub fn open_menu(&mut self, level: &mut ServerLevel, provider: &dyn MenuProvider) -> Option<i32> {
        if self.player.menus.has_container_open() {
            self.close_container(level);
        }
        let container_id = self.player.menus.next_container_counter();
        let created = {
            let mut ctx = PlayerCtx { player: self, level };
            provider.create_menu(container_id, &mut ctx)
        };
        let Some(menu) = created else {
            if self.is_spectator() {
                let message = translatable!("container.spectatorCantOpen").with_format(ChatFormatting::Red);
                self.send(&ClientboundSystemChatPacket { content: message, overlay: true });
            }
            return None;
        };
        self.send(&ClientboundOpenScreenPacket { container_id: menu.base().container_id, menu_type: menu.base().get_type(), title: provider.get_display_name() });
        self.init_menu(level, menu);
        Some(container_id)
    }

    /// `openHorseInventory` / `openNautilusInventory`: the mount screen packet, then the menu built by the caller from
    /// the returned container id (`HorseInventoryMenu`/`NautilusInventoryMenu`).
    pub fn open_mount_inventory(
        &mut self,
        level: &mut ServerLevel,
        mount_id: i32,
        inventory_columns: i32,
        create: impl FnOnce(i32, &mut PlayerCtx) -> Box<dyn AbstractContainerMenu>,
    ) {
        if self.player.menus.has_container_open() {
            self.close_container(level);
        }
        let container_id = self.player.menus.next_container_counter();
        self.send(&ClientboundMountScreenOpenPacket { container_id, inventory_columns, entity_id: mount_id });
        let menu = {
            let mut ctx = PlayerCtx { player: self, level };
            create(container_id, &mut ctx)
        };
        self.init_menu(level, menu);
    }

    /// `initMenu(menu)` + `containerMenu = menu`.
    fn init_menu(&mut self, level: &mut ServerLevel, menu: Box<dyn AbstractContainerMenu>) {
        let mut menus = std::mem::replace(&mut self.player.menus, PlayerMenus::new());
        match self.connection.clone() {
            Some(connection) => {
                let init = ServerPlayerMenuInit { connection };
                let mut ctx = PlayerCtx { player: self, level };
                menus.set_container_menu(menu, &init, &mut ctx);
            }
            None => menus.put_container_menu(menu),
        }
        self.player.menus = menus;
    }

    /// `closeContainer()`: tells the client, then `doCloseContainer()`.
    pub fn close_container(&mut self, level: &mut ServerLevel) {
        let id = self.player.menus.current_container_id();
        self.send(&ClientboundContainerClosePacket { container_id: id });
        self.do_close_container(level);
    }

    /// `if (!containerMenu.stillValid(this)) closeContainer()` (`ServerPlayer.tick` and `doTick`).
    pub fn close_container_if_invalid(&mut self, level: &mut ServerLevel) {
        let valid = {
            let mut ctx = PlayerCtx { player: self, level };
            PlayerMenus::with_current(&mut ctx, |c| &mut c.player.player.menus, |menu, c| menu.still_valid(&*c))
        };
        if !valid {
            self.close_container(level);
        }
    }

    /// The tail of `Player.remove(reason)`: `inventoryMenu.removed(this)`, then `doCloseContainer()` if a container
    /// is open (releases chest open counters, returns carried items, ...).
    pub fn close_menus_on_removal(&mut self, level: &mut ServerLevel) {
        let mut menus = std::mem::replace(&mut self.player.menus, PlayerMenus::new());
        {
            let mut ctx = PlayerCtx { player: self, level };
            menus.inventory_menu_mut().removed(&mut ctx);
        }
        self.player.menus = menus;
        if self.player.menus.has_container_open() {
            self.do_close_container(level);
        }
    }
}
