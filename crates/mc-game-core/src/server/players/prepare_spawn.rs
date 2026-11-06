//! `PrepareSpawnTask`'s world side: load the player data, pick the spawn level and position (`PlayerSpawnFinder`), keep
//! the spawn chunks loaded with a `PLAYER_SPAWN` ticket, then create the `ServerPlayer` and `placeNewPlayer` it.
//! Everything runs on the game thread, polled from the connection's configuration tick (the chunk system advances
//! with the server tick, so `tick` never blocks).

use super::join::{ListenerParams, place_new_player};
use super::name_and_id::NameAndId;
use super::server_join_host::ServerJoinHost;
use crate::server::MinecraftServer;
use crate::server::level::player_spawn_finder::{
    FindSpawn, PlayerSpawnFinder, Step, fixup_spawn_height,
};
use crate::server::level::{ServerLevel, TicketType};
use crate::server::network::config::PrepareSpawn;
use crate::server::network::configuration_task::TaskError;
use crate::server::network::hooks::ServerHandle;
use crate::server::network::pending_player::PendingPlayer;
use crate::server::network::server_network_hooks::PrepareSpawnFactory;
use crate::server::world_dimensions::overworld_key;
use crate::world::entity::ServerPlayer;
use crate::world::entity::entity::Entity;
use crate::world::level::gamerules::GameRules;
use crate::world::level::{DimensionKey, LevelReader};
use mc_core::phys::{Vec2, Vec3};
use mc_core::pos::{BlockPos, ChunkPos};
use mc_data::enums::GameType;
use mc_nbt::CompoundTag;
use mc_protocol::codec::GameProfile;
use std::sync::Arc;

/// `PrepareSpawnTask.PREPARE_CHUNK_RADIUS`.
const PREPARE_CHUNK_RADIUS: i32 = 3;

enum Spawn {
    /// The position came from the player file.
    Known(Vec3),
    /// `PlayerSpawnFinder.findSpawn` is running; `waiting` is the candidate whose chunk is being loaded.
    Searching {
        finder: PlayerSpawnFinder,
        waiting: Option<Waiting>,
    },
    /// `GameType.ADVENTURE`: the suggestion with a fixed-up height once its chunk is loaded.
    Adventure(BlockPos),
}

enum Waiting {
    Candidate(crate::server::level::player_spawn_finder::Candidate),
    Fallback(BlockPos),
}

struct Preparing {
    level: DimensionKey,
    spawn: Spawn,
    angle: Vec2,
}

struct Ready {
    level: DimensionKey,
    position: Vec3,
    angle: Vec2,
}

enum State {
    Closed,
    Preparing(Preparing),
    /// The spawn position is known and its chunks are loading (`chunkLoadFuture`).
    Loading(Ready),
    Ready(Ready),
}

pub struct PrepareSpawnTask {
    profile: GameProfile,
    name_and_id: NameAndId,
    /// The player file (`loadPlayerData`), `None` for a first-time player.
    saved: Option<CompoundTag>,
    state: State,
}

/// The factory `PlayerListHooks::set_prepare_spawn` takes.
pub fn factory() -> PrepareSpawnFactory {
    Box::new(|_server: &mut MinecraftServer, profile: &GameProfile| {
        Box::new(PrepareSpawnTask::new(profile.clone())) as Box<dyn PrepareSpawn>
    })
}

fn real(server: &mut dyn ServerHandle) -> Result<&mut MinecraftServer, TaskError> {
    server.minecraft_server().ok_or_else(|| "no server".into())
}

fn chunk_of(position: Vec3) -> ChunkPos {
    let pos = BlockPos::containing(position.x, position.y, position.z);
    ChunkPos::new(pos.x() >> 4, pos.z() >> 4)
}

impl PrepareSpawnTask {
    pub fn new(profile: GameProfile) -> Self {
        let name_and_id = NameAndId::new(profile.id, profile.name.clone());
        Self {
            profile,
            name_and_id,
            saved: None,
            state: State::Closed,
        }
    }

