//! Seams to sibling slices' state (levels, entities, player list): every place where a command needs something owned
//! by another slice goes through here, so the adaptation to their final APIs is confined to this file.
//! Each function documents the vanilla call it stands for.

use crate::server::MinecraftServer;
use crate::world::level::DimensionKey;
use mc_core::pos::BlockPos;

/// `level.hasChunkAt(pos)` (SEAM: ServerLevel).
pub fn level_has_chunk_at(server: &mut MinecraftServer, level: &DimensionKey, pos: BlockPos) -> bool {
    let _ = (server, level, pos);
    todo!("SEAM: ServerLevel::has_chunk_at")
}

/// `level.isInWorldBounds(pos)` (SEAM: ServerLevel).
pub fn level_is_in_world_bounds(server: &mut MinecraftServer, level: &DimensionKey, pos: BlockPos) -> bool {
    let _ = (server, level, pos);
    todo!("SEAM: ServerLevel::is_in_world_bounds")
}

/// `(MAX_COMMAND_SEQUENCE_LENGTH, MAX_COMMAND_FORKS)` of the server's game rules.
/// SEAM(GameServerState/GameLifecycle): `MinecraftServer` does not own `GameRules` yet; vanilla's defaults (65536 each)
/// apply until it does.
pub fn command_limits(server: &mut MinecraftServer) -> (i32, i32) {
    let _ = server;
    (65536, 65536)
}

/// `level.enabledFeatures()` (SEAM: ServerLevel).
pub fn level_enabled_features(server: &mut MinecraftServer, level: &DimensionKey) -> mc_world::level_storage::FeatureFlagSet {
    let _ = (server, level);
    todo!("SEAM: ServerLevel::enabled_features")
}

/// The tail of `CommandSourceStack.broadcastToAdmins`: when `sendCommandFeedback` is on, every op player whose command
/// source is not `source_kind` gets `message`; unless the source is the server (`source_kind`), `logAdminCommands`
/// logs it to the console.
/// SEAM(GamePlayerList, GameServerState): needs the player list, `isOp`, and the game rules.
pub fn broadcast_to_admins(
    server: &mut MinecraftServer,
    source_kind: super::CommandSourceKind,
    level: &DimensionKey,
    message: &mc_text::Component,
) {
    let _ = (server, source_kind, level, message);
    todo!("SEAM: PlayerList + GameRules")
}

// ---- entity facts the selectors and `CommandSourceStack` read off live entities ----

use crate::world::entity::Entity;
use mc_text::Component;

/// `Entity.getName()`: the custom name, else the type name; players: their profile name.
pub fn entity_name(entity: &dyn Entity) -> Component {
    if let Some(player) = entity.as_any().downcast_ref::<crate::world::entity::ServerPlayer>() {
        return Component::literal(player.player.game_profile.name.clone());
    }
    match entity.base().get_custom_name() {
        Some(name) => name,
        None => Component::translatable(entity.get_type().data().description_id, Vec::new()),
    }
}

/// `Entity.getPlainTextName()`.
pub fn entity_plain_text_name(entity: &dyn Entity) -> String {
    entity_name(entity).get_string()
}

/// `Entity.getDisplayName()`: the name formatted for its team, with the hover/insertion vanilla adds.
/// SEAM(GameServerState scoreboard): team formatting (`PlayerTeam.formatNameForTeam`) needs the scoreboard; without a
/// team the name is used as is.
pub fn entity_display_name(server: &MinecraftServer, entity: &dyn Entity) -> Component {
    let _ = server;
    let name = entity_name(entity);
    let hover = mc_text::events::HoverEvent::ShowEntity(mc_text::events::EntityTooltipInfo {
        entity_type: entity.get_type().identifier(),
        uuid: entity.uuid().as_u128(),
        name: Some(Box::new(name.clone())),
    });
    name.with_style_fn(|s| s.with_hover_event(Some(hover.clone())).with_insertion(Some(entity.uuid().to_string())))
}

/// `Entity.getTeam().getName()`.
/// SEAM(GameServerState scoreboard).
pub fn entity_team_name(server: &MinecraftServer, entity: &dyn Entity) -> Option<String> {
    let _ = (server, entity);
    None
}

/// `ServerPlayer.gameMode()`.
pub fn player_game_mode(entity: &dyn Entity) -> Option<mc_world::level_storage::GameType> {
    entity
        .as_any()
        .downcast_ref::<crate::world::entity::ServerPlayer>()
        .map(|p| mc_world::level_storage::GameType::by_id(p.game_mode.game_mode_for_player.ordinal() as i32))
}

