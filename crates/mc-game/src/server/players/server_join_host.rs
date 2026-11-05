//! [`JoinHost`] over a live server: the server (player list, rules, clocks, caches, status) plus the level the player
//! is placed in. A level that is being joined is taken out of the server map for the duration
//! (`MinecraftServer::with_level`), which is what the `(server, level)` pair expresses.

use super::join::{JoinHost, LevelJoinInfo};
use super::name_and_id::NameAndId;
use super::player_handle::PlayerHandle;
use super::player_list::PlayerList;
use crate::registry::keys;
use crate::registry::typed::level_stem::{DimensionTypeSource, LevelStem};
use crate::server::MinecraftServer;
use crate::server::level::ServerLevel;
use crate::server::permissions::LevelBasedPermissionSet;
use crate::world::entity::{EntityId, ServerPlayer};
use crate::world::level::{DimensionKey, LevelReader};
use crate::world::level::gamerules::GameRules;
use crate::world::level::saveddata::world_border::WorldBorder;
use mc_core::ident::Identifier;
use mc_protocol::packets::play::{
    ClientboundInitializeBorderPacket, ClientboundServerDataPacket, ClientboundSetTimePacket,
    ClientboundTickingStatePacket, ClientboundTickingStepPacket, ClientboundUpdateRecipesPacket, Difficulty, RespawnData as WireRespawnData,
};
use mc_protocol::codec::GlobalPos;
use uuid::Uuid;

/// `Commands.sendCommands(player)`: installed by the command system (`commands`) when it is created, since the command
/// tree lives with it. Until then (no commands) nothing is sent.
pub type SendCommands = fn(&mut MinecraftServer, &PlayerHandle, LevelBasedPermissionSet);
static SEND_COMMANDS: std::sync::OnceLock<SendCommands> = std::sync::OnceLock::new();

/// Installs the command tree sender (`PlayerList.sendPlayerPermissionLevel` / join / `/reload` call it).
pub fn set_send_commands(send: SendCommands) {
    let _ = SEND_COMMANDS.set(send);
}

pub struct ServerJoinHost<'a> {
    pub server: &'a mut MinecraftServer,
    pub level: &'a mut ServerLevel,
}

impl ServerLevel {
    /// `getWorldBorder()`: the level's border saved data, its initial settings applied on first use.
    pub fn get_world_border(&mut self) -> &mut WorldBorder {
        let game_time = self.level_data.get_game_time();
        let border = self.get_data_storage().compute_if_absent(&WorldBorder::type_());
        border.apply_initial_settings(game_time);
        border
    }
}

impl MinecraftServer {
    /// `level.dimensionTypeRegistration()`'s protocol id (`0` for an inline dimension type, which has no registry id).
    pub fn dimension_type_id(&self, dimension: &DimensionKey) -> i32 {
        let access = self.registries.composite_access();
        let Some(stems) = access.registry(keys::LEVEL_STEM) else { return 0 };
        let Some(id) = stems.id_of(dimension) else { return 0 };
        let Some(stem) = stems.entry(id).and_then(|e| e.value.clone()).and_then(|v| v.downcast::<LevelStem>().ok()) else { return 0 };
        match &stem.dimension_type {
            DimensionTypeSource::Reference(type_id) => access.registry(keys::DIMENSION_TYPE).and_then(|r| r.id_of(type_id)).map_or(0, |i| i as i32),
            DimensionTypeSource::Inline(_) => 0,
        }
    }
}

/// `new ClientboundInitializeBorderPacket(border)`.
pub fn initialize_border_packet(border: &WorldBorder) -> ClientboundInitializeBorderPacket {
    ClientboundInitializeBorderPacket {
        new_center_x: border.get_center_x(),
        new_center_z: border.get_center_z(),
        old_size: border.get_size(),
        new_size: border.get_lerp_target(),
        lerp_time: border.get_lerp_time(),
        new_absolute_max_size: border.get_absolute_max_size(),
        warning_blocks: border.get_warning_blocks(),
        warning_time: border.get_warning_time(),
    }
}

