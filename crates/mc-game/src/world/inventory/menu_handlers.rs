//! Serverbound packets of the menus owned by this module (`handleSetBeaconPacket`, `handleSelectTrade`).

use super::{BeaconMenu, PlayerMenus};
use crate::server::network::game_packets::PlayCtx;
use crate::server::network::game_packets::hooks::{PLAY_HANDLERS, PlayHandlers};
use crate::translatable;
use crate::world::entity::player::menu_ctx::PlayerCtx;
use linkme::distributed_slice;
use mc_protocol::packets::play::ServerboundSetBeaconPacket;

struct MenuCoreHandlers;

impl PlayHandlers for MenuCoreHandlers {
    fn handle_set_beacon(&self, ctx: &mut PlayCtx, packet: &ServerboundSetBeaconPacket) {
        let mut pctx = PlayerCtx { player: &mut *ctx.player, level: &mut *ctx.level };
        let accepted = PlayerMenus::with_current(&mut pctx, |c| &mut c.player.player.menus, |menu, p| {
            if !menu.as_any().is::<BeaconMenu>() {
                return true;
            }
            if !menu.still_valid(&*p) {
                log::debug!("Player {} interacted with invalid menu", p.player.player.game_profile.name);
                return true;
            }
            let beacon = menu.as_any_mut().downcast_mut::<BeaconMenu>().expect("checked above");
            beacon.update_effects(p, packet.primary, packet.secondary)
        });
        if !accepted {
            log::warn!("Player {} tried to set invalid beacon effects", ctx.player.player.game_profile.name);
            ctx.lis().disconnect(translatable!("multiplayer.disconnect.generic"));
        }
    }
}

#[distributed_slice(PLAY_HANDLERS)]
static MENU_CORE_HANDLERS: &dyn PlayHandlers = &MenuCoreHandlers;
