//! `PlayerList.placeNewPlayer` and `sendLevelInfo`: the join sequence, packet for packet in vanilla's order.
//!
//! Everything that is *world or server state* (level data, game rules, world border, clocks, recipe manager,
//! scoreboard, command tree, name cache, status) is reached through [`JoinHost`]: the server glue implements it over
//! `(&mut MinecraftServer, &mut ServerLevel)`; the sequence itself lives here and is exercised by the unit tests with a
//! recording host against the packet order recorded from the real server (`tests/fixtures/vanilla_join_sequence.txt`).

use super::name_and_id::NameAndId;
use super::player_handle::{PlayerHandle, create_player_initializing};
use super::player_list::{PlayerList, join_message, permission_level_event_id};
use crate::server::network::ConnectionSender;
use crate::server::network::common_listener::CommonListenerCookie;
use crate::server::network::game_listener::ServerGamePacketListenerImpl;
use crate::server::permissions::LevelBasedPermissionSet;
use crate::world::entity::effect::MobEffectInstance;
use crate::world::entity::{EntityId, ServerPlayer};
use crate::world::level::DimensionKey;
use mc_core::ident::Identifier;
use mc_nbt::Tag;
use mc_protocol::packets::play::{
    ClientboundChangeDifficultyPacket, ClientboundEntityEventPacket, ClientboundGameEventPacket, ClientboundInitializeBorderPacket,
    ClientboundLoginPacket, ClientboundPlayerAbilitiesPacket, ClientboundPostEffectsPacket, ClientboundRecipeBookAddEntry,
    ClientboundRecipeBookAddPacket, ClientboundRecipeBookSettingsPacket, ClientboundServerDataPacket, ClientboundSetDefaultSpawnPositionPacket,
    ClientboundSetHeldSlotPacket, ClientboundSetTimePacket, ClientboundTickingStatePacket, ClientboundTickingStepPacket,
    ClientboundUpdateMobEffectPacket, ClientboundUpdateRecipesPacket, CommonPlayerSpawnInfo, Difficulty, GameType as WireGameType,
    RecipeBookSettings, RecipeBookSettingsTypeSettings, RespawnData,
};
use mc_data::enums::GameType;
use uuid::Uuid;

/// The level facts of `CommonPlayerSpawnInfo` and the login/respawn packets (`ServerLevel` getters).
#[derive(Clone, Debug)]
pub struct LevelJoinInfo {
    /// `levelData.isHardcore()`.
    pub hardcore: bool,
    /// `levelData.getDifficulty()` / `isDifficultyLocked()`.
    pub difficulty: Difficulty,
    pub difficulty_locked: bool,
    /// `GameRules.IMMEDIATE_RESPAWN` / `REDUCED_DEBUG_INFO` / `LIMITED_CRAFTING`.
    pub immediate_respawn: bool,
    pub reduced_debug_info: bool,
    pub limited_crafting: bool,
    /// `level.dimensionTypeRegistration()` registry id.
    pub dimension_type: i32,
    /// `BiomeManager.obfuscateSeed(level.getSeed())`.
    pub seed_hash: i64,
    pub is_debug: bool,
    pub is_flat: bool,
    pub sea_level: i32,
    /// `new ClientboundInitializeBorderPacket(level.getWorldBorder())`.
    pub border: ClientboundInitializeBorderPacket,
    /// `level.getRespawnData()`.
    pub respawn_data: RespawnData,
    /// `(rainLevel, thunderLevel)` of `getRainLevel(1.0)`/`getThunderLevel(1.0)` when `isRaining()`.
    pub raining: Option<(f32, f32)>,
}

