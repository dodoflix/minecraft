//! The recipe book packet handlers of `ServerGamePacketListenerImpl` (`handlePlaceRecipe`,
//! `handleRecipeBookSeenRecipePacket`, `handleRecipeBookChangeSettingsPacket`), registered with the play dispatcher.

use linkme::distributed_slice;
use mc_protocol::packets::play::ClientboundPlaceGhostRecipePacket;
use mc_protocol::packets::play::inventory::{
    ServerboundPlaceRecipePacket, ServerboundRecipeBookChangeSettingsPacket,
    ServerboundRecipeBookSeenRecipePacket,
};

use super::PostPlaceAction;
use crate::server::network::game_packets::PlayCtx;
use crate::server::network::game_packets::hooks::{PLAY_HANDLERS, PlayHandlers};
use crate::world::entity::Entity;
use crate::world::inventory::MenuPlayer;
use crate::world::item::player_systems::with_current_menu;

struct RecipeBookHandlers;

impl PlayHandlers for RecipeBookHandlers {
    fn handle_place_recipe(&self, ctx: &mut PlayCtx, packet: &ServerboundPlaceRecipePacket) {
        if ctx.player.is_spectator()
            || ctx.player.player.menus.current_container_id() != packet.container_id
        {
            return;
        }
        let recipes = ctx.level.recipe_access();
        let ghost = with_current_menu(ctx, |menu, p| {
            if !menu.still_valid(&*p) {
                log::debug!(
                    "Player {} interacted with invalid menu",
                    p.player.player.game_profile.name
                );
                return None;
            }
            let info = recipes.get_recipe_from_display(packet.recipe)?;
            let recipe = &info.parent;
            if !p.recipe_book_contains(&recipe.id) {
                return None;
            }
            let container_id = menu.base().container_id;
            let book_menu = menu.as_recipe_book_menu_mut()?;
            if recipe.value.placement_info().is_impossible_to_place() {
                log::debug!(
                    "Player {} tried to place impossible recipe {}",
                    p.player.player.game_profile.name,
                    recipe.id
                );
                return None;
            }
            let creative = p.is_creative();
            let action = book_menu.handle_placement(p, packet.use_max_items, creative, recipe);
            (action == PostPlaceAction::PlaceGhostRecipe).then(|| {
                ClientboundPlaceGhostRecipePacket {
                    container_id,
                    recipe_display: info.display.display.clone(),
                }
            })
        });
        if let Some(ghost) = ghost {
            ctx.player.send(&ghost);
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
