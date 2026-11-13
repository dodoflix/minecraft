//! `BaseContainerBlockEntity` / `RandomizableContainerBlockEntity` as `MenuProvider` for the dispenser, dropper, hopper and
//! crafter: the lock check, the loot table unpack, `sendChestLockedNotifications`, then the menu over the block entity
//! container.

use crate::world::block_entity::HopperBlockEntity;
use crate::world::block_entity::crafter_block_entity::CrafterBlockEntity;
use crate::world::block_entity::crafter_menu::CrafterMenu;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_game_core::world::block_entity::base_container::{
    BaseContainer, send_chest_locked_notifications,
};
use mc_game_core::world::block_entity::randomizable_container::RandomizableContainerData;
use mc_game_core::world::block_entity::{BlockEntity, DispenserBlockEntity};
use mc_game_core::world::inventory::{
    AbstractContainerMenu, ContainerRef, DispenserMenu, HopperMenu, MenuPlayer, MenuProvider,
};
use mc_game_core::world::level::BlockGetter;
use mc_text::Component;

/// The `lockKey`/`name`/`lootTable` of a container block entity this module can open.
fn container_state(be: &dyn BlockEntity) -> Option<(&BaseContainer, &RandomizableContainerData)> {
    if let Some(hopper) = be.as_any().downcast_ref::<HopperBlockEntity>() {
        return Some((&hopper.container, &hopper.loot));
    }
    if let Some(crafter) = be.as_any().downcast_ref::<CrafterBlockEntity>() {
        return Some((&crafter.container, &crafter.loot));
    }
    be.as_any()
        .downcast_ref::<DispenserBlockEntity>()
        .map(|d| (&d.container, &d.loot))
}

#[derive(Clone, Copy)]
enum Kind {
    Hopper,
    Dispenser,
    Crafter,
}

/// `MenuProvider` of the block entity at `pos` (`getDisplayName()` = `getName()`).
pub struct BlockEntityMenuProvider {
    pos: BlockPos,
    title: Component,
    kind: Kind,
}

impl BlockEntityMenuProvider {
    /// The provider of the dispenser/dropper/hopper/crafter at `pos` (`None` for other block entities).
    pub fn of(level: &dyn BlockGetter, pos: BlockPos) -> Option<Self> {
        let be = level.get_block_entity(pos)?;
        let (base, _) = container_state(be)?;
        let (default_name, kind) = if let Some(h) = be.as_any().downcast_ref::<HopperBlockEntity>()
        {
            (
                Component::translatable(h.default_name_key(), vec![]),
                Kind::Hopper,
            )
        } else if be.as_any().is::<CrafterBlockEntity>() {
            (CrafterBlockEntity::get_default_name(), Kind::Crafter)
        } else {
            let key = be
                .as_any()
                .downcast_ref::<DispenserBlockEntity>()?
                .default_name_key();
            (Component::translatable(key, vec![]), Kind::Dispenser)
        };
        Some(Self {
            pos,
            title: base.get_name(default_name),
            kind,
        })
    }
}

impl MenuProvider for BlockEntityMenuProvider {
    fn get_display_name(&self) -> Component {
        self.title.clone()
    }

    /// `RandomizableContainerBlockEntity.createMenu(containerId, inventory, player)`.
    fn create_menu(
        &self,
        container_id: i32,
        player: &mut dyn MenuPlayer,
    ) -> Option<Box<dyn AbstractContainerMenu>> {
        let pos = self.pos;
        let can_open = {
            let be = player.level_ref()?.get_block_entity(pos)?;
            let (base, loot) = container_state(be)?;
            (loot.loot_table.is_none() || !player.is_spectator()) && base.can_open(&*player)
        };
        if can_open {
            player.with_server_player(&mut |level, server_player| {
                level.with_block_entity(pos, |level, be| {
                    be.unpack_loot_table(level, Some(&mut *server_player))
                });
            });
            Some(match self.kind {
                Kind::Hopper => Box::new(HopperMenu::new(
                    container_id,
                    player,
                    ContainerRef::block_entity(pos, 5),
                )),
                Kind::Dispenser => Box::new(DispenserMenu::new(
                    container_id,
                    player,
                    ContainerRef::block_entity(pos, 9),
                )),
                Kind::Crafter => Box::new(CrafterMenu::new(container_id, player, pos)),
            })
        } else {
            if !player.is_spectator() {
                let title = self.title.clone();
                player.with_server_player(&mut |level, server_player| {
                    send_chest_locked_notifications(
                        level,
                        Vec3::at_center_of(&pos),
                        server_player,
                        &title,
                    );
                });
            }
            None
        }
    }
}
