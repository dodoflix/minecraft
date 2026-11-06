//! Text resolution for `set_name` / `set_lore` (`ComponentUtils.resolve` with the entity as command source) and
//! the player game profile of `fill_player_head`.

use mc_protocol::components::values::profile::ResolvableProfile;
use mc_text::Component;

use crate::server::level::ServerLevel;
use crate::world::entity::Entity;

/// `ComponentUtils.resolve(ResolutionContext.create(entity.createCommandSourceStackForNameResolution(level)), line)`.
///
/// shortcut: selector / score / NBT contents stay unresolved (building the entity's command source needs the command
/// stack for entities); text without such contents resolves to itself, as in vanilla.
pub fn resolve_component(
    _level: &ServerLevel,
    _entity: &dyn Entity,
    line: &Component,
) -> Result<Component, String> {
    Ok(line.clone())
}

/// `ResolvableProfile.createResolved(player.getGameProfile())`.
pub fn player_profile(player: &dyn Entity) -> Option<ResolvableProfile> {
    let data = player.player()?;
    Some(ResolvableProfile {
        contents: mc_protocol::codec::Either::Left(data.game_profile.clone()),
        skin_patch: Default::default(),
    })
}
