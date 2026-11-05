//! Seams to other subsystems' state (levels, entities, player list): every place where a command needs something owned
//! by another subsystem goes through here, so the adaptation to their final APIs is confined to this file.
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
        None => entity.get_type_name(),
    }
}

/// `Entity.getPlainTextName()`.
pub fn entity_plain_text_name(entity: &dyn Entity) -> String {
    entity_name(entity).get_string()
}

/// `Entity.getDisplayName()`: the name formatted for its team, with the hover/insertion vanilla adds.
pub fn entity_display_name(server: &MinecraftServer, entity: &dyn Entity) -> Component {
    let name = entity_name(entity);
    let scoreboard = server.scoreboard.read();
    let team = scoreboard.get_players_team(&entity.get_scoreboard_name());
    decorate_display_name(crate::world::scores::PlayerTeam::format_name_for_team(team, &name), entity)
}

/// `Entity.decorateDisplayNameComponent` (hover = the entity, insertion = its UUID) and the override of
/// `Player` (click = suggest `/tell <name> `, insertion = the profile name).
pub fn decorate_display_name(name: Component, entity: &dyn Entity) -> Component {
    let hover = mc_text::events::HoverEvent::ShowEntity(mc_text::events::EntityTooltipInfo {
        entity_type: entity.get_type().identifier(),
        uuid: entity.uuid().as_u128(),
        name: Some(Box::new(entity_name(entity))),
    });
    if entity.is_player() {
        let profile_name = entity_name(entity).get_string();
        let click = mc_text::events::ClickEvent::SuggestCommand(format!("/tell {profile_name} "));
        return name.with_style_fn(|s| s.with_click_event(Some(click.clone())).with_hover_event(Some(hover.clone())).with_insertion(Some(profile_name.clone())));
    }
    name.with_style_fn(|s| s.with_hover_event(Some(hover.clone())).with_insertion(Some(entity.uuid().to_string())))
}