/// What `placeNewPlayer`/`respawn` need from the server and the player's level.
pub trait JoinHost {
    /// `server.getPlayerList()`.
    fn list(&mut self) -> &mut PlayerList;
    /// `server.levelKeys()` (vanilla `getAllLevels` dimensions in order).
    fn level_keys(&self) -> Vec<DimensionKey>;
    /// The level facts of the level the player is placed in.
    fn level_join_info(&mut self) -> LevelJoinInfo;
    /// `recipeManager.getSynchronizedItemProperties()/StonecutterRecipes()`.
    fn update_recipes_packet(&self) -> ClientboundUpdateRecipesPacket;
    /// `server.getCommands().sendCommands(player)` (after the op-level entity event).
    fn send_commands(&mut self, player: &PlayerHandle, permissions: LevelBasedPermissionSet);
    /// `updateEntireScoreboard(level.getScoreboard(), player)`.
    fn update_entire_scoreboard(&mut self, player: &PlayerHandle);
    /// `server.invalidateStatus()`.
    fn invalidate_status(&mut self);
    /// `ServerPlayer.sendServerStatus(status)` payload (`None` while there is no status yet).
    fn server_data_packet(&self) -> Option<ClientboundServerDataPacket>;
    /// `nameToIdCache.get(id)` / `.add(nameAndId)`.
    fn name_cache_get(&self, id: &Uuid) -> Option<String>;
    fn name_cache_add(&mut self, name_and_id: &NameAndId);
    /// `server.clockManager().createFullSyncPacket()`.
    fn clock_sync_packet(&self) -> ClientboundSetTimePacket;
    /// `server.tickRateManager().updateJoiningPlayer(player)`.
    fn ticking_packets(&self) -> (ClientboundTickingStatePacket, ClientboundTickingStepPacket);
    /// `server.getCustomBossEvents().onPlayerConnect(player)`.
    fn custom_boss_events_on_player_connect(&mut self, player: &PlayerHandle);
    /// `level.addNewPlayer(player)`: the level takes ownership of the entity.
    fn add_new_player(&mut self, player: ServerPlayer);
    /// Runs `f` on the (now level-owned) player with its level.
    fn with_player(&mut self, id: EntityId, f: &mut dyn FnMut(&mut crate::server::level::ServerLevel, &mut ServerPlayer));
}

/// `new ClientboundUpdateMobEffectPacket(entityId, effect, blend)`.
pub fn mob_effect_packet(entity_id: EntityId, effect: &MobEffectInstance, blend: bool) -> ClientboundUpdateMobEffectPacket {
    let mut flags = 0u8;
    if effect.ambient {
        flags |= ClientboundUpdateMobEffectPacket::FLAG_AMBIENT;
    }
    if effect.visible {
        flags |= ClientboundUpdateMobEffectPacket::FLAG_VISIBLE;
    }
    if effect.show_icon {
        flags |= ClientboundUpdateMobEffectPacket::FLAG_SHOW_ICON;
    }
    if blend {
        flags |= ClientboundUpdateMobEffectPacket::FLAG_BLEND;
    }
    ClientboundUpdateMobEffectPacket { entity_id, effect: effect.effect, effect_amplifier: effect.amplifier, effect_duration_ticks: effect.duration, flags }
}

fn wire_game_type(game_type: GameType) -> WireGameType {
    match game_type {
        GameType::Survival => WireGameType::Survival,
        GameType::Creative => WireGameType::Creative,
        GameType::Adventure => WireGameType::Adventure,
        GameType::Spectator => WireGameType::Spectator,
    }
}

/// `ServerPlayer.createCommonSpawnInfo(level)`.
pub fn create_common_spawn_info(player: &ServerPlayer, level: &LevelJoinInfo, dimension: &DimensionKey) -> CommonPlayerSpawnInfo {
    CommonPlayerSpawnInfo {
        dimension_type: level.dimension_type,
        dimension: dimension.clone(),
        seed: level.seed_hash,
        game_type: wire_game_type(player.game_mode.game_mode_for_player),
        previous_game_type: player.game_mode.previous_game_mode_for_player.map(wire_game_type),
        is_debug: level.is_debug,
        is_flat: level.is_flat,
        last_death_location: player.player.last_death_location.clone(),
        portal_cooldown: player.player.living.entity.portal_cooldown,
        sea_level: level.sea_level,
    }
}

/// `new ClientboundPlayerAbilitiesPacket(player.getAbilities())`.
pub fn abilities_packet(player: &ServerPlayer) -> ClientboundPlayerAbilitiesPacket {
    let a = &player.player.abilities;
    ClientboundPlayerAbilitiesPacket {
        invulnerable: a.invulnerable,
        is_flying: a.flying,
        can_fly: a.mayfly,
        instabuild: a.instabuild,
        flying_speed: a.get_flying_speed(),
        walking_speed: a.get_walking_speed(),
    }
}