/// `ServerPlayer.experienceLevel`.
pub fn player_experience_level(entity: &dyn Entity) -> Option<i32> {
    entity.player().filter(|_| entity.is_player()).map(|p| p.experience_level)
}

// ---- entity lookup across levels ----

use crate::commands::arguments::selector::EntityRef;
use crate::world::entity::EntityId;
use mc_core::phys::AABB;

/// `level.getEntities(...)` candidates: every entity of the level (lookup order), or those intersecting `aabb` (section
/// order).
pub fn level_entity_ids(server: &MinecraftServer, dimension: &DimensionKey, aabb: Option<&AABB>) -> Vec<EntityId> {
    let Some(level) = server.level(dimension) else { return Vec::new() };
    match aabb {
        Some(bb) => level.get_entities(None, bb, |_| true),
        None => level.entity_manager.get_all().map(|e| e.id()).collect(),
    }
}

/// The live entity `entity` refers to.
pub fn get_entity<'a>(server: &'a MinecraftServer, entity: &EntityRef) -> Option<&'a dyn Entity> {
    server.level(&entity.dimension)?.get_entity(entity.id)
}

/// `PlayerList.getPlayers()` as entity references, in join order.
/// SEAM(GameLifecycle): `MinecraftServer.player_list`; until the field exists the players of every level are listed
/// (dimension order, then level order).
pub fn server_players(server: &MinecraftServer) -> Vec<EntityRef> {
    let mut out = Vec::new();
    for dimension in server.level_keys() {
        if let Some(level) = server.level(&dimension) {
            out.extend(level.players().iter().map(|&id| EntityRef { dimension: dimension.clone(), id }));
        }
    }
    out
}

/// `PlayerList.getPlayerByName(name)` (case-insensitive).
pub fn player_by_name(server: &MinecraftServer, name: &str) -> Option<EntityRef> {
    server_players(server).into_iter().find(|p| {
        get_entity(server, p)
            .and_then(|e| e.as_any().downcast_ref::<crate::world::entity::ServerPlayer>().map(|sp| sp.player.game_profile.name.eq_ignore_ascii_case(name)))
            .unwrap_or(false)
    })
}

/// `PlayerList.getPlayer(uuid)`.
pub fn player_by_uuid(server: &MinecraftServer, uuid: uuid::Uuid) -> Option<EntityRef> {
    server_players(server).into_iter().find(|p| get_entity(server, p).is_some_and(|e| e.uuid() == uuid))
}

/// `entityType.isEnabled(features)`.
pub fn entity_type_enabled(entity_type: mc_data::EntityType, features: mc_world::level_storage::FeatureFlagSet) -> bool {
    let required: Vec<mc_core::ident::Identifier> =
        entity_type.data().required_features.iter().filter_map(|n| mc_core::ident::Identifier::try_parse(n)).collect();
    mc_world::level_storage::FeatureFlagSet::from_names(required.iter(), |_| {}).is_subset_of(features)
}

/// `level.enabledFeatures()` through a shared server borrow.
pub fn level_enabled_features_ref(server: &MinecraftServer, level: &DimensionKey) -> mc_world::level_storage::FeatureFlagSet {
    use crate::world::level::LevelReader;
    server.level(level).map_or(mc_world::level_storage::FeatureFlagSet::VANILLA, |l| l.enabled_features())
}

// ---- selector option predicates that need other slices' state ----

/// The `scores` option: every `(objective name, bounds)` matches the entity's score.
/// SEAM(GameServerState scoreboard): `Scoreboard.getObjective`/`getPlayerScoreInfo`.
pub fn entity_scores_match(server: &MinecraftServer, entity: &dyn Entity, expected: &[(String, crate::commands::arguments::min_max_bounds::Ints)]) -> bool {
    let _ = (server, entity, expected);
    todo!("SEAM: scoreboard")
}

/// The `advancements` option: for a player, every `(advancement, criteria)` expectation holds
/// (`criteria: None` = the advancement `done == value`).
/// SEAM(advancements slice): `PlayerAdvancements.getOrStartProgress`.
pub fn advancements_match(server: &MinecraftServer, entity: &dyn Entity, expected: &[(mc_core::ident::Identifier, AdvancementExpectation)]) -> bool {
    let _ = (server, entity, expected);
    todo!("SEAM: advancements")
}

/// What an `advancements={...}` entry expects.
#[derive(Clone, Debug)]
pub enum AdvancementExpectation {
    Done(bool),
    Criteria(Vec<(String, bool)>),
}