/// `Entity.getTeam().getName()`.
pub fn entity_team_name(server: &MinecraftServer, entity: &dyn Entity) -> Option<String> {
    server.scoreboard.read().get_players_team(&entity.get_scoreboard_name()).map(|t| t.get_name().to_owned())
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
/// SEAM(server lifecycle): `MinecraftServer.player_list`; until the field exists the players of every level are listed
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

// ---- selector option predicates that need other subsystems' state ----

/// The `scores` option: every `(objective name, bounds)` matches the entity's score.
pub fn entity_scores_match(server: &MinecraftServer, entity: &dyn Entity, expected: &[(String, crate::commands::arguments::min_max_bounds::Ints)]) -> bool {
    let holder = crate::world::scores::score::for_name_only(&entity.get_scoreboard_name());
    expected.iter().all(|(objective, bounds)| {
        server.scoreboard.read().get_objective(objective).is_some()
            && server.scoreboard.read().get_player_score_info(&holder, objective).is_some_and(|score| bounds.matches(score.value()))
    })
}

/// The `advancements` option: for a player, every `(advancement, criteria)` expectation holds
/// (`criteria: None` = the advancement `done == value`).
/// No advancement data is loaded yet (`ServerAdvancementManager` is empty): a known advancement cannot be found, and
/// vanilla's `getOrStartProgress` is skipped for unknown ones (`advancement == null` -> no match).
pub fn advancements_match(_server: &MinecraftServer, entity: &dyn Entity, expected: &[(mc_core::ident::Identifier, AdvancementExpectation)]) -> bool {
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
/// SEAM(items).
pub fn player_selected_item_nbt(_entity: &dyn Entity) -> Option<mc_nbt::Tag> {
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

/// `scoreboard.getObjective(name) != null`.
pub fn scoreboard_has_objective(server: &MinecraftServer, name: &str) -> bool {
    server.scoreboard.read().get_objective(name).is_some()
}

/// `objective.getCriteria().isReadOnly()` of an existing objective.
pub fn scoreboard_objective_read_only(server: &MinecraftServer, name: &str) -> bool {
    server.scoreboard.read().get_objective(name).is_some_and(|o| o.get_criteria().is_read_only())
}

/// `scoreboard.getObjectiveNames()`.
pub fn scoreboard_objective_names(server: &MinecraftServer) -> Vec<String> {
    server.scoreboard.read().get_objective_names().map(str::to_owned).collect()
}

/// `scoreboard.getTeamNames()`.
pub fn scoreboard_team_names(server: &MinecraftServer) -> Vec<String> {
    server.scoreboard.read().get_team_names().map(str::to_owned).collect()
}

/// `scoreboard.getPlayerTeam(name) != null`.
pub fn scoreboard_has_team(server: &MinecraftServer, name: &str) -> bool {
    server.scoreboard.read().get_player_team(name).is_some()
}

/// `scoreboard.getTrackedPlayers()` (score holder names).
pub fn scoreboard_tracked_players(server: &MinecraftServer) -> Vec<String> {
    server.scoreboard.read().get_tracked_players()
}

// ---- registries ----

/// The registry `registry_key` (e.g. `minecraft:item`) as the command source sees it: the server's registries
/// (static, data-driven and reloadable ones).
/// Reloadable registries (loot tables, predicates, item modifiers) come from `reloadableRegistries().lookup()`: the
/// full registry access of the loaded resources.
pub fn registry_view(server: &MinecraftServer, registry_key: &str) -> Option<std::sync::Arc<dyn super::RegistryView>> {
    use super::command_build_context::{AccessRegistries, HolderLookupProvider, StaticRegistries};
    AccessRegistries(server.registries.composite_access().clone())
        .lookup(registry_key)
        .or_else(|| AccessRegistries(server.resources.managers.full_registries().clone()).lookup(registry_key))
        .or_else(|| StaticRegistries.lookup(registry_key))
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
    e.save_without_id(&mut output, &lookup);
    if let Some(selected) = player_selected_item_nbt(e) {
        output.put("SelectedItem", selected);
    }
    Some(output)
}

/// `BlockEntity.saveWithFullMetadata` of the block entity at `pos` (block `DataSource`).
pub fn block_entity_data(server: &MinecraftServer, dimension: &DimensionKey, pos: BlockPos) -> Option<mc_nbt::CompoundTag> {
    use crate::world::level::BlockGetter;
    Some(server.level(dimension)?.get_block_entity(pos)?.save_with_full_metadata())
}

/// `CommandStorage.get(id)` (storage `DataSource`).
pub fn command_storage_get(server: &mut MinecraftServer, id: &Identifier) -> mc_nbt::CompoundTag {
    server.command_storage.get(&mut server.saved_data, id)
}

/// `ScoreHolder` of an entity (`getScoreboardName`).
pub fn score_holder_of(server: &MinecraftServer, entity: &EntityRef) -> Option<super::arguments::scores::ScoreHolder> {
    let e = get_entity(server, entity)?;
    Some(super::arguments::scores::ScoreHolder::for_name_only(&e.get_scoreboard_name()))
}

/// `ScoreContents.getScore`: the score of `holder` in `objective` formatted, `empty` when there is none.
pub fn score_component(server: &MinecraftServer, holder: &super::arguments::scores::ScoreHolder, objective: &str) -> Component {
    let scoreboard = server.scoreboard.read();
    if let Some(o) = scoreboard.get_objective(objective)
        && let Some(info) = scoreboard.get_player_score_info(&crate::world::scores::score::for_name_only(&holder.scoreboard_name), objective)
    {
        return info.format_value(o.number_format_or_default(&crate::world::scores::NumberFormat::no_style()));
    }
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
    use mc_worldgen::world_preset::Holder;
    match server.registries.composite_access().level_stem(dimension) {
        Some(stem) => match &stem.dimension_type {
            Holder::Reference(id, _) => id.to_string(),
            Holder::Direct(_) => "[unregistered]".to_owned(),
        },
        None => "[unregistered]".to_owned(),
    }
}

/// `Entity.teleportTo(level, x, y, z, relatives, yRot, xRot, true)` (`ServerPlayer` override: also the head rotation).
/// Returns the teleported entity (a non-player changing dimension is re-created under a new id), `None` on failure.
pub fn teleport_entity(
    server: &mut MinecraftServer,
    victim: &EntityRef,
    destination: &DimensionKey,
    position: mc_core::phys::Vec3,
    relatives: mc_protocol::packets::play::Relatives,
    y_rot: f32,
    x_rot: f32,
) -> Option<EntityRef> {
    use crate::server::players::dimension_change::{PostTeleportTransition, TeleportTransition, teleport_entity, teleport_player};
    let transition = TeleportTransition::new(destination.clone(), position, mc_core::phys::Vec3::ZERO, y_rot, x_rot, PostTeleportTransition::DO_NOTHING).with_relatives(relatives);
    let is_player = get_entity(server, victim)?.is_player();
    let result = if is_player {
        // `ServerPlayer.teleportTo`: a sleeping player wakes up and the camera is reset first
        server.level_mut(&victim.dimension)?.with_player(victim.id, |level, player| {
            if player.player.living.is_sleeping() {
                player.stop_sleep_in_bed(level, true, true);
            }
            player.set_camera(level, None);
        });
        if !teleport_player(server, &victim.dimension, victim.id, &transition) {
            return None;
        }
        let result = EntityRef { dimension: destination.clone(), id: victim.id };
        if let Some(player) = server.level_mut(&result.dimension).and_then(|l| l.get_entity_mut(result.id)) {
            let head = if relatives.contains(mc_protocol::packets::play::Relative::YRot) { player.get_y_head_rot() + y_rot } else { y_rot };
            player.set_y_head_rot(head);
            if let Some(listener) = player.as_any_mut().downcast_mut::<crate::world::entity::ServerPlayer>().and_then(|p| p.listener.as_mut()) {
                listener.reset_flying_ticks();
            }
        }
        result
    } else {
        let id = teleport_entity(server, &victim.dimension, victim.id, &transition)?;
        EntityRef { dimension: destination.clone(), id }
    };
    Some(result)
}

/// `TeleportCommand.performTeleport`'s tail for a successfully teleported `victim`: keeps only horizontal momentum and
/// `setOnGround(true)` (with the supporting-block check) unless it is fall flying.
///
/// SEAM(mobs): `if (victim instanceof PathfinderMob mob) mob.getNavigation().stop()` once `PathfinderMob` exists.
pub fn after_teleport(server: &mut MinecraftServer, victim: &EntityRef) {
    let Some(level) = server.level_mut(&victim.dimension) else { return };
    level.with_entity(victim.id, |level, entity| {
        if entity.living().is_some_and(|l| l.is_fall_flying()) {
            return;
        }
        let dm = entity.base().delta_movement();
        entity.base_mut().set_delta_movement(mc_core::phys::Vec3::new(dm.x, 0.0, dm.z));
        entity.base_mut().set_on_ground_raw(true);
        crate::world::entity::movement::check_supporting_block(entity, &*level, true, None);
    });
}

/// `Entity.lookAt(anchor, pos)`.
pub fn look_at(server: &mut MinecraftServer, target: &EntityRef, anchor: super::arguments::entity_anchor_argument::Anchor, pos: mc_core::phys::Vec3) {
    look_at_impl(server, target, anchor, pos, None);
}

/// `ServerPlayer.lookAt(fromAnchor, entity, toAnchor)` / `target.lookAt(fromAnchor, toAnchor.apply(entity))`: the player
/// is told to look at the entity (the packet follows it), everything else looks at the entity's current anchor.
pub fn look_at_entity(
    server: &mut MinecraftServer,
    target: &EntityRef,
    from_anchor: super::arguments::entity_anchor_argument::Anchor,
    entity: &EntityRef,
    to_anchor: super::arguments::entity_anchor_argument::Anchor,
) {
    let Some(pos) = get_entity(server, entity).map(|e| to_anchor.apply_entity(e.base().position(), e.base().eye_height)) else { return };
    look_at_impl(server, target, from_anchor, pos, Some((entity.id, to_anchor)));
}

fn look_at_impl(
    server: &mut MinecraftServer,
    target: &EntityRef,
    anchor: super::arguments::entity_anchor_argument::Anchor,
    pos: mc_core::phys::Vec3,
    at_entity: Option<(EntityId, super::arguments::entity_anchor_argument::Anchor)>,
) {
    use super::arguments::entity_anchor_argument::Anchor;
    use mc_protocol::packets::play::{Anchor as WireAnchor, ClientboundPlayerLookAtPacket, PlayerLookAtEntity};
    let Some(entity) = server.level_mut(&target.dimension).and_then(|l| l.get_entity_mut(target.id)) else { return };
    entity.look_at(anchor, pos);
    let wire = |a: Anchor| match a {
        Anchor::Feet => WireAnchor::Feet,
        Anchor::Eyes => WireAnchor::Eyes,
    };
    // only a `ServerPlayer` overrides `lookAt` to tell its client; the entity variant is sent only for the player overload
    if let Some(player) = entity.as_any().downcast_ref::<crate::world::entity::ServerPlayer>() {
        player.send(&ClientboundPlayerLookAtPacket {
            from_anchor: wire(anchor),
            x: pos.x,
            y: pos.y,
            z: pos.z,
            at_entity: at_entity.map(|(entity, to_anchor)| PlayerLookAtEntity { entity, to_anchor: wire(to_anchor) }),
        });
    }
}

/// `ServerPlayer.setGameMode(mode)`: false when the player already has it.
pub fn set_game_mode(server: &mut MinecraftServer, player: &EntityRef, game_mode: mc_world::level_storage::GameType) -> bool {
    use mc_data::enums::GameType;
    let mode = match game_mode {
        mc_world::level_storage::GameType::Survival => GameType::Survival,
        mc_world::level_storage::GameType::Creative => GameType::Creative,
        mc_world::level_storage::GameType::Adventure => GameType::Adventure,
        mc_world::level_storage::GameType::Spectator => GameType::Spectator,
    };
    let Some(level) = server.level_mut(&player.dimension) else { return false };
    level.with_player(player.id, |level, sp| sp.set_game_mode(level, mode)).unwrap_or(false)
}

/// Kicks `player` (`player.connection.disconnect(reason)`); the player's display name for the feedback.
pub fn kick_player(server: &mut MinecraftServer, player: &EntityRef, reason: &Component) -> Option<Component> {
    let handle = player_handle(server, player)?;
    let name = player_display_name(server, &handle);
    handle.disconnect(reason);
    Some(name)
}

// ==== GiveCommand seams ====

/// `GiveCommand.giveItem`'s per-player loop: gives `count` of `prototype` in stacks of `max_stack_size`.
pub fn give_item(server: &mut MinecraftServer, player: &EntityRef, prototype: &mc_protocol::components::ItemStack, max_stack_size: i32, count: i32) {
    use mc_core::random::RandomSource;
    use mc_protocol::packets::play::level::SoundSource;
    let Some(level) = server.level_mut(&player.dimension) else { return };
    level.with_player(player.id, |level, player| {
        let mut remaining = count;
        while remaining > 0 {
            let size = max_stack_size.min(remaining);
            remaining -= size;
            let mut copy_to_drop = prototype.copy_with_count(size);
            let infinite = player.player.abilities.instabuild;
            let added = player.player.inventory.add(&mut copy_to_drop, infinite);
            if added && copy_to_drop.is_empty() {
                if let Some(mut drop) = player.create_item_stack_to_drop(level, prototype.clone(), false, false) {
                    drop.make_fake_item();
                    level.add_fresh_entity(Box::new(drop));
                }
                let (x, y, z) = (player.base().x(), player.base().y(), player.base().z());
                let random = &mut player.base_mut().random;
                let pitch = ((random.next_float() - random.next_float()) * 0.7 + 1.0) * 2.0;
                level.play_sound_at(None, x, y, z, mc_data::SoundEvent::ENTITY_ITEM_PICKUP, SoundSource::Players, 0.2, pitch);
                player.broadcast_menu_changes(level);
            } else if let Some(mut drop) = player.create_item_stack_to_drop(level, copy_to_drop, false, false) {
                drop.set_no_pick_up_delay();
                drop.set_target(Some(player.uuid()));
                level.add_fresh_entity(Box::new(drop));
            }
        }
    });
}

// ---- DataCommands seams (`/data`: `EntityDataAccessor`, `BlockDataAccessor`, `StorageDataAccessor`) ----

/// `CommandStorage.set(id, contents)`.
pub fn command_storage_set(server: &mut MinecraftServer, id: &Identifier, contents: mc_nbt::CompoundTag) {
    server.command_storage.set(&mut server.saved_data, id, contents);
}

/// `CommandStorage.keys()`.
pub fn command_storage_keys(server: &mut MinecraftServer) -> Vec<Identifier> {
    server.command_storage.keys(&mut server.saved_data)
}

/// `EntityDataAccessor.setData` of a non-player: `entity.load(input)`, then `setUUID(old uuid)` and
/// `postDataManipulated()`. `Err` is vanilla's `IllegalStateException` of `Entity.load` (non-finite position/rotation).
pub fn entity_load_data(server: &mut MinecraftServer, entity: &EntityRef, data: &mc_nbt::CompoundTag) -> Result<(), String> {
    let Some(level) = server.level_mut(&entity.dimension) else { return Ok(()) };
    level
        .with_entity(entity.id, |_, e| {
            let uuid = e.uuid();
            let result = e.load(data);
            e.base_mut().uuid = uuid;
            e.post_data_manipulated();
            result
        })
        .unwrap_or(Ok(()))
}

/// `level.getBlockEntity(pos) != null` (`LevelChunk.getBlockEntity(pos)`: `CHECK` creation).
pub fn block_entity_exists(server: &mut MinecraftServer, dimension: &DimensionKey, pos: BlockPos) -> bool {
    use crate::server::level::level_chunk_ops::EntityCreationType;
    server.level_mut(dimension).is_some_and(|l| l.is_in_valid_bounds(pos) && l.get_block_entity_creating(pos, EntityCreationType::Check))
}

/// `BlockDataAccessor.setData`: `entity.loadWithComponents(input)`, `setChanged()`, `sendBlockUpdated(pos, state, state, 3)`.
pub fn block_entity_load_data(server: &mut MinecraftServer, dimension: &DimensionKey, pos: BlockPos, data: &mc_nbt::CompoundTag) {
    use crate::world::level::BlockGetter;
    let Some(level) = server.level_mut(dimension) else { return };
    let state = BlockGetter::get_block_state(level, pos);
    let Some(()) = level.with_block_entity(pos, |_, e| e.load_with_components(data)) else { return };
    level.block_entity_changed(pos);
    if !state.is_air() {
        level.update_neighbour_for_output_signal(pos, state.block());
    }
    level.send_block_updated(pos, state, state, 3);
}

// ==== AdvancementCommands / RecipeCommand seams ====

/// An `AdvancementHolder` as the commands need it: `id`, `Advancement.name(holder)` and `advancement.criteria().keySet()`.
#[derive(Clone, Debug)]
pub struct AdvancementRef {
    pub id: Identifier,
    pub name: Component,
    pub criteria: Vec<String>,
}

/// `AdvancementNode` of `server.getAdvancements().tree()`: `parent()` and `children()` holders.
pub struct AdvancementNodeRef {
    pub parent: Option<AdvancementRef>,
    pub children: Vec<AdvancementRef>,
}

/// `AdvancementProgress` of a player's advancement (`getOrStartProgress`): `isDone`, `hasProgress`,
/// `getRemainingCriteria`, `getCompletedCriteria`.
pub struct AdvancementProgressRef {
    pub done: bool,
    pub has_progress: bool,
    pub remaining: Vec<String>,
    pub completed: Vec<String>,
}

/// `server.getAdvancements().get(id)`.
/// SEAM(advancements): `ServerAdvancementManager` is not ported yet, so nothing is found.
pub fn advancement_get(server: &MinecraftServer, id: &Identifier) -> Option<AdvancementRef> {
    let _ = (server, id);
    None
}

/// `server.getAdvancements().getAllAdvancements()`.
/// SEAM(advancements).
pub fn advancement_all(_server: &MinecraftServer) -> Vec<AdvancementRef> {
    Vec::new()
}

/// `server.getAdvancements().tree().get(holder)`.
/// SEAM(advancements).
pub fn advancement_node(server: &MinecraftServer, id: &Identifier) -> Option<AdvancementNodeRef> {
    let _ = (server, id);
    None
}

/// `player.getAdvancements().getOrStartProgress(holder)`.
/// SEAM(advancements).
pub fn player_advancement_progress(server: &mut MinecraftServer, player: &EntityRef, id: &Identifier) -> AdvancementProgressRef {
    let _ = (server, player, id);
    AdvancementProgressRef { done: false, has_progress: false, remaining: Vec::new(), completed: Vec::new() }
}

/// `player.getAdvancements().award(holder, criterion)`.
/// SEAM(advancements).
pub fn player_advancement_award(server: &mut MinecraftServer, player: &EntityRef, id: &Identifier, criterion: &str) -> bool {
    let _ = (server, player, id, criterion);
    false
}

/// `player.getAdvancements().revoke(holder, criterion)`.
/// SEAM(advancements).
pub fn player_advancement_revoke(server: &mut MinecraftServer, player: &EntityRef, id: &Identifier, criterion: &str) -> bool {
    let _ = (server, player, id, criterion);
    false
}

/// `player.getAdvancements().flushDirty(player, showAdvancements)`.
/// SEAM(advancements).
pub fn player_advancements_flush_dirty(server: &mut MinecraftServer, player: &EntityRef, show_advancements: bool) {
    let _ = (server, player, show_advancements);
}

/// `server.getRecipeManager().byKey(key)` filtered by `!recipe.isSpecial()`.
pub fn learnable_recipe_exists(server: &MinecraftServer, id: &Identifier) -> bool {
    server.shared.recipe_manager().by_key(id).is_some_and(|r| !r.value.is_special())
}

/// `server.getRecipeManager().getLearnableRecipes()` (ids, in manager order).
pub fn learnable_recipes(server: &MinecraftServer) -> Vec<Identifier> {
    server.shared.recipe_manager().learnable_recipes().iter().map(|r| r.id.clone()).collect()
}

/// `player.awardRecipes(recipes)`: the number of newly unlocked recipes.
pub fn player_award_recipes(server: &mut MinecraftServer, player: &EntityRef, recipes: &[Identifier]) -> i32 {
    let manager = server.shared.recipe_manager();
    let holders: Vec<_> = recipes.iter().filter_map(|id| manager.by_key(id).cloned()).collect();
    server.level_mut(&player.dimension).and_then(|level| level.with_player(player.id, |level, p| p.award_recipes(level, &holders))).unwrap_or(0)
}

/// `player.resetRecipes(recipes)`: the number of removed recipes.
pub fn player_reset_recipes(server: &mut MinecraftServer, player: &EntityRef, recipes: &[Identifier]) -> i32 {
    let manager = server.shared.recipe_manager();
    let holders: Vec<_> = recipes.iter().filter_map(|id| manager.by_key(id).cloned()).collect();
    server.level_mut(&player.dimension).and_then(|level| level.with_player(player.id, |level, p| p.reset_recipes(level, &holders))).unwrap_or(0)
}

// ==== Entity command seams (SummonCommand, KillCommand, RideCommand, RotateCommand, EnchantCommand, ...) ====

/// `entity.getDisplayName()` of a live entity (empty when it is gone).
pub fn display_name_of(source: &super::CommandSourceStack, entity: &EntityRef) -> Component {
    source.with_server(|server| get_entity(server, entity).map_or_else(Component::empty, |e| entity_display_name(server, e)))
}

/// `source.getEntityOrException()` as the live entity: the source's entity is searched in the source's level first (an
/// `execute in` can change the level without moving the entity), then in the other levels.
pub fn source_entity_ref(source: &super::CommandSourceStack) -> Result<EntityRef, mc_brigadier::CommandSyntaxException> {
    let entity = source.get_entity_or_exception()?;
    Ok(source.with_server(|server| {
        let first = source.get_level().clone();
        std::iter::once(first.clone())
            .chain(server.level_keys())
            .find(|dimension| server.level(dimension).and_then(|l| l.get_entity(entity.id)).is_some_and(|e| e.uuid() == entity.uuid))
            .map_or(EntityRef { dimension: first, id: entity.id }, |dimension| EntityRef { dimension, id: entity.id })
    }))
}

/// `if (entity instanceof Mob mob) mob.finalizeSpawn(level, level.getCurrentDifficultyAt(entity.blockPosition()), EntitySpawnReason.COMMAND, null)`.
pub fn finalize_mob_spawn(level: &mut crate::server::level::ServerLevel, entity: &mut dyn Entity) {
    use crate::world::level::server_level_accessor::ServerLevelAccessor;
    if entity.mob().is_none() {
        return;
    }
    let difficulty = ServerLevelAccessor::get_current_difficulty_at(level, entity.block_position());
    entity.finalize_spawn(level, difficulty, mc_data::enums::EntitySpawnReason::Command, None);
}

/// `entity.kill(level)` (`LivingEntity`: hurts itself with `genericKill`).
pub fn kill_entity(server: &mut MinecraftServer, entity: &EntityRef) {
    if let Some(level) = server.level_mut(&entity.dimension) {
        level.with_entity(entity.id, |level, e| e.kill_in_level(level));
    }
}

/// `Entity.forceSetRotation(yRot, relativeY, xRot, relativeX)` (`ServerPlayer` override: also the rotation packet).
pub fn force_set_rotation(server: &mut MinecraftServer, target: &EntityRef, y_rot: f32, relative_y: bool, x_rot: f32, relative_x: bool) {
    use crate::world::entity::entity::{calculate_absolute, position_move_rotation_of};
    use mc_protocol::packets::play::{ClientboundPlayerRotationPacket, PositionMoveRotation, Relative, Relatives};
    let Some(entity) = server.level_mut(&target.dimension).and_then(|l| l.get_entity_mut(target.id)) else { return };
    let mut relatives = Relatives::NONE;
    if relative_y {
        relatives.insert(Relative::YRot);
    }
    if relative_x {
        relatives.insert(Relative::XRot);
    }
    let destination = PositionMoveRotation { y_rot, x_rot, ..position_move_rotation_of(entity) };
    let absolute = calculate_absolute(position_move_rotation_of(entity), destination, relatives);
    entity.set_y_rot(absolute.y_rot);
    entity.set_y_head_rot(absolute.y_rot);
    entity.set_x_rot(absolute.x_rot);
    let base = entity.base_mut();
    base.set_old_rot_values(base.y_rot(), base.x_rot());
    if let Some(player) = entity.as_any().downcast_ref::<crate::world::entity::ServerPlayer>() {
        player.send(&ClientboundPlayerRotationPacket { y_rot, relative_y, x_rot, relative_x });
    }
}

/// `Enchantment.getFullname(enchantment, level)`: the description coloured gray (red for curses) with the level appended
/// unless it is a single-level enchantment at level 1.
pub fn enchantment_full_name(level: &crate::server::level::ServerLevel, enchantment: &Identifier, enchantment_level: i32) -> Component {
    use crate::world::item::enchantment_helper::definition;
    use mc_text::{ChatFormatting, style::Style};
    let Some(definition) = definition(&level.registries, enchantment) else { return Component::empty() };
    let curse = Identifier::parse("minecraft:curse").expect("valid tag id");
    let color = if level.registries.is_in_tag(crate::registry::keys::ENCHANTMENT, enchantment, &curse) { ChatFormatting::Red } else { ChatFormatting::Gray };
    let style = Style::EMPTY.with_color_formatting(Some(color));
    // `ComponentUtils.mergeStyles(description, style)`: the description's own style wins
    let inner = definition.description.style().clone();
    let merged = if inner.is_empty() { style } else if inner == style { inner } else { inner.apply_to(&style) };
    let mut result = definition.description.clone().set_style(merged);
    if enchantment_level != 1 || definition.get_max_level() != 1 {
        result = result.append(mc_text::util::space()).append(Component::translatable(format!("enchantment.level.{enchantment_level}"), Vec::new()));
    }
    result
}

// ==== ExecuteCommand seams ====

/// The facts `CommandSourceStack.withEntity(entity)` needs from the live `entity`.
pub fn command_entity(server: &MinecraftServer, entity: &EntityRef) -> Option<super::CommandEntity> {
    let e = get_entity(server, entity)?;
    Some(super::CommandEntity {
        id: entity.id,
        uuid: e.uuid(),
        is_player: e.is_player(),
        display_name: entity_display_name(server, e),
        plain_text_name: entity_plain_text_name(e),
        eye_height: e.base().eye_height,
        player: if e.is_player() { player_handle(server, entity) } else { None },
    })
}

/// `entity.position()`.
pub fn entity_position(server: &MinecraftServer, entity: &EntityRef) -> Option<mc_core::phys::Vec3> {
    Some(get_entity(server, entity)?.position())
}

/// `entity.getRotationVector()`: `(xRot, yRot)`.
pub fn entity_rotation_vector(server: &MinecraftServer, entity: &EntityRef) -> Option<mc_core::phys::Vec2> {
    let base = get_entity(server, entity)?.base();
    Some(mc_core::phys::Vec2::new(base.x_rot(), base.y_rot()))
}

/// `DimensionType.getTeleportationScale(from, to)` of the two levels' dimension types.
pub fn teleportation_scale(server: &MinecraftServer, from: &DimensionKey, to: &DimensionKey) -> f64 {
    coordinate_scale(server, from) / coordinate_scale(server, to)
}

/// `level.dimensionType().coordinateScale()`.
fn coordinate_scale(server: &MinecraftServer, dimension: &DimensionKey) -> f64 {
    server.registries.composite_access().level_stem(dimension).map_or(1.0, |stem| stem.dimension_type.value().coordinate_scale)
}

/// `ExecuteCommand.isChunkLoaded`: the chunk is entity-ticking and its entities are loaded.
pub fn is_chunk_entity_ticking(server: &MinecraftServer, dimension: &DimensionKey, pos: BlockPos) -> bool {
    use crate::server::level::FullChunkStatus;
    use mc_core::pos::ChunkPos;
    let Some(level) = server.level(dimension) else { return false };
    let (x, z) = (pos.x() >> 4, pos.z() >> 4);
    let key = ChunkPos::pack_xz(x, z);
    level.chunk_source.get_chunk_now(x, z).is_some()
        && level.chunk_source.chunk_map.get_visible_chunk_if_present(key).is_some_and(|h| h.get_full_status() == FullChunkStatus::EntityTicking)
        && level.are_entities_loaded(key)
}

/// `level.getHeight(type, x, z)`.
pub fn level_height(server: &MinecraftServer, dimension: &DimensionKey, ty: mc_world::heightmap::HeightmapType, x: i32, z: i32) -> Option<i32> {
    use crate::world::level::LevelReader;
    Some(server.level(dimension)?.get_height_at(ty, x, z))
}

/// `level.getBiome(pos).unwrapKey()`.
pub fn biome_key_at(server: &MinecraftServer, dimension: &DimensionKey, pos: BlockPos) -> Option<Identifier> {
    server.level(dimension)?.get_biome_key(pos)
}

/// The relations of `/execute on`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EntityRelation {
    Owner,
    Leasher,
    Target,
    Attacker,
    Vehicle,
    Controller,
    Origin,
    Passengers,
}

/// The entities `relation` leads to from `entity`, before the `!isRemoved()` filter `/execute on` applies:
/// `OwnableEntity.getOwner`, `Leashable.getLeashHolder`, `Targeting.getTarget`, `Attackable.getLastAttacker`,
/// `getVehicle`, `getControllingPassenger`, `TraceableEntity.getOwner`, `getPassengers`.
/// `TraceableEntity` covers the projectiles, item entities and primed TNT that exist so far; `OwnableEntity` goes
/// through [`Entity::ownable_owner_uuid`] (SEAM(mobs): the tameable/horse classes implement it).
pub fn entity_relation(server: &MinecraftServer, entity: &EntityRef, relation: EntityRelation) -> Vec<EntityRef> {
    use crate::world::entity::item::ItemEntity;
    use crate::world::entity::item::primed_tnt::PrimedTnt;
    let Some(level) = server.level(&entity.dimension) else { return Vec::new() };
    let Some(e) = level.get_entity(entity.id) else { return Vec::new() };
    let by_uuid = |uuid: Option<uuid::Uuid>| -> Vec<EntityId> { uuid.and_then(|u| level.get_entity_by_uuid(u)).map(|o| o.id()).into_iter().collect() };
    let ids: Vec<EntityId> = match relation {
        EntityRelation::Vehicle => e.base().vehicle.into_iter().collect(),
        EntityRelation::Controller => e.get_controlling_passenger().into_iter().collect(),
        EntityRelation::Passengers => e.base().passengers.clone(),
        EntityRelation::Leasher => e.leash_holder().into_iter().collect(),
        EntityRelation::Target => e.get_target().into_iter().collect(),
        EntityRelation::Owner => by_uuid(e.ownable_owner_uuid()),
        // `LivingEntity.getLastHurtByMob`: a living entity of the level.
        EntityRelation::Attacker => by_uuid(e.living().and_then(|l| l.last_hurt_by_mob).filter(|&u| level.get_entity_by_uuid(u).is_some_and(|o| o.living().is_some()))),
        EntityRelation::Origin => match e.as_projectile() {
            Some(projectile) => projectile.get_owner(level).map(|o| o.id()).into_iter().collect(),
            None => match (e.as_any().downcast_ref::<ItemEntity>(), e.as_any().downcast_ref::<PrimedTnt>()) {
                (Some(item), _) => item.get_owner(level).into_iter().collect(),
                (_, Some(tnt)) => tnt.get_owner(level).map(|o| o.id()).into_iter().collect(),
                _ => Vec::new(),
            },
        },
    };
    ids.into_iter()
        .filter(|&id| level.get_entity(id).is_some_and(|o| !o.is_removed()))
        .map(|id| EntityRef { dimension: entity.dimension.clone(), id })
        .collect()
}

/// The live entity a stack's [`CommandEntity`](super::CommandEntity) stands for (`source.getEntity()`): looked up in
/// `preferred` (the stack's level) first, then by UUID in every level (a stack may run `in` another dimension).
pub fn entity_ref_of(server: &MinecraftServer, entity: &super::CommandEntity, preferred: &DimensionKey) -> Option<EntityRef> {
    if server.level(preferred).and_then(|l| l.get_entity(entity.id)).is_some_and(|e| e.uuid() == entity.uuid) {
        return Some(EntityRef { dimension: preferred.clone(), id: entity.id });
    }
    server.level_keys().into_iter().find_map(|dimension| {
        let found = server.level(&dimension)?.get_entity_by_uuid(entity.uuid)?;
        Some(EntityRef { dimension, id: found.id() })
    })
}

/// `ExecuteCommand.checkCustomPredicate`: evaluates the loot predicate with `LootContextParams.ORIGIN = origin`,
/// `THIS_ENTITY = entity` in `LootContextParamSets.COMMAND`. A reference that is not registered, or an inline predicate
/// that does not decode, cannot test true (vanilla rejects both while parsing the argument).
pub fn loot_predicate_test(
    server: &MinecraftServer,
    dimension: &DimensionKey,
    origin: mc_core::phys::Vec3,
    entity: Option<&EntityRef>,
    predicate: &super::arguments::resource_arguments::ResourceOrId,
) -> bool {
    use super::arguments::resource_arguments::ResourceOrId;
    use crate::registry::keys;
    use crate::world::level::storage::loot::parameters::LootContextParamSets;
    use crate::world::level::storage::loot::predicates::{ConditionRef, condition_codec};
    use crate::world::level::storage::loot::context::VisitedEntry;
    use crate::world::level::storage::loot::{LootContext, LootParams};
    let Some(level) = server.level(dimension) else { return false };
    let condition: ConditionRef = match predicate {
        ResourceOrId::Reference(key) => match level.registries.get::<ConditionRef>(keys::PREDICATE, &key.id) {
            Some(condition) => condition.clone(),
            None => return false,
        },
        ResourceOrId::Inline(tag) => match mc_dfu::parse_nbt(&condition_codec(), tag).into_result() {
            Ok(condition) => condition,
            Err(_) => return false,
        },
    };
    let this_entity = entity.and_then(|e| get_entity(server, e));
    let params = LootParams::builder(level).origin(origin).optional_this_entity(this_entity).create(&LootContextParamSets::COMMAND);
    let mut context = LootContext::builder(&params).create(None);
    context.push_visited_element(VisitedEntry::predicate(&condition));
    condition.test(&mut context)
}
