//! Text resolution for `set_name` / `set_lore` (`ComponentUtils.resolve` with the entity as command source) and
//! the player game profile of `fill_player_head`.

use mc_protocol::components::values::profile::ResolvableProfile;
use mc_text::Component;

use crate::commands::component_utils::{ResolutionContext, resolve};
use crate::commands::server_access;
use crate::server::level::ServerLevel;
use crate::server::permissions::{LevelBasedPermissionSet, PermissionSet};
use crate::world::entity::Entity;

/// `ComponentUtils.resolve(ResolutionContext.create(entity.createCommandSourceStackForNameResolution(level).withPermission(GAMEMASTER)), line)`.
///
/// The server is reached through [`server_access::share_server`] (command entry points that roll loot share it) or the installed
/// one; code that rolls loot without either, or while the server does not hold the level (a tick, a packet handler), keeps the
/// line as written (selector, score and NBT contents stay unresolved there; text without them resolves to itself).
pub fn resolve_component(
    level: &ServerLevel,
    entity: &dyn Entity,
    line: &Component,
) -> Result<Component, String> {
    let source = server_access::with_server_ref_opt(|server| {
        // a server that does not hold this very level (it is taken out for a packet) cannot find the entity either
        if !server
            .level(&level.dimension)
            .is_some_and(|l| std::ptr::eq(l, level))
        {
            return None;
        }
        Some(
            entity
                .create_command_source_stack_for_name_resolution(server, &level.dimension)
                .with_permission(PermissionSet::LevelBased(
                    LevelBasedPermissionSet::GAMEMASTER,
                )),
        )
    })
    .flatten();
    let Some(source) = source else {
        return Ok(line.clone());
    };
    resolve(&ResolutionContext::create(&source), line)
        .map_err(|e| e.get_message().into_string_lossy())
}

/// `ResolvableProfile.createResolved(player.getGameProfile())`.
pub fn player_profile(player: &dyn Entity) -> Option<ResolvableProfile> {
    let data = player.player()?;
    Some(ResolvableProfile {
        contents: mc_protocol::codec::Either::Left(data.game_profile.clone()),
        skin_patch: Default::default(),
    })
}