/// `RecipeBookSettings` of the saved `recipeBook` tag (`isGuiOpen`, `isFilteringCraftable`, `isFurnaceGuiOpen`, ...).
pub fn recipe_book_settings(recipe_book: Option<&Tag>) -> RecipeBookSettings {
    let flag = |key: &str| recipe_book.and_then(Tag::as_compound).and_then(|c| c.get(key)).and_then(Tag::as_boolean).unwrap_or(false);
    let ty = |open: &str, filtering: &str| RecipeBookSettingsTypeSettings { open: flag(open), filtering: flag(filtering) };
    RecipeBookSettings {
        crafting: ty("isGuiOpen", "isFilteringCraftable"),
        furnace: ty("isFurnaceGuiOpen", "isFurnaceFilteringCraftable"),
        blast_furnace: ty("isBlastingFurnaceGuiOpen", "isBlastingFurnaceFilteringCraftable"),
        smoker: ty("isSmokerGuiOpen", "isSmokerFilteringCraftable"),
    }
}

fn recipe_ids(recipe_book: Option<&Tag>, key: &str) -> Vec<Identifier> {
    recipe_book
        .and_then(Tag::as_compound)
        .and_then(|c| c.get_list(key))
        .map(|l| l.iter().filter_map(|t| t.as_string().and_then(Identifier::try_parse)).collect())
        .unwrap_or_default()
}

/// `ServerRecipeBook` display resolution of the known recipes (`RecipeManager.listDisplaysForRecipe`): the recipe
/// manager is a later wave, so no displays resolve.
pub fn recipe_book_entries(_known: &[Identifier], _highlight: &[Identifier]) -> Vec<ClientboundRecipeBookAddEntry> {
    Vec::new()
}

/// `ServerRecipeBook.sendInitialRecipeBook(player)`.
pub fn send_initial_recipe_book(player: &ServerPlayer) {
    let book = player.recipe_book.as_ref();
    player.send(&ClientboundRecipeBookSettingsPacket { book_settings: recipe_book_settings(book) });
    let known = recipe_ids(book, "recipes");
    let highlight = recipe_ids(book, "toBeDisplayed");
    player.send(&ClientboundRecipeBookAddPacket { entries: recipe_book_entries(&known, &highlight), replace: true });
}

/// `ServerPlayer.sendPostEffects()`.
pub fn send_post_effects(player: &mut ServerPlayer) {
    player.post_effects_dirty = false;
    player.send(&ClientboundPostEffectsPacket { post_effects: player.player.post_effects.clone() });
}

/// `sendActiveEffects(livingEntity, connection)` (`PlayerList.sendActivePlayerEffects`).
pub fn send_active_player_effects(player: &ServerPlayer) {
    let id = player.base_id();
    for effect in player.player.living.active_effects.values() {
        player.send(&mob_effect_packet(id, effect, false));
    }
}

/// Everything `new ServerGamePacketListenerImpl(server, connection, player, cookie)` needs besides the player.
pub struct ListenerParams {
    pub cookie: CommonListenerCookie,
    /// `Util.getMillis()`.
    pub now_ms: i64,
    pub command_spam_threshold_seconds: i32,
    pub chat_spam_threshold_seconds: i32,
    pub memory_connection: bool,
    /// `connection.getLoggableAddress(logIPs)` source: the peer address (`None` for a local connection).
    pub remote_address: Option<std::net::SocketAddr>,
    /// The connection's inbound packet queue (`PendingPlayer.inbound`), owned by the listener from here on.
    pub inbound: std::sync::mpsc::Receiver<crate::server::network::InboundPacket>,
    /// `server::enforceSecureProfile`.
    pub enforce_secure_profile: std::sync::Arc<dyn Fn() -> bool + Send + Sync>,
}

/// `connection.getLoggableAddress(logIps)`.
pub fn loggable_address(remote: Option<&std::net::SocketAddr>, log_ips: bool) -> String {
    match remote {
        None => "local".to_owned(),
        Some(addr) if log_ips => format!("/{addr}"),
        Some(_) => "IP hidden".to_owned(),
    }
}

