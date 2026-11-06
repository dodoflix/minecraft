//! The recipe book packet handlers of `ServerGamePacketListenerImpl` (`handlePlaceRecipe`, `handleRecipeBookSeenRecipePacket`,
//! `handleRecipeBookChangeSettingsPacket`), registered with the play dispatcher.

use linkme::distributed_slice;
use mc_protocol::packets::play::inventory::{
    ClientboundPlaceGhostRecipePacket, ServerboundPlaceRecipePacket,
    ServerboundRecipeBookChangeSettingsPacket, ServerboundRecipeBookSeenRecipePacket,
};

use super::place_recipe::PostPlaceAction;
use crate::server::network::game_packets::PlayCtx;
use crate::server::network::game_packets::hooks::{PLAY_HANDLERS, PlayHandlers};
use crate::world::entity::Entity;
use crate::world::entity::player::menu_ctx::PlayerCtx;
use crate::world::inventory::PlayerMenus;

struct RecipeBookHandlers;

impl PlayHandlers for RecipeBookHandlers {
    fn handle_place_recipe(&self, ctx: &mut PlayCtx, packet: &ServerboundPlaceRecipePacket) {
        let mut pctx = PlayerCtx {
            player: &mut *ctx.player,
            level: &mut *ctx.level,
        };
        if pctx.player.is_spectator() {
            return;
        }
        let manager = pctx.level.recipe_access();
        let container_id = packet.container_id;
        let (use_max_items, creative) = (packet.use_max_items, pctx.player.is_creative());
        let Some(info) = manager.get_recipe_from_display(packet.recipe) else {
            return;
        };
        let recipe = info.parent.clone();
        if !pctx.player.recipe_book.contains(&recipe.id) {
            return;
        }
        let result = PlayerMenus::with_current(
            &mut pctx,
            |c| &mut c.player.player.menus,
            |menu, p| {
                if menu.base().container_id != container_id {
                    return None;
                }
                if !menu.still_valid(p) {
                    log::debug!("Player interacted with invalid menu");
                    return None;
                }
                let menu = menu.as_recipe_book_menu()?;
                if recipe.value.placement_info().is_impossible_to_place() {
                    log::debug!("Player tried to place impossible recipe {}", recipe.id);
                    return None;
                }
                Some((
                    menu.handle_placement(p, use_max_items, creative, &recipe),
                    container_id,
                ))
            },
        );
        if let Some((PostPlaceAction::PlaceGhostRecipe, container_id)) = result {
            ctx.send(&ClientboundPlaceGhostRecipePacket {
                container_id,
                recipe_display: info.display.display.clone(),
            });
        }
    }

    fn handle_recipe_book_seen_recipe(
        &self,
        ctx: &mut PlayCtx,
        packet: &ServerboundRecipeBookSeenRecipePacket,
    ) {
        ctx.player.handle_recipe_book_seen_recipe(ctx.level, packet);
    }

    fn handle_recipe_book_change_settings(
        &self,
        ctx: &mut PlayCtx,
        packet: &ServerboundRecipeBookChangeSettingsPacket,
    ) {
        ctx.player.handle_recipe_book_change_settings(packet);
    }
}

#[distributed_slice(PLAY_HANDLERS)]
static RECIPE_BOOK_HANDLERS: &dyn PlayHandlers = &RecipeBookHandlers;