    fn start_world(&mut self, server: &mut MinecraftServer) {
        let loaded = server
            .player_list
            .load_player_data_with_position(&self.name_and_id);
        self.saved = loaded.data;
        let respawn = server.world_data.lock().respawn_data.clone();
        let level = loaded
            .position
            .dimension
            .filter(|d| server.level(d).is_some())
            .or_else(|| {
                server
                    .level(&respawn.dimension)
                    .map(|_| respawn.dimension.clone())
            })
            .unwrap_or_else(overworld_key);
        let spawn = match loaded.position.position {
            Some(position) => Spawn::Known(position),
            None => {
                let adventure = server.world_data.lock().settings.game_type
                    == mc_world::level_storage::GameType::Adventure;
                let radius = server.game_rules_ref().get(GameRules::RESPAWN_RADIUS);
                let border = server
                    .level_mut(&level)
                    .map_or(i32::MAX, |l| distance_to_border(l, respawn.pos));
                match PlayerSpawnFinder::find_spawn(adventure, radius, border, respawn.pos) {
                    FindSpawn::Adventure(pos) => Spawn::Adventure(pos),
                    FindSpawn::Search(finder) => Spawn::Searching {
                        finder,
                        waiting: None,
                    },
                }
            }
        };
        let angle = loaded
            .position
            .rotation
            .unwrap_or_else(|| Vec2::new(respawn.yaw, respawn.pitch));
        self.state = State::Preparing(Preparing {
            level,
            spawn,
            angle,
        });
    }

    /// Advances the search; `Some(position)` once found.
    fn find_position(server: &mut MinecraftServer, p: &mut Preparing) -> Option<Vec3> {
        let level = server.level_mut(&p.level)?;
        match &mut p.spawn {
            Spawn::Known(position) => Some(*position),
            Spawn::Adventure(pos) => {
                let chunk = ChunkPos::new(pos.x() >> 4, pos.z() >> 4);
                level.add_ticket_with_radius(TicketType::SPAWN_SEARCH, chunk, 0);
                level
                    .chunk_now(chunk.x, chunk.z)
                    .is_some()
                    .then(|| fixup_spawn_height(level, *pos))
            }
            Spawn::Searching { finder, waiting } => loop {
                let current = waiting
                    .take()
                    .unwrap_or_else(|| match finder.next_candidate() {
                        Step::Candidate(c) => Waiting::Candidate(c),
                        Step::Fallback(pos) => Waiting::Fallback(pos),
                    });
                let chunk = match &current {
                    Waiting::Candidate(c) => c.chunk_pos(),
                    Waiting::Fallback(pos) => ChunkPos::new(pos.x() >> 4, pos.z() >> 4),
                };
                level.add_ticket_with_radius(TicketType::SPAWN_SEARCH, chunk, 0);
                // `getChunkFuture(...FULL)` is done: the chunk object exists (a holder with a low enough ticket level
                // only says the chunk is wanted, not that it is generated).
                if level.chunk_now(chunk.x, chunk.z).is_none() {
                    *waiting = Some(current);
                    return None;
                }
                match current {
                    Waiting::Candidate(c) => {
                        if let Some(found) = finder.check(&*level, &c) {
                            return Some(found);
                        }
                    }
                    Waiting::Fallback(pos) => return Some(fixup_spawn_height(&*level, pos)),
                }
            },
        }
    }

    fn spawn_chunks_loaded(level: &ServerLevel, center: ChunkPos) -> bool {
        (-PREPARE_CHUNK_RADIUS..=PREPARE_CHUNK_RADIUS).all(|dx| {
            (-PREPARE_CHUNK_RADIUS..=PREPARE_CHUNK_RADIUS)
                .all(|dz| level.chunk_now(center.x + dx, center.z + dz).is_some())
        })
    }
}

/// `Mth.floor(worldBorder.getDistanceToBorder(x, z))` at the suggestion.
fn distance_to_border(level: &mut ServerLevel, pos: BlockPos) -> i32 {
    let border = level.get_world_border();
    mc_core::math::mth::floor_f64(
        border.get_distance_to_border(f64::from(pos.x()), f64::from(pos.z())),
    )
}

/// `GameType.byId` (unknown ids are survival).
fn game_type_by_id(id: i32) -> GameType {
    match id {
        1 => GameType::Creative,
        2 => GameType::Adventure,
        3 => GameType::Spectator,
        _ => GameType::Survival,
    }
}

/// `ServerPlayer.calculateGameModeForNewPlayer(loaded)`.
fn calculate_game_mode_for_new_player(
    server: &MinecraftServer,
    loaded: Option<GameType>,
) -> GameType {
    let to_game_type = |g: mc_world::level_storage::GameType| game_type_by_id(g.get_id());
    let default = to_game_type(server.world_data.lock().settings.game_type);
    if server
        .properties()
        .force_game_mode
        .get()
        .then_some(default)
        .is_some()
    {
        // `getForcedGameType()` is the world's game type when `force-gamemode` is on
        return default;
    }
    loaded.unwrap_or(default)
}

