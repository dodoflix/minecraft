//! `net.minecraft.world.level.block.BeaconBlock` (`BaseEntityBlock` + `BeaconBeamBlock`: the beam color of the block is
//! `block_entity::beacon_beam_color`).
use crate::world::block_entity::BeaconBlockEntity;
use mc_core::phys::{BlockHitResult, Vec3};
use mc_game_core::register_block_class;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::stats::stats::Stats;
use mc_game_core::world::block_entity::base_container::send_chest_locked_notifications;
use mc_game_core::world::entity::{Entity, ServerPlayer};
use mc_game_core::world::inventory::{BeaconMenu, ContainerLevelAccess, SimpleMenuProvider};
use mc_game_core::world::item::InteractionResult;
use mc_game_core::world::level::block::impls::prelude::*;

pub struct BeaconBlock;

impl BlockBehaviour for BeaconBlock {
    /// `BeaconBlockEntity.createMenu` (`BaseEntityBlock.getMenuProvider`): the lock is checked by the caller; spectators
    /// always unlock.
    fn get_menu_provider(
        &self,
        _state: BlockState,
        level: &ServerLevel,
        pos: BlockPos,
    ) -> Option<Box<dyn mc_game_core::world::inventory::MenuProvider>> {
        let name = level
            .get_block_entity(pos)
            .and_then(|be| be.as_any().downcast_ref::<BeaconBlockEntity>())?
            .get_name();
        Some(Box::new(SimpleMenuProvider::new(
            name,
            move |id, _player| {
                Some(Box::new(BeaconMenu::new(
                    id,
                    ContainerLevelAccess::create(pos),
                )))
            },
        )))
    }
    fn use_without_item(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        player: &mut ServerPlayer,
        _hit: &BlockHitResult,
    ) -> InteractionResult {
        let Some((name, unlocked)) = level
            .get_block_entity(pos)
            .and_then(|be| be.as_any().downcast_ref::<BeaconBlockEntity>())
            .map(|beacon| {
                // `lockKey.canUnlock(player)`
                let unlocked = match &beacon.lock_key {
                    None => true,
                    Some(lock) => {
                        player.is_spectator()
                            || lock
                                .predicate
                                .test(player.player.inventory.get_selected_item())
                    }
                };
                (beacon.get_name(), unlocked)
            })
        else {
            return InteractionResult::SUCCESS;
        };
        if unlocked {
            if let Some(provider) = self.get_menu_provider(state, level, pos) {
                player.open_menu(level, provider.as_ref());
            }
            player.award_stat_once(level, Stats::INTERACT_WITH_BEACON);
        } else {
            send_chest_locked_notifications(level, Vec3::at_center_of(&pos), player, &name);
        }
        InteractionResult::SUCCESS
    }
}

register_block_class!("BeaconBlock", |_| Box::new(BeaconBlock));
