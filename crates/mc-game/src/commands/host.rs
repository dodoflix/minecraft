//! Seams to sibling slices' state (levels, entities, player list): every place where a command needs something owned
//! by another slice goes through here, so the adaptation to their final APIs is confined to this file.
//! Each function documents the vanilla call it stands for.

use crate::server::MinecraftServer;
use crate::world::level::DimensionKey;
use mc_core::pos::BlockPos;

/// `level.hasChunkAt(pos)`: the chunk of `pos` is loaded (no height check, like vanilla).
pub fn level_has_chunk_at(server: &mut MinecraftServer, level: &DimensionKey, pos: BlockPos) -> bool {
    use crate::world::level::LevelReader;
    server.level(level).is_some_and(|l| l.has_chunk(pos.x() >> 4, pos.z() >> 4))
}

/// `level.isInWorldBounds(pos)`.
pub fn level_is_in_world_bounds(server: &mut MinecraftServer, level: &DimensionKey, pos: BlockPos) -> bool {
    server.level(level).is_some_and(|l| l.is_in_world_bounds(pos))
}

/// `(MAX_COMMAND_SEQUENCE_LENGTH, MAX_COMMAND_FORKS)` of the server's game rules.
pub fn command_limits(server: &mut MinecraftServer) -> (i32, i32) {
    use crate::world::level::gamerules::GameRules;
    let rules = server.game_rules_ref();
    (rules.get(GameRules::MAX_COMMAND_SEQUENCE_LENGTH), rules.get(GameRules::MAX_COMMAND_FORKS))
}

/// `level.enabledFeatures()`.
pub fn level_enabled_features(server: &mut MinecraftServer, level: &DimensionKey) -> mc_world::level_storage::FeatureFlagSet {
    level_enabled_features_ref(server, level)
}