/// `sendLevelInfo(player, level)`.
pub fn send_level_info(host: &dyn JoinHost, info: &LevelJoinInfo, player: &PlayerHandle) {
    let c = &player.connection;
    c.send(&info.border);
    c.send(&host.clock_sync_packet());
    c.send(&ClientboundSetDefaultSpawnPositionPacket { respawn_data: info.respawn_data.clone() });
    if let Some((rain, thunder)) = info.raining {
        c.send(&ClientboundGameEventPacket { event: ClientboundGameEventPacket::START_RAINING, param: 0.0 });
        c.send(&ClientboundGameEventPacket { event: ClientboundGameEventPacket::RAIN_LEVEL_CHANGE, param: rain });
        c.send(&ClientboundGameEventPacket { event: ClientboundGameEventPacket::THUNDER_LEVEL_CHANGE, param: thunder });
    }
    c.send(&ClientboundGameEventPacket { event: ClientboundGameEventPacket::LEVEL_CHUNKS_LOAD_START, param: 0.0 });
    let (state, step) = host.ticking_packets();
    c.send(&state);
    c.send(&step);
}

/// `PlayerList.placeNewPlayer(connection, player, cookie)`. `player` has no connection yet; it is given the sender,
/// the play listener and its [`PlayerHandle`] here. Returns the handle.
pub fn place_new_player(
    host: &mut dyn JoinHost,
    mut player: ServerPlayer,
    sender: ConnectionSender,
    params: ListenerParams,
) -> PlayerHandle {
    let profile = player.player.game_profile.clone();
    let name_and_id = NameAndId::new(profile.id, profile.name.clone());
    let old_name = host.name_cache_get(&name_and_id.id).unwrap_or_else(|| name_and_id.name.clone());
    host.name_cache_add(&name_and_id);
    let info = host.level_join_info();
    let id = player.base_id();
    let pos = player.player.living.entity.position();
    log::info!(
        "{}[{}] logged in with entity id {} at ({}, {}, {})",
        profile.name,
        loggable_address(params.remote_address.as_ref(), host.list().settings.log_ips),
        id,
        java_double(pos.x),
        java_double(pos.y),
        java_double(pos.z)
    );

    let ip_address = params.remote_address.map_or_else(|| "<unknown>".to_owned(), |a| a.ip().to_string());
    let handle = PlayerHandle::new(id, profile.clone(), sender.clone(), ip_address, player.dimension.clone());
    player.connection = Some(sender.clone());
    player.listener = Some(ServerGamePacketListenerImpl::new(
        sender.clone(),
        params.inbound,
        &params.cookie,
        handle.chat_out.clone(),
        crate::server::network::game_listener::ListenerSettings {
            command_spam_threshold_seconds: params.command_spam_threshold_seconds,
            chat_spam_threshold_seconds: params.chat_spam_threshold_seconds,
            memory_connection: params.memory_connection,
            enforce_secure_profile: params.enforce_secure_profile,
        },
        params.now_ms,
    ));
    handle.shared.lock().latency = params.cookie.latency;
    player.handle = Some(handle.clone());
    player.sync_handle();

    player.send(&ClientboundLoginPacket {
        player_id: id,
        hardcore: info.hardcore,
        levels: host.level_keys(),
        max_players: host.list().get_max_players(),
        chunk_radius: host.list().get_view_distance(),
        simulation_distance: host.list().get_simulation_distance(),
        reduced_debug_info: info.reduced_debug_info,
        show_death_screen: !info.immediate_respawn,
        do_limited_crafting: info.limited_crafting,
        common_player_spawn_info: create_common_spawn_info(&player, &info, &player.dimension),
        online_mode: host.list().settings.uses_authentication,
        enforces_secure_chat: host.list().settings.enforce_secure_profile,
    });
    player.send(&ClientboundChangeDifficultyPacket { difficulty: info.difficulty, locked: info.difficulty_locked });
    player.send(&abilities_packet(&player));
    player.send(&ClientboundSetHeldSlotPacket { slot: player.player.inventory.get_selected_slot() });
    player.send(&host.update_recipes_packet());
    send_player_permission_level(host, &handle);
    // `player.getStats().markAllDirty()`: the statistics are loaded (and their file created) with the first access
    host.list().get_player_stats(&handle);
    send_initial_recipe_book(&player);
    host.update_entire_scoreboard(&handle);
    host.invalidate_status();
    let display_name = mc_text::Component::literal(profile.name.clone());
    let renamed = (!profile.name.eq_ignore_ascii_case(&old_name)).then_some(old_name.as_str());
    host.list().broadcast_system_message(&join_message(display_name, renamed), false);
    let position = player.player.living.entity.position();
    let (y_rot, x_rot) = (player.player.living.entity.y_rot(), player.player.living.entity.x_rot());
    player.connection_teleport(position.x, position.y, position.z, y_rot, x_rot);
    if let Some(status) = host.server_data_packet()
        && !params.cookie.transferred
    {
        player.send(&status);
    }
    player.send(&create_player_initializing(host.list().players()));
    host.list().add_player(handle.clone());
    host.list().broadcast_all(&create_player_initializing([&handle]));
    send_level_info(host, &info, &handle);
    host.add_new_player(player);
    host.custom_boss_events_on_player_connect(&handle);
    host.with_player(id, &mut |level, player| {
        send_active_player_effects(player);
        send_post_effects(player);
        player.init_inventory_menu(level);
    });
    host.list().notifications.player_joined(&name_and_id);
    handle
}

