//! `net.minecraft.world.level.block.BeaconBlock` (`BaseEntityBlock` + `BeaconBeamBlock`: the beam color of the block is
//! `block_entity::beacon_beam_color`).
use super::prelude::*;
use crate::server::level::ServerLevel;
use crate::stats::stats::Stats;
use crate::world::block_entity::BeaconBlockEntity;
use crate::world::block_entity::base_container::send_chest_locked_notifications;
use crate::world::entity::{Entity, ServerPlayer};
use crate::world::inventory::{BeaconMenu, ContainerLevelAccess, SimpleMenuProvider};
use crate::world::item::InteractionResult;
use mc_core::phys::{BlockHitResult, Vec3};

pub struct BeaconBlock;

impl BlockBehaviour for BeaconBlock {
    fn use_without_item(&self, _state: BlockState, level: &mut ServerLevel, pos: BlockPos, player: &mut ServerPlayer, _hit: &BlockHitResult) -> InteractionResult {
        let Some((name, unlocked)) = level.get_block_entity(pos).and_then(|be| be.as_any().downcast_ref::<BeaconBlockEntity>()).map(|beacon| {
            // `lockKey.canUnlock(player)`
            let unlocked = match &beacon.lock_key {
                None => true,
                Some(lock) => player.is_spectator() || lock.predicate.test(player.player.inventory.get_selected_item()),
            };
            (beacon.get_name(), unlocked)
        }) else {
            return InteractionResult::SUCCESS;
        };
        if unlocked {
            let provider = SimpleMenuProvider::new(name, move |id, _player| Some(Box::new(BeaconMenu::new(id, ContainerLevelAccess::create(pos)))));
            player.open_menu(level, &provider);
            player.award_stat_once(level, Stats::INTERACT_WITH_BEACON);
        } else {
            send_chest_locked_notifications(level, Vec3::at_center_of(&pos), player, &name);
        }
        InteractionResult::SUCCESS
    }
}

register_block_class!("BeaconBlock", |_| Box::new(BeaconBlock));