/// The level facts every join/respawn packet needs, read from the server and the level.
pub fn level_join_info(server: &mut MinecraftServer, level: &mut ServerLevel) -> LevelJoinInfo {
    let dimension = level.dimension.clone();
    // (the level data of the overworld locks the same world data: read it before taking the lock here)
    let respawn = level.level_data.get_respawn_data();
    let (hardcore, difficulty, difficulty_locked, is_flat) = {
        let data = server.world_data.lock();
        let settings = &data.settings.difficulty_settings;
        (settings.hardcore, settings.difficulty, settings.locked, data.is_flat_world())
    };
    let rules = server.game_rules_ref();
    let (immediate_respawn, reduced_debug_info, limited_crafting) =
        (rules.get(GameRules::IMMEDIATE_RESPAWN), rules.get(GameRules::REDUCED_DEBUG_INFO), rules.get(GameRules::LIMITED_CRAFTING));
    let border = initialize_border_packet(level.get_world_border());
    let raining = level.is_raining().then(|| (level.get_rain_level(1.0), level.get_thunder_level(1.0)));
    LevelJoinInfo {
        hardcore,
        difficulty: Difficulty::by_id(difficulty.get_id()),
        difficulty_locked,
        immediate_respawn,
        reduced_debug_info,
        limited_crafting,
        dimension_type: server.dimension_type_id(&dimension),
        seed_hash: level.tick_state.biome_zoom_seed,
        is_debug: level.is_debug,
        is_flat,
        sea_level: level.get_sea_level(),
        border,
        respawn_data: WireRespawnData {
            global_pos: GlobalPos { dimension: respawn.dimension, pos: respawn.pos },
            yaw: respawn.yaw,
            pitch: respawn.pitch,
        },
        raining,
    }
}

impl JoinHost for ServerJoinHost<'_> {
    fn list(&mut self) -> &mut PlayerList {
        &mut self.server.player_list
    }

    fn level_keys(&self) -> Vec<DimensionKey> {
        let mut keys = self.server.level_keys();
        // the level being joined is taken out of the server map but still is one of its levels
        if !keys.contains(&self.level.dimension) {
            keys.push(self.level.dimension.clone());
        }
        keys
    }

    fn level_join_info(&mut self) -> LevelJoinInfo {
        level_join_info(self.server, self.level)
    }

    fn update_recipes_packet(&self) -> ClientboundUpdateRecipesPacket {
        crate::world::item::crafting::update_recipes_packet(&self.level.recipe_access())
    }

    fn recipe_manager(&self) -> crate::world::item::crafting::RecipeManager {
        self.level.recipe_access()
    }

    fn send_commands(&mut self, player: &PlayerHandle, permissions: LevelBasedPermissionSet) {
        if let Some(send) = SEND_COMMANDS.get() {
            send(self.server, player, permissions);
        }
    }

    fn update_entire_scoreboard(&mut self, player: &PlayerHandle) {
        self.server.update_entire_scoreboard(player);
    }

    fn invalidate_status(&mut self) {
        self.server.invalidate_status();
    }

    fn server_data_packet(&self) -> Option<ClientboundServerDataPacket> {
        let status = self.server.shared.status()?;
        Some(ClientboundServerDataPacket { motd: status.description.clone(), icon_bytes: status.favicon.clone() })
    }

    fn name_cache_get(&self, id: &Uuid) -> Option<String> {
        use super::UserNameToIdResolver;
        self.server.name_to_id_cache.get_by_id(*id).map(|n| n.name)
    }

    fn name_cache_add(&mut self, name_and_id: &NameAndId) {
        use super::UserNameToIdResolver;
        self.server.name_to_id_cache.add(name_and_id.clone());
    }

    fn clock_sync_packet(&self) -> ClientboundSetTimePacket {
        self.server.create_clock_full_sync_packet()
    }

    fn ticking_packets(&self) -> (ClientboundTickingStatePacket, ClientboundTickingStepPacket) {
        let (state, step) = self.server.tick_rate_manager.update_joining_player();
        (
            ClientboundTickingStatePacket { tick_rate: state.tick_rate, is_frozen: state.is_frozen },
            ClientboundTickingStepPacket { tick_steps: step.tick_steps },
        )
    }

    fn custom_boss_events_on_player_connect(&mut self, player: &PlayerHandle) {
        self.server.custom_boss_events().on_player_connect(player);
    }

    fn add_new_player(&mut self, player: ServerPlayer) {
        self.level.add_player(Box::new(player));
    }

    fn with_player(&mut self, id: EntityId, f: &mut dyn FnMut(&mut ServerLevel, &mut ServerPlayer)) {
        self.level.with_player(id, |level, player| f(level, player));
    }
}
