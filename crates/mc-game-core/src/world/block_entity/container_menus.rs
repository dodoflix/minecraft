//! `MenuProvider`s of the container blocks: chests (single and double), barrels, shulker boxes and ender chests.
//! `createMenu` = `canOpen` (lock / loot table + spectator), `unpackLootTable(player)`, then the `ChestMenu` over the
//! block entity container(s).

use super::barrel_block_entity::BarrelBlockEntity;
use super::base_container::send_chest_locked_notifications;
use super::chest_block_entity::ChestBlockEntity;
use super::shulker_box_block_entity::ShulkerBoxBlockEntity;
use crate::world::level::BlockGetter;
use crate::world::inventory::{AbstractContainerMenu, ChestMenu, ContainerRef, MenuPlayer, MenuProvider, ShulkerBoxMenu};
use crate::world::level::block::impls::chest_block::get_connected_direction;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_text::Component;

/// What the provider opens.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ContainerBlockKind {
    /// One chest, or two (`first`, `second` of the `CompoundContainer`).
    Chest,
    Barrel,
    ShulkerBox,
}

pub struct BlockEntityMenuProvider {
    pub kind: ContainerBlockKind,
    pub first: BlockPos,
    pub second: Option<BlockPos>,
    pub title: Component,
}

impl BlockEntityMenuProvider {
    /// `canOpen(player)` of every involved block entity.
    fn can_open(&self, player: &dyn MenuPlayer) -> bool {
        let Some(level) = player.level_ref() else { return false };
        [Some(self.first), self.second].into_iter().flatten().all(|pos| {
            let Some(be) = level.get_block_entity(pos) else { return false };
            let any = be.as_any();
            match self.kind {
                ContainerBlockKind::Chest => any.downcast_ref::<ChestBlockEntity>().is_some_and(|c| c.can_open(player)),
                ContainerBlockKind::Barrel => any.downcast_ref::<BarrelBlockEntity>().is_some_and(|c| c.can_open(player)),
                ContainerBlockKind::ShulkerBox => any.downcast_ref::<ShulkerBoxBlockEntity>().is_some_and(|c| c.can_open(player)),
            }
        })
    }
}

impl MenuProvider for BlockEntityMenuProvider {
    fn get_display_name(&self) -> Component {
        self.title.clone()
    }

    fn create_menu(&self, container_id: i32, player: &mut dyn MenuPlayer) -> Option<Box<dyn AbstractContainerMenu>> {
        if self.can_open(&*player) {
            let (first, second) = (self.first, self.second);
            player.with_server_player(&mut |level, sp| {
                for pos in [Some(first), second].into_iter().flatten() {
                    level.with_block_entity(pos, |level, be| be.unpack_loot_table(&*level, Some(&*sp as &dyn crate::world::entity::Entity)));
                }
            });
            let single = |pos| ContainerRef::block_entity(pos, 27);
            Some(match (self.kind, self.second) {
                (ContainerBlockKind::Chest, Some(second)) => Box::new(ChestMenu::six_rows(container_id, player, ContainerRef::compound([single(self.first), single(second)]))),
                (ContainerBlockKind::ShulkerBox, _) => Box::new(ShulkerBoxMenu::new(container_id, player, single(self.first))),
                _ => Box::new(ChestMenu::three_rows(container_id, player, single(self.first))),
            })
        } else {
            if !player.is_spectator() {
                let center = match (self.kind, self.second) {
                    (ContainerBlockKind::Chest, Some(_)) => {
                        let state = player.level_ref().map(|l| crate::world::level::BlockGetter::get_block_state(l, self.first));
                        let first_center = Vec3::at_center_of(&self.first);
                        match state {
                            Some(state) => {
                                let d = get_connected_direction(state);
                                first_center.add_xyz(f64::from(d.get_step_x()) / 2.0, 0.0, f64::from(d.get_step_z()) / 2.0)
                            }
                            None => first_center,
                        }
                    }
                    _ => Vec3::at_center_of(&self.first),
                };
                let title = self.title.clone();
                player.with_server_player(&mut |level, sp| send_chest_locked_notifications(level, center, sp, &title));
            }
            None
        }
    }
}
