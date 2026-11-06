//! The recipe book packet handlers of `ServerGamePacketListenerImpl` (`handleRecipeBookSeenRecipePacket`,
//! `handleRecipeBookChangeSettingsPacket`), registered with the play dispatcher.

use linkme::distributed_slice;
use mc_protocol::packets::play::inventory::{
    ServerboundRecipeBookChangeSettingsPacket, ServerboundRecipeBookSeenRecipePacket,
};

use crate::server::network::game_packets::PlayCtx;
use crate::server::network::game_packets::hooks::{PLAY_HANDLERS, PlayHandlers};

struct RecipeBookHandlers;

impl PlayHandlers for RecipeBookHandlers {
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