/// The tail of `CommandSourceStack.broadcastToAdmins`: when `sendCommandFeedback` is on, every op player whose command
/// source is not `source_kind` gets `message`; unless the source is the server (`source_kind`), `logAdminCommands`
/// logs it to the console.
pub fn broadcast_to_admins(
    server: &mut MinecraftServer,
    source_kind: super::CommandSourceKind,
    _level: &DimensionKey,
    message: &Component,
) {
    use crate::world::level::gamerules::GameRules;
    if server.game_rules_ref().get(GameRules::SEND_COMMAND_FEEDBACK) {
        for player in server.player_list.get_players() {
            let is_source = source_kind == super::CommandSourceKind::Player(player.uuid);
            if !is_source && server.player_list.is_op(&player.name_and_id()) {
                player.send_system_message(message, false);
            }
        }
    }
    if source_kind != super::CommandSourceKind::Server && server.game_rules_ref().get(GameRules::LOG_ADMIN_COMMANDS) {
        log::info!("System chat: {}", message.get_string());
    }
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
/// The server has no scoreboard instance yet (`MinecraftServer.getScoreboard()`), i.e. no objectives exist: like
/// vanilla for an unknown objective, nothing matches.
pub fn entity_scores_match(server: &MinecraftServer, entity: &dyn Entity, expected: &[(String, crate::commands::arguments::min_max_bounds::Ints)]) -> bool {
    let _ = (server, entity);
    expected.is_empty()
}

/// The `advancements` option: for a player, every `(advancement, criteria)` expectation holds
/// (`criteria: None` = the advancement `done == value`).
/// No advancement data is loaded yet (`ServerAdvancementManager` is empty): a known advancement cannot be found, and
/// vanilla's `getOrStartProgress` is skipped for unknown ones (`advancement == null` -> no match).
pub fn advancements_match(server: &MinecraftServer, entity: &dyn Entity, expected: &[(mc_core::ident::Identifier, AdvancementExpectation)]) -> bool {
    let _ = server;
    entity.as_any().downcast_ref::<crate::world::entity::ServerPlayer>().is_some() && expected.is_empty()
}

/// What an `advancements={...}` entry expects.
#[derive(Clone, Debug)]
pub enum AdvancementExpectation {
    Done(bool),
    Criteria(Vec<(String, bool)>),
}

/// The `predicate` option: evaluates the loot predicate `id` with `THIS_ENTITY = entity`.
/// No loot predicates are loaded yet (`ReloadableServerRegistries.lookup` has none), so `lookup(id) == null`: false.
pub fn loot_predicate_matches(server: &MinecraftServer, dimension: &DimensionKey, entity: &dyn Entity, id: &mc_core::ident::Identifier) -> bool {
    let _ = (server, dimension, entity, id);
    false
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
pub fn name_to_id(server: &MinecraftServer, name: &str) -> Option<crate::server::players::NameAndId> {
    use crate::server::players::name_resolver::UserNameToIdResolver;
    server.name_to_id_cache.get_by_name(name)
}

/// `ServerPlayer.nameAndId()`.
pub fn player_name_and_id(entity: &dyn Entity) -> Option<crate::server::players::NameAndId> {
    entity
        .as_any()
        .downcast_ref::<crate::world::entity::ServerPlayer>()
        .map(|sp| crate::server::players::NameAndId::new(sp.player.game_profile.id, sp.player.game_profile.name.clone()))
}

// ---- scoreboard ----
// `MinecraftServer` does not own a `ServerScoreboard` yet (the model is `world::scores::Scoreboard`): until it does the
// scoreboard is the empty one (no objectives, teams or tracked players).

/// `scoreboard.getObjective(name) != null`.
pub fn scoreboard_has_objective(server: &MinecraftServer, name: &str) -> bool {
    let _ = server;
    let _ = name;
    false
}

/// `objective.getCriteria().isReadOnly()` of an existing objective.
pub fn scoreboard_objective_read_only(server: &MinecraftServer, name: &str) -> bool {
    let _ = server;
    let _ = name;
    false
}

/// `scoreboard.getObjectiveNames()`.
pub fn scoreboard_objective_names(server: &MinecraftServer) -> Vec<String> {
    let _ = server;
    Vec::new()
}

/// `scoreboard.getTeamNames()`.
pub fn scoreboard_team_names(server: &MinecraftServer) -> Vec<String> {
    let _ = server;
    Vec::new()
}

/// `scoreboard.getPlayerTeam(name) != null`.
pub fn scoreboard_has_team(server: &MinecraftServer, name: &str) -> bool {
    let _ = server;
    let _ = name;
    false
}

/// `scoreboard.getTrackedPlayers()` (score holder names).
pub fn scoreboard_tracked_players(server: &MinecraftServer) -> Vec<String> {
    let _ = server;
    Vec::new()
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
/// Nobody owns `ServerFunctionManager` yet: no data pack function is loaded, so every lookup is `Optional.empty()`
/// (`arguments.function.unknown`).
pub fn get_function(server: &MinecraftServer, id: &mc_core::ident::Identifier) -> Option<std::sync::Arc<dyn super::functions::CommandFunction>> {
    let _ = (server, id);
    None
}

/// `server.getFunctions().getTag(id)`.
/// See [`get_function`]: every function tag is unknown (`arguments.function.tag.unknown`).
pub fn get_function_tag(server: &MinecraftServer, id: &mc_core::ident::Identifier) -> Option<Vec<std::sync::Arc<dyn super::functions::CommandFunction>>> {
    let _ = (server, id);
    None
}

// ---- players, chat, permissions ----

use crate::server::chat::chat_type::{Bound, ChatTypeLookup};
use crate::server::players::{NameAndId, PlayerHandle};
use mc_core::ident::Identifier;
use std::sync::Arc;

/// `ChatType.bind(key, registryAccess, name)` over the server's `chat_type` registry. The vanilla chat types are
/// always present, so a missing one is a server bug and panics like vanilla's `getOrThrow`.
pub fn bind_chat_type(server: &MinecraftServer, chat_type: &str, name: Component) -> Bound {
    use crate::registry::typed::chat_type::{ChatParameter, ChatType, ChatTypeDecoration};
    use mc_protocol::packets::play as wire;
    struct Lookup<'a>(&'a crate::registry::access::RegistryAccess);
    fn decoration(d: &ChatTypeDecoration) -> wire::ChatTypeDecoration {
        wire::ChatTypeDecoration {
            translation_key: d.translation_key.clone(),
            parameters: d
                .parameters
                .iter()
                .map(|p| match p {
                    ChatParameter::Sender => wire::ChatTypeDecorationParameter::Sender,
                    ChatParameter::Target => wire::ChatTypeDecorationParameter::Target,
                    ChatParameter::Content => wire::ChatTypeDecorationParameter::Content,
                })
                .collect(),
            style: d.style.clone(),
        }
    }
    impl ChatTypeLookup for Lookup<'_> {
        fn chat_type(&self, key: &Identifier) -> Option<(i32, Arc<wire::ChatType>)> {
            let registry = self.0.registry(crate::registry::keys::CHAT_TYPE)?;
            let id = registry.id_of(key)?;
            let value = registry.get_by_id::<ChatType>(id)?;
            Some((id as i32, Arc::new(wire::ChatType { chat: decoration(&value.chat), narration: decoration(&value.narration) })))
        }
    }
    Bound::bind(&Lookup(server.registries.composite_access()), chat_type, name).unwrap_or_else(|| panic!("Missing chat type {chat_type}"))
}

/// `PlayerList.broadcastChatMessage(message, source, bound)` (the sender is `source.getPlayer()`).
pub fn broadcast_chat_message(
    server: &MinecraftServer,
    message: &crate::server::chat::PlayerChatMessage,
    sender: Option<&PlayerHandle>,
    bound: &Bound,
) {
    server.player_list.broadcast_chat_message(message, |receiver| sender.is_some_and(|s| s.should_filter_message_to(receiver)), sender, bound);
}

/// The connection handle of the player entity `player`.
pub fn player_handle(server: &MinecraftServer, player: &EntityRef) -> Option<PlayerHandle> {
    server.player_list.get_players().into_iter().find(|p| p.id == player.id)
}

/// `ServerPlayer.getDisplayName()`.
pub fn player_display_name(server: &MinecraftServer, player: &PlayerHandle) -> Component {
    let dimension = player.dimension();
    match server.level(&dimension).and_then(|l| l.get_entity(player.id)) {
        Some(entity) => entity_display_name(server, entity),
        None => Component::literal(player.name().to_owned()),
    }
}

/// `PlayerList.op(nameAndId)`.
pub fn op(server: &mut MinecraftServer, name_and_id: NameAndId) {
    crate::server::players::player_ops::op(server, name_and_id, None, None);
}

/// `PlayerList.deop(nameAndId)`.
pub fn deop(server: &mut MinecraftServer, name_and_id: NameAndId) {
    crate::server::players::player_ops::deop(server, &name_and_id);
}

/// `MinecraftServer.kickUnlistedPlayers()`.
pub fn kick_unlisted_players(server: &mut MinecraftServer) {
    if server.is_enforce_whitelist() && server.is_using_whitelist() {
        for player in server.player_list.get_players() {
            if !server.player_list.whitelist.is_white_listed(&player.name_and_id()) {
                player.disconnect(&Component::translatable("multiplayer.disconnect.not_whitelisted", vec![]));
            }
        }
    }
}

// ---- levels ----

/// `UniformInt.of(min, max).sample(level.getRandom())` on the source's level.
pub fn sample_uniform_int(source: &super::CommandSourceStack, min: i32, max: i32) -> i32 {
    use mc_core::random::RandomSource;
    source.with_server(|server| {
        server.level_mut(source.get_level()).map_or(min, |level| level.tick_state.random.next_int_between_inclusive(min, max))
    })
}

/// `NbtPredicate.getEntityTagWithSelectedItem(entity)` (entity `DataSource`).
pub fn entity_data_with_selected_item(server: &MinecraftServer, entity: &EntityRef) -> Option<mc_nbt::CompoundTag> {
    let e = get_entity(server, entity)?;
    let mut output = mc_nbt::CompoundTag::new();
    let level = server.level(&entity.dimension);
    let lookup = |id| level.and_then(|l| l.get_entity(id));
    crate::world::entity::entity::save_without_id(e, &mut output, &lookup);
    if let Some(selected) = player_selected_item_nbt(e) {
        output.put("SelectedItem", selected);
    }
    Some(output)
}

/// `BlockEntity.saveWithFullMetadata` of the block entity at `pos` (block `DataSource`).
pub fn block_entity_data(server: &MinecraftServer, dimension: &DimensionKey, pos: BlockPos) -> Option<mc_nbt::CompoundTag> {
    use crate::world::level::BlockGetter;
    let block_entity = server.level(dimension)?.get_block_entity(pos)?;
    let mut tag = mc_nbt::CompoundTag::new();
    block_entity.save_additional(&mut tag);
    tag.put_string("id", block_entity.get_type().name());
    tag.put_int("x", pos.x());
    tag.put_int("y", pos.y());
    tag.put_int("z", pos.z());
    Some(tag)
}

/// `CommandStorage.get(id)` (storage `DataSource`).
pub fn command_storage_get(server: &mut MinecraftServer, id: &Identifier) -> mc_nbt::CompoundTag {
    crate::world::level::saveddata::command_storage::CommandStorage::new().get(&mut server.saved_data, id)
}

/// `ScoreHolder` of an entity (`getScoreboardName`).
pub fn score_holder_of(server: &MinecraftServer, entity: &EntityRef) -> Option<super::arguments::scores::ScoreHolder> {
    let e = get_entity(server, entity)?;
    Some(super::arguments::scores::ScoreHolder::for_name_only(&e.get_scoreboard_name()))
}

/// `ScoreContents.getScore`: the score of `holder` in `objective` formatted, `empty` when there is none. The server has
/// no scoreboard instance yet (see above), so no objective exists.
pub fn score_component(server: &MinecraftServer, holder: &super::arguments::scores::ScoreHolder, objective: &str) -> Component {
    let _ = (server, holder, objective);
    Component::empty()
}

// ---- clocks and timelines ----

/// `DimensionType.defaultClock()` of `dimension`.
pub fn default_clock(server: &MinecraftServer, dimension: &DimensionKey) -> Option<Identifier> {
    server.default_clock_of(dimension)
}

/// `(clock, periodTicks)` of the timeline `id`.
pub fn timeline(server: &MinecraftServer, id: &Identifier) -> Option<(Identifier, Option<i32>)> {
    let registry = server.registries.composite_access().registry(crate::registry::keys::TIMELINE)?;
    let entry = registry.entry_by_key(id)?;
    let clock = Identifier::try_parse(entry.json.get("clock")?.as_str()?)?;
    let period = entry.json.get("period_ticks").and_then(|p| p.as_i64()).map(|p| p as i32);
    Some((clock, period))
}

/// Ids of the timelines whose clock is `clock`.
pub fn timelines_of_clock(server: &MinecraftServer, clock: &Identifier) -> Vec<Identifier> {
    let Some(registry) = server.registries.composite_access().registry(crate::registry::keys::TIMELINE) else { return Vec::new() };
    registry
        .entries()
        .iter()
        .filter(|e| e.json.get("clock").and_then(|c| c.as_str()) == Some(&clock.to_string()))
        .map(|e| e.id.clone())
        .collect()
}

// ---- teleporting, rotating, game modes ----

/// `DimensionType` registered name of `dimension` (`Holder.getRegisteredName`: `[unregistered]` for inline types).
pub fn dimension_type_name(server: &MinecraftServer, dimension: &DimensionKey) -> String {
    use crate::registry::typed::level_stem::{DimensionTypeSource, LevelStem};
    match server.registries.composite_access().get::<LevelStem>(crate::registry::keys::LEVEL_STEM, dimension) {
        Some(stem) => match &stem.dimension_type {
            DimensionTypeSource::Reference(id) => id.to_string(),
            DimensionTypeSource::Inline(_) => "[unregistered]".to_owned(),
        },
        None => "[unregistered]".to_owned(),
    }
}

/// `Entity.teleportTo(level, x, y, z, relatives, yRot, xRot, true)` followed by `TeleportCommand`'s post steps
/// (no vertical momentum unless fall flying). `destination` is the level to teleport into.
///
/// Only same-dimension teleports are implemented: a cross-dimension move needs `Entity.teleportCrossDimension` /
/// `ServerPlayer.teleport(TeleportTransition)` (entity re-creation and the respawn packet sequence), which the entity
/// slice has not landed; the call then fails like vanilla's `teleport` returning `null`.
pub fn teleport_entity(
    server: &mut MinecraftServer,
    victim: &EntityRef,
    destination: &DimensionKey,
    position: mc_core::phys::Vec3,
    relatives: mc_protocol::packets::play::Relatives,
    y_rot: f32,
    x_rot: f32,
) -> bool {
    use mc_protocol::packets::play::PositionMoveRotation;
    if &victim.dimension != destination {
        return false;
    }
    let pmr = PositionMoveRotation { position, delta_movement: mc_core::phys::Vec3::ZERO, y_rot, x_rot };
    server
        .level_mut(&victim.dimension)
        .and_then(|level| {
            level.with_entity(victim.id, |_, entity| {
                if entity.is_removed() {
                    return false;
                }
                if let Some(player) = entity.as_any_mut().downcast_mut::<crate::world::entity::ServerPlayer>() {
                    player.connection_teleport_pmr(pmr, relatives);
                } else {
                    entity.teleport_set_position(pmr, relatives);
                }
                let head = if relatives.contains(mc_protocol::packets::play::Relative::YRot) { entity.get_y_head_rot() + y_rot } else { y_rot };
                entity.set_y_head_rot(head);
                true
            })
        })
        .unwrap_or(false)
}

/// `TeleportCommand.performTeleport`'s tail for a successfully teleported `victim`: keeps only horizontal momentum and
/// sets it on the ground unless it is fall flying.
pub fn after_teleport(server: &mut MinecraftServer, victim: &EntityRef) {
    if let Some(entity) = server.level_mut(&victim.dimension).and_then(|l| l.get_entity_mut(victim.id)) {
        let fall_flying = entity.living().is_some_and(|l| l.is_fall_flying());
        if !fall_flying {
            let dm = entity.base().delta_movement();
            entity.base_mut().set_delta_movement(mc_core::phys::Vec3::new(dm.x, 0.0, dm.z));
            entity.base_mut().set_on_ground_raw(true);
        }
    }
}

/// `Entity.lookAt(anchor, pos)`.
pub fn look_at(server: &mut MinecraftServer, target: &EntityRef, anchor: super::arguments::entity_anchor_argument::Anchor, pos: mc_core::phys::Vec3) {
    use mc_core::math::mth;
    let Some(entity) = server.level_mut(&target.dimension).and_then(|l| l.get_entity_mut(target.id)) else { return };
    let from = anchor.apply_entity(entity.base().position(), entity.base().eye_height);
    let (xd, yd, zd) = (pos.x - from.x, pos.y - from.y, pos.z - from.z);
    let sd = (xd * xd + zd * zd).sqrt();
    let x_rot = mth::wrap_degrees_f32((-(mth::atan2(yd, sd) * 180.0 / std::f64::consts::PI as f32 as f64)) as f32);
    let y_rot = mth::wrap_degrees_f32((mth::atan2(zd, xd) * 180.0 / std::f64::consts::PI as f32 as f64) as f32 - 90.0);
    entity.set_x_rot(x_rot);
    entity.set_y_rot(y_rot);
    entity.set_y_head_rot(y_rot);
    let base = entity.base_mut();
    base.x_rot_o = base.x_rot();
    base.y_rot_o = base.y_rot();
}

/// `ServerPlayer.setGameMode(mode)`: false when the player already has it.
///
/// Gaps (entity slice): the spectator side effects (`removeEntitiesOnShoulder`/`stopRiding`/`stopUsingItem`/location
/// effects) and `updateEffectVisibility` have no counterpart yet; `isInRangeOfGround` is approximated by `onGround`.
pub fn set_game_mode(server: &mut MinecraftServer, player: &EntityRef, game_mode: mc_world::level_storage::GameType) -> bool {
    use mc_data::enums::GameType;
    let mode = match game_mode {
        mc_world::level_storage::GameType::Survival => GameType::Survival,
        mc_world::level_storage::GameType::Creative => GameType::Creative,
        mc_world::level_storage::GameType::Adventure => GameType::Adventure,
        mc_world::level_storage::GameType::Spectator => GameType::Spectator,
    };
    use mc_protocol::packets::play::{ClientboundGameEventPacket, ClientboundPlayerInfoUpdateAction, ClientboundPlayerInfoUpdatePacket};
    let Some(handle) = player_handle(server, player) else { return false };
    let changed = server.level_mut(&player.dimension).and_then(|l| l.get_entity_mut(player.id)).and_then(|entity| {
        let sp = entity.as_any_mut().downcast_mut::<crate::world::entity::ServerPlayer>()?;
        let old = sp.game_mode.game_mode_for_player;
        if old == mode {
            return Some(false);
        }
        sp.set_game_mode_for_player(mode, Some(old));
        if sp.player.abilities.flying && mode != GameType::Spectator && sp.base().on_ground() {
            sp.player.abilities.flying = false;
        }
        sp.on_update_abilities();
        if mode == GameType::Creative {
            sp.player.living.current_impulse_impact_pos = None;
            sp.player.living.current_impulse_context_reset_grace_time = 0;
        }
        Some(true)
    });
    if changed != Some(true) {
        return false;
    }
    handle.shared.lock().game_mode = mode;
    server.player_list.broadcast_all(&ClientboundPlayerInfoUpdatePacket {
        actions: [ClientboundPlayerInfoUpdateAction::UpdateGameMode].into_iter().collect(),
        entries: vec![handle.info_entry()],
    });
    let id = match mode {
        GameType::Survival => 0,
        GameType::Creative => 1,
        GameType::Adventure => 2,
        GameType::Spectator => 3,
    };
    handle.connection.send(&ClientboundGameEventPacket { event: ClientboundGameEventPacket::CHANGE_GAME_MODE, param: id as f32 });
    if let Some(sp) = server.level_mut(&player.dimension).and_then(|l| l.get_entity_mut(player.id)).and_then(|e| e.as_any_mut().downcast_mut::<crate::world::entity::ServerPlayer>()) {
        sp.on_update_abilities();
    }
    true
}

/// Kicks `player` (`player.connection.disconnect(reason)`); the player's display name for the feedback.
pub fn kick_player(server: &mut MinecraftServer, player: &EntityRef, reason: &Component) -> Option<Component> {
    let handle = player_handle(server, player)?;
    let name = player_display_name(server, &handle);
    handle.disconnect(reason);
    Some(name)
}