impl PrepareSpawn for PrepareSpawnTask {
    fn start(&mut self, server: &mut dyn ServerHandle) -> Result<(), TaskError> {
        let server = real(server)?;
        self.start_world(server);
        Ok(())
    }

    fn tick(&mut self, server: &mut dyn ServerHandle) -> Result<bool, TaskError> {
        let server = real(server)?;
        loop {
            match std::mem::replace(&mut self.state, State::Closed) {
                State::Closed => return Ok(false),
                State::Preparing(mut p) => match Self::find_position(server, &mut p) {
                    None => {
                        self.state = State::Preparing(p);
                        return Ok(false);
                    }
                    Some(position) => {
                        self.state = State::Loading(Ready {
                            level: p.level,
                            position,
                            angle: p.angle,
                        });
                    }
                },
                State::Loading(ready) => {
                    let center = chunk_of(ready.position);
                    let Some(level) = server.level_mut(&ready.level) else {
                        return Err("the spawn level is gone".into());
                    };
                    level.add_ticket_with_radius(
                        TicketType::PLAYER_SPAWN,
                        center,
                        PREPARE_CHUNK_RADIUS,
                    );
                    if Self::spawn_chunks_loaded(level, center) {
                        self.state = State::Ready(ready);
                        return Ok(true);
                    }
                    self.state = State::Loading(ready);
                    return Ok(false);
                }
                State::Ready(ready) => {
                    self.state = State::Ready(ready);
                    return Ok(true);
                }
            }
        }
    }

    fn keep_alive(&mut self, server: &mut dyn ServerHandle) {
        let (State::Ready(ready) | State::Loading(ready)) = &self.state else {
            return;
        };
        if let Some(level) = server
            .minecraft_server()
            .and_then(|s| s.level_mut(&ready.level))
        {
            level.add_ticket_with_radius(
                TicketType::PLAYER_SPAWN,
                chunk_of(ready.position),
                PREPARE_CHUNK_RADIUS,
            );
        }
    }

    fn close(&mut self, _server: &mut dyn ServerHandle) {
        self.state = State::Closed;
    }

    fn spawn_player(
        &mut self,
        server: &mut dyn ServerHandle,
        pending: PendingPlayer,
    ) -> Result<(), TaskError> {
        let server = real(server)?;
        let State::Ready(ready) = std::mem::replace(&mut self.state, State::Closed) else {
            return Err("Player spawn was not ready".into());
        };
        let saved = self.saved.take();
        let profile = self.profile.clone();
        let result = server.with_level(&ready.level, |server, level| -> Result<(), TaskError> {
            let mut player = ServerPlayer::new(
                level.next_entity_id(),
                ready.level.clone(),
                profile,
                pending.cookie.client_information.clone(),
            );
            let loaded_mode = saved
                .as_ref()
                .and_then(|t| t.get("playerGameType"))
                .and_then(|t| t.as_number())
                .map(|n| game_type_by_id(n.int_value()));
            let previous = player.game_mode.previous_game_mode_for_player;
            if let Some(tag) = &saved {
                player.load(tag)?;
                player
                    .recipe_book
                    .retain_known(&server.shared.recipe_manager());
            }
            let previous = if saved.is_some() {
                player.game_mode.previous_game_mode_for_player
            } else {
                previous
            };
            let mode = calculate_game_mode_for_new_player(server, loaded_mode);
            player.set_game_mode_for_player(mode, previous);
            player.snap_to(
                ready.position.x,
                ready.position.y,
                ready.position.z,
                ready.angle.x,
                ready.angle.y,
            );
            let enforce = server.player_list.settings.enforce_secure_profile;
            let params = ListenerParams {
                remote_address: pending.sender.remote_address(),
                cookie: pending.cookie,
                now_ms: crate::server::network::connection::get_millis(),
                command_spam_threshold_seconds: server.get_command_spam_threshold_seconds(),
                chat_spam_threshold_seconds: server.get_chat_spam_threshold_seconds(),
                memory_connection: false,
                inbound: pending.inbound,
                enforce_secure_profile: Arc::new(move || enforce),
            };
            let mut host = ServerJoinHost { server, level };
            place_new_player(&mut host, player, pending.sender, params);
            Ok(())
        });
        result.unwrap_or_else(|| Err("the spawn level is unavailable".into()))
    }
}