/// `sendPlayerPermissionLevel(player)`: the op-level entity event, then the command tree.
pub fn send_player_permission_level(host: &mut dyn JoinHost, player: &PlayerHandle) {
    let permissions = host.list().get_profile_permissions(&player.name_and_id());
    send_player_permission_level_with(host, player, permissions);
}

pub fn send_player_permission_level_with(host: &mut dyn JoinHost, player: &PlayerHandle, permissions: LevelBasedPermissionSet) {
    player.connection.send(&ClientboundEntityEventPacket { entity_id: player.id, event_id: permission_level_event_id(permissions) });
    host.send_commands(player, permissions);
}

/// Java `Double.toString` (the coordinates in the login log line).
fn java_double(v: f64) -> String {
    mc_core::math::java::java_double_to_string(v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::level::ServerLevel;
    use crate::server::notifications::EmptyNotificationService;
    use crate::server::players::player_handle::test_support::handle;
    use crate::server::players::player_list::PlayerListSettings;
    use mc_core::ident::Identifier;
    use mc_protocol::codec::{GameProfile, GlobalPos};
    use mc_protocol::packet::{ConnectionState, PacketFlow, packet_name};
    use mc_protocol::packets::play::{ClientboundCommandsEntry, ClientboundCommandsPacket};
    use mc_world::level_storage::PlayerDataStorage;
    use std::sync::Arc;

    struct RecordingHost {
        list: PlayerList,
        level: ServerLevel,
        player: Option<ServerPlayer>,
        events: Vec<&'static str>,
    }

    impl JoinHost for RecordingHost {
        fn list(&mut self) -> &mut PlayerList {
            &mut self.list
        }
        fn level_keys(&self) -> Vec<DimensionKey> {
            vec![Identifier::parse("minecraft:overworld").unwrap()]
        }
        fn level_join_info(&mut self) -> LevelJoinInfo {
            LevelJoinInfo {
                hardcore: false,
                difficulty: Difficulty::Normal,
                difficulty_locked: false,
                immediate_respawn: false,
                reduced_debug_info: false,
                limited_crafting: false,
                dimension_type: 0,
                seed_hash: 42,
                is_debug: false,
                is_flat: false,
                sea_level: 63,
                border: ClientboundInitializeBorderPacket {
                    new_center_x: 0.0,
                    new_center_z: 0.0,
                    old_size: 5.9999968E7,
                    new_size: 5.9999968E7,
                    lerp_time: 0,
                    new_absolute_max_size: 29999984,
                    warning_blocks: 5,
                    warning_time: 15,
                },
                respawn_data: RespawnData { global_pos: GlobalPos { dimension: Identifier::parse("minecraft:overworld").unwrap(), pos: mc_core::pos::BlockPos::ZERO }, yaw: 0.0, pitch: 0.0 },
                raining: None,
            }
        }
        fn update_recipes_packet(&self) -> ClientboundUpdateRecipesPacket {
            ClientboundUpdateRecipesPacket { item_sets: Vec::new(), stonecutter_recipes: Default::default() }
        }
        fn send_commands(&mut self, player: &PlayerHandle, _permissions: LevelBasedPermissionSet) {
            player.connection.send(&ClientboundCommandsPacket { entries: vec![mc_protocol::packets::play::ClientboundCommandsEntry::root(vec![])], root_index: 0 });
            self.events.push("commands");
        }
        fn update_entire_scoreboard(&mut self, _player: &PlayerHandle) {}
        fn invalidate_status(&mut self) {
            self.events.push("invalidate_status");
        }
        fn server_data_packet(&self) -> Option<ClientboundServerDataPacket> {
            Some(ClientboundServerDataPacket { motd: mc_text::Component::literal("A Minecraft Server"), icon_bytes: None })
        }
        fn name_cache_get(&self, _id: &Uuid) -> Option<String> {
            None
        }
        fn name_cache_add(&mut self, _name_and_id: &NameAndId) {}
        fn clock_sync_packet(&self) -> ClientboundSetTimePacket {
            ClientboundSetTimePacket { game_time: 0, clock_updates: vec![] }
        }
        fn ticking_packets(&self) -> (ClientboundTickingStatePacket, ClientboundTickingStepPacket) {
            (ClientboundTickingStatePacket { tick_rate: 20.0, is_frozen: false }, ClientboundTickingStepPacket { tick_steps: 0 })
        }
        fn custom_boss_events_on_player_connect(&mut self, _player: &PlayerHandle) {}
        fn add_new_player(&mut self, player: ServerPlayer) {
            self.player = Some(player);
        }
        fn with_player(&mut self, _id: EntityId, f: &mut dyn FnMut(&mut ServerLevel, &mut ServerPlayer)) {
            let mut player = self.player.take().expect("added before");
            f(&mut self.level, &mut player);
            self.player = Some(player);
        }
    }

    fn fixture_names() -> Vec<String> {
        include_str!("../../../tests/fixtures/vanilla_join_sequence.txt").lines().filter_map(|l| l.split_whitespace().nth(1)).map(str::to_owned).collect()
    }

    /// The packets `placeNewPlayer` sends itself (vanilla's list minus what the chunk tracking, the entity tracker, the
    /// recipe unlock and the first player tick add) are in the order the real server sends them.
    #[test]
    fn join_packet_order_matches_the_real_server() {
        let dir = tempfile::tempdir().unwrap();
        let list = PlayerList::new(
            PlayerListSettings::default(),
            PlayerDataStorage::new(dir.path().join("players/data")),
            dir.path().join("players/stats"),
            dir.path().join("players/advancements"),
            Arc::new(EmptyNotificationService),
        );
        let mut host = RecordingHost { list, level: ServerLevel::detached(Identifier::parse("minecraft:overworld").unwrap()), player: None, events: vec![] };
        let profile = GameProfile { id: mc_core::uuid_util::create_offline_player_uuid("Bot"), name: "Bot".into(), properties: vec![] };
        let player = ServerPlayer::new(5, Identifier::parse("minecraft:overworld").unwrap(), profile.clone(), Default::default());
        let (wire_handle, mut wire) = handle(5, "Bot", "minecraft:overworld");
        let (_tx, inbound) = std::sync::mpsc::channel();
        let params = ListenerParams {
            cookie: CommonListenerCookie { profile, latency: 0, client_information: Default::default(), transferred: false },
            now_ms: 0,
            command_spam_threshold_seconds: 10,
            chat_spam_threshold_seconds: 10,
            memory_connection: false,
            remote_address: None,
            inbound,
            enforce_secure_profile: Arc::new(|| false),
        };
        place_new_player(&mut host, player, wire_handle.connection.clone(), params);
        let sent: Vec<String> =
            wire.ids().into_iter().flatten().map(|id| packet_name(ConnectionState::Play, PacketFlow::Clientbound, id).unwrap().trim_start_matches("minecraft:").to_owned()).collect();

        // vanilla's first packets up to the inventory, minus the second `recipe_book_add` and what other systems send
        let mut expected = Vec::new();
        for name in fixture_names() {
            if name == "set_chunk_cache_center" || name == "set_entity_data" {
                continue;
            }
            expected.push(name.clone());
            if name == "container_set_content" {
                break;
            }
        }
        assert_eq!(sent, expected, "our join sequence differs from the recorded real-server sequence");
        assert_eq!(host.list.get_player_count(), 1);
    }
}