/// The `predicate` option: evaluates the loot predicate `id` with `THIS_ENTITY = entity`.
/// SEAM(loot slice): `LootItemCondition` registry + `LootContext`.
pub fn loot_predicate_matches(server: &MinecraftServer, dimension: &DimensionKey, entity: &dyn Entity, id: &mc_core::ident::Identifier) -> bool {
    let _ = (server, dimension, entity, id);
    todo!("SEAM: loot predicates")
}

/// The `SelectedItem` the `nbt` option adds for players (`ItemStack.CODEC` of the selected hotbar item).
/// SEAM(items slice).
pub fn player_selected_item_nbt(entity: &dyn Entity) -> Option<mc_nbt::Tag> {
    let _ = entity;
    None
}

// ---- names of players (suggestions, profiles) ----

/// `PlayerList.getPlayerNamesArray()` / `MinecraftServer.getPlayerNames()`.
pub fn online_player_names(server: &MinecraftServer) -> Vec<String> {
    server_players(server)
        .iter()
        .filter_map(|p| get_entity(server, p))
        .filter_map(|e| e.as_any().downcast_ref::<crate::world::entity::ServerPlayer>().map(|sp| sp.player.game_profile.name.clone()))
        .collect()
}

/// `nameToIdCache().get(name)`.
/// SEAM(GameLifecycle): the server's `UserNameToIdResolver`.
pub fn name_to_id(server: &MinecraftServer, name: &str) -> Option<crate::server::players::NameAndId> {
    let _ = (server, name);
    todo!("SEAM: MinecraftServer name->id cache")
}

/// `ServerPlayer.nameAndId()`.
pub fn player_name_and_id(entity: &dyn Entity) -> Option<crate::server::players::NameAndId> {
    entity
        .as_any()
        .downcast_ref::<crate::world::entity::ServerPlayer>()
        .map(|sp| crate::server::players::NameAndId::new(sp.player.game_profile.id, sp.player.game_profile.name.clone()))
}

// ---- scoreboard (SEAM: GameServerState scoreboard model) ----

/// `scoreboard.getObjective(name) != null`.
pub fn scoreboard_has_objective(server: &MinecraftServer, name: &str) -> bool {
    let _ = (server, name);
    todo!("SEAM: scoreboard")
}

/// `objective.getCriteria().isReadOnly()` of an existing objective.
pub fn scoreboard_objective_read_only(server: &MinecraftServer, name: &str) -> bool {
    let _ = (server, name);
    todo!("SEAM: scoreboard")
}

/// `scoreboard.getObjectiveNames()`.
pub fn scoreboard_objective_names(server: &MinecraftServer) -> Vec<String> {
    let _ = server;
    todo!("SEAM: scoreboard")
}

/// `scoreboard.getTeamNames()`.
pub fn scoreboard_team_names(server: &MinecraftServer) -> Vec<String> {
    let _ = server;
    todo!("SEAM: scoreboard")
}

/// `scoreboard.getPlayerTeam(name) != null`.
pub fn scoreboard_has_team(server: &MinecraftServer, name: &str) -> bool {
    let _ = (server, name);
    todo!("SEAM: scoreboard")
}

/// `scoreboard.getTrackedPlayers()` (score holder names).
pub fn scoreboard_tracked_players(server: &MinecraftServer) -> Vec<String> {
    let _ = server;
    todo!("SEAM: scoreboard")
}

// ---- registries ----

/// The registry `registry_key` (e.g. `minecraft:item`) as the command source sees it: the server's registries
/// (static, data-driven and reloadable ones).
/// SEAM(GameServerPacks/GameLifecycle): `MinecraftServer.registry_access()` + reloadable registries; until the server
/// owns them only the built-in registries of `mc_data` (with vanilla tags) are available.
pub fn registry_view(server: &MinecraftServer, registry_key: &str) -> Option<std::sync::Arc<dyn super::RegistryView>> {
    let _ = server;
    use super::command_build_context::{HolderLookupProvider, StaticRegistries};
    StaticRegistries.lookup(registry_key)
}

// ---- functions ----

/// `server.getFunctions().get(id)`.
/// SEAM(function manager, nobody owns `ServerFunctionManager` yet).
pub fn get_function(server: &MinecraftServer, id: &mc_core::ident::Identifier) -> Option<std::sync::Arc<dyn super::functions::CommandFunction>> {
    let _ = (server, id);
    todo!("SEAM: ServerFunctionManager")
}

/// `server.getFunctions().getTag(id)`.
/// SEAM(function manager).
pub fn get_function_tag(server: &MinecraftServer, id: &mc_core::ident::Identifier) -> Option<Vec<std::sync::Arc<dyn super::functions::CommandFunction>>> {
    let _ = (server, id);
    todo!("SEAM: ServerFunctionManager")
}
