//! `net.minecraft.world.level.block.CartographyTableBlock`: `useWithoutItem` (stat and success; the `CartographyTableMenu` provider does not exist yet).
use mc_core::phys::BlockHitResult;
use mc_game_core::register_block_class;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::stats::stats::Stats;
use mc_game_core::world::entity::ServerPlayer;
use mc_game_core::world::item::InteractionResult;
use mc_game_core::world::level::block::impls::prelude::*;

pub struct CartographyTableBlock;

impl BlockBehaviour for CartographyTableBlock {
    /// `useWithoutItem`: `player.openMenu(state.getMenuProvider(level, pos))` + `INTERACT_WITH_CARTOGRAPHY_TABLE`. The menu provider is the
    /// menu hook: `getMenuProvider` returns none until the menu exists.
    fn use_without_item(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        player: &mut ServerPlayer,
        _hit: &BlockHitResult,
    ) -> InteractionResult {
        if let Some(provider) = self.get_menu_provider(state, level, pos) {
            player.open_menu(level, provider.as_ref());
        }
        player.award_stat_once(level, Stats::INTERACT_WITH_CARTOGRAPHY_TABLE);
        InteractionResult::SUCCESS
    }
}

register_block_class!("CartographyTableBlock", |_| Box::new(CartographyTableBlock));
