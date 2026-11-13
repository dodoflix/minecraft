//! `net.minecraft.world.item.trading.Merchant`: what a trading screen needs from the villager / wandering trader.
//!
//! The merchant is an entity; menus address it by id and reach it through `Entity::as_merchant[_mut]` while the entity
//! is taken out of the level (so every method that may touch the level gets it as a parameter). The trading player is
//! an entity id too.

use super::MerchantOffers;
use crate::server::level::ServerLevel;
use crate::world::entity::EntityId;
use crate::world::entity::player::ServerPlayer;
use crate::world::inventory::{MenuPlayer, MerchantMenu, SimpleMenuProvider};
use crate::world::item::ItemStack;
use mc_data::SoundEvent;
use mc_protocol::packets::play::ClientboundMerchantOffersPacket;
use mc_text::Component;

pub trait Merchant {
    /// `setTradingPlayer(player)`.
    fn set_trading_player(&mut self, player: Option<EntityId>);
    /// `getTradingPlayer()`.
    fn get_trading_player(&self) -> Option<EntityId>;
    /// `getOffers()` (a villager generates its offers on first access, hence the level).
    fn get_offers(&mut self, level: &mut ServerLevel) -> &mut MerchantOffers;
    /// `overrideOffers(offers)`.
    fn override_offers(&mut self, offers: MerchantOffers);
    /// `notifyTrade(offer)`: `offer_index` indexes `get_offers()` (`offer.increaseUses()`, xp orb, reputation, ...).
    fn notify_trade(&mut self, level: &mut ServerLevel, offer_index: usize);
    /// `notifyTradeUpdated(itemStack)`.
    fn notify_trade_updated(&mut self, level: &mut ServerLevel, stack: &ItemStack);
    /// `getVillagerXp()`.
    fn get_villager_xp(&self) -> i32;
    /// `overrideXp(xp)`.
    fn override_xp(&mut self, xp: i32);
    /// `showProgressBar()`.
    fn show_progress_bar(&self) -> bool;
    /// `getNotifyTradeSound()`.
    fn get_notify_trade_sound(&self) -> SoundEvent;
    /// `canRestock()`.
    fn can_restock(&self) -> bool {
        false
    }
    /// `stillValid(player)` (`getTradingPlayer() == player`).
    fn still_valid(&self, player: EntityId) -> bool {
        self.get_trading_player() == Some(player)
    }
}

/// Runs `f` on the merchant entity `id` (taken out of the level meanwhile); `None` if it is gone or not a merchant.
pub fn with_merchant<R>(
    player: &mut dyn MenuPlayer,
    id: EntityId,
    f: impl FnOnce(&mut ServerLevel, &mut dyn Merchant) -> R,
) -> Option<R> {
    let level = player.level()?;
    level
        .with_entity(id, |level, e| e.as_merchant_mut().map(|m| f(level, m)))
        .flatten()
}

/// `Merchant.notifyTrade(offer)` as `AbstractVillager.notifyTrade` runs it: the merchant's own part, then
/// `CriteriaTriggers.TRADE` for the trading player when that is a `ServerPlayer`. The trigger needs the player entity and the
/// merchant at once, so it runs here, where the player is at hand, and not inside [`Merchant::notify_trade`].
pub fn notify_trade(player: &mut dyn MenuPlayer, id: EntityId, offer_index: usize) {
    let mut ran = false;
    player.with_server_player(&mut |level, p| {
        ran = true;
        level.with_entity(id, |level, e| {
            let result = {
                let m = e.as_merchant_mut()?;
                m.notify_trade(level, offer_index);
                m.get_offers(level).get(offer_index)?.get_result().clone()
            };
            crate::advancements::triggers::CriteriaTriggers::TRADE.trigger(level, p, &*e, &result);
            Some(())
        });
    });
    if !ran {
        with_merchant(player, id, |level, m| m.notify_trade(level, offer_index));
    }
}

/// `Merchant.openTradingScreen(player, title, level)`.
pub fn open_trading_screen(
    player: &mut ServerPlayer,
    level: &mut ServerLevel,
    merchant: EntityId,
    title: Component,
    merchant_level: i32,
) {
    let provider = SimpleMenuProvider::new(title, move |id, _player| {
        Some(Box::new(MerchantMenu::new(id, merchant))
            as Box<dyn crate::world::inventory::AbstractContainerMenu>)
    });
    let Some(container_id) = player.open_menu(level, &provider) else {
        return;
    };
    let data = level.with_entity(merchant, |level, e| {
        let m = e.as_merchant_mut()?;
        let offers = m.get_offers(level);
        if offers.is_empty() {
            return None;
        }
        Some((
            offers.clone(),
            m.get_villager_xp(),
            m.show_progress_bar(),
            m.can_restock(),
        ))
    });
    if let Some(Some((offers, xp, show_progress, can_restock))) = data {
        player.send_merchant_offers(
            container_id,
            &offers,
            merchant_level,
            xp,
            show_progress,
            can_restock,
        );
    }
}

impl ServerPlayer {
    /// `ServerPlayer.sendMerchantOffers(containerId, offers, merchantLevel, merchantXp, showProgressBar, canRestock)`.
    pub fn send_merchant_offers(
        &self,
        container_id: i32,
        offers: &MerchantOffers,
        merchant_level: i32,
        merchant_xp: i32,
        show_progress: bool,
        can_restock: bool,
    ) {
        self.send(&ClientboundMerchantOffersPacket {
            container_id,
            offers: offers.to_wire(),
            villager_level: merchant_level,
            villager_xp: merchant_xp,
            show_progress,
            can_restock,
        });
    }
}
