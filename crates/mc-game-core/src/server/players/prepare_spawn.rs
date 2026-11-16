//! `PrepareSpawnTask`'s world side: load the player data, pick the spawn level and position (`PlayerSpawnFinder`), keep
//! the spawn chunks loaded with a `PLAYER_SPAWN` ticket, then create the `ServerPlayer` and `placeNewPlayer` it.
//! Everything runs on the game thread, polled from the connection's configuration tick (the chunk system advances
//! with the server tick, so `tick` never blocks).

use super::ender_pearl_ticket::load_and_spawn_ender_pearls;
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
use crate::world::entity::entity::Entity;
use crate::world::entity::factory::{self, EntitySpawnRequest};
use crate::world::entity::{EntityId, ServerPlayer};
use crate::world::level::DimensionKey;
use crate::world::level::gamerules::GameRules;
use mc_core::phys::{Vec2, Vec3};
use mc_core::pos::{BlockPos, ChunkPos};
use mc_data::enums::{EntitySpawnReason, GameType};
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
            if server.is_demo {
                player.use_demo_mode();
            }
            let loaded_mode = saved
                .as_ref()
                .and_then(|t| t.get("playerGameType"))
                .and_then(|t| t.as_number())
                .map(|n| game_type_by_id(n.int_value()));
            let previous = player.game_mode.previous_game_mode_for_player;
            if let Some(tag) = &saved {
                player.load(level, tag)?;
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
            let settings = server.player_list.settings.clone();
            let params = ListenerParams {
                remote_address: pending.sender.remote_address(),
                cookie: pending.cookie,
                now_ms: crate::server::network::connection::get_millis(),
                command_spam_threshold_seconds: server.get_command_spam_threshold_seconds(),
                chat_spam_threshold_seconds: server.get_chat_spam_threshold_seconds(),
                memory_connection: false,
                inbound: pending.inbound,
                enforce_secure_profile: Arc::new(move || settings.enforces_secure_profile()),
            };
            let mut host = ServerJoinHost { server, level };
            let handle = place_new_player(&mut host, player, pending.sender, params);
            load_and_spawn_ender_pearls(server, level, handle.id);
            load_and_spawn_parent_vehicle(level, handle.id);
            Ok(())
        });
        result.unwrap_or_else(|| Err("the spawn level is unavailable".into()))
    }
}

/// `ServerPlayer.loadAndSpawnParentVehicle(input)`: recreates the root vehicle (with its passengers) saved in the player
/// data and seats `player` on the entity named by `Attach`; the vehicle is discarded when that entity is missing.
pub fn load_and_spawn_parent_vehicle(level: &mut ServerLevel, player: EntityId) {
    let Some(root) = level
        .get_entity_mut(player)
        .and_then(|e| e.as_any_mut().downcast_mut::<ServerPlayer>())
        .and_then(ServerPlayer::take_saved_root_vehicle)
    else {
        return;
    };
    let Some(tag) = root.get_compound("Entity") else {
        return;
    };
    let request = EntitySpawnRequest {
        reason: EntitySpawnReason::Load,
        ignore_checks: false,
    };
    let mut loaded = Vec::new();
    factory::load_entity_recursive(tag, level, request, &mut loaded);
    // `loadEntityRecursive` yields nothing when the root cannot be created
    let members: Vec<(EntityId, uuid::Uuid)> = loaded.iter().map(|e| (e.id(), e.uuid())).collect();
    if members.is_empty() || !level.try_add_fresh_entity_with_passengers(loaded) {
        return;
    }
    let attach_to = root
        .get_int_array("Attach")
        .and_then(|ints| <[i32; 4]>::try_from(ints).ok())
        .map(mc_core::uuid_util::uuid_from_int_array);
    if let Some(&(target, _)) = members.iter().find(|(_, uuid)| Some(*uuid) == attach_to) {
        level.with_entity(player, |level, rider| {
            level.start_riding(rider, target, true, false)
        });
    }
    if !level
        .get_entity(player)
        .is_some_and(|p| p.base().is_passenger())
    {
        log::warn!("Couldn't reattach entity to player");
        // `discard()` of the root ejects its passengers, so only the vehicle goes
        level.with_entity(members[0].0, |level, vehicle| {
            vehicle.discard_in_level(level)
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mc_nbt::snbt;

    const BOAT: &str = "{id:\"minecraft:oak_boat\",UUID:[I;7,7,7,7],Pos:[0.5d,64.0d,0.5d],Passengers:[{id:\"minecraft:zombie\",UUID:[I;8,8,8,8],Pos:[0.5d,64.0d,0.5d]}]}";

    const BOAT_UUID: u128 = 0x0000_0007_0000_0007_0000_0007_0000_0007;
    const ZOMBIE_UUID: u128 = 0x0000_0008_0000_0008_0000_0008_0000_0008;

    /// A level with a player that has `RootVehicle` data attaching it to the entity `attach`.
    fn rejoin(attach: [i32; 4]) -> (ServerLevel, EntityId) {
        let mut level = ServerLevel::detached(DimensionKey::parse("minecraft:overworld").unwrap());
        level.entity_manager.update_chunk_status(
            ChunkPos::new(0, 0),
            crate::world::level::entity::Visibility::Ticking,
        );
        let player = ServerPlayer::new(
            1,
            DimensionKey::parse("minecraft:overworld").unwrap(),
            GameProfile {
                id: uuid::Uuid::from_u128(1),
                name: "A".into(),
                properties: vec![],
            },
            Default::default(),
        );
        let mut root = CompoundTag::new();
        root.put_int_array("Attach", attach.to_vec());
        root.put("Entity", snbt::parse_compound(BOAT).unwrap());
        let mut player = player;
        player.saved_root_vehicle = Some(root);
        level.add_player(Box::new(player));
        load_and_spawn_parent_vehicle(&mut level, 1);
        (level, 1)
    }

    fn uuid_ints(uuid: u128) -> [i32; 4] {
        mc_core::uuid_util::uuid_to_int_array(&uuid::Uuid::from_u128(uuid))
    }

    fn entity_of(level: &ServerLevel, uuid: u128) -> Option<(EntityId, Option<EntityId>)> {
        let id = level
            .entity_manager
            .id_by_uuid(uuid::Uuid::from_u128(uuid))?;
        Some((id, level.get_entity(id)?.base().vehicle))
    }

    #[test]
    fn the_player_rides_the_attached_root_vehicle_again() {
        let (level, player) = rejoin(uuid_ints(BOAT_UUID));
        let (boat, _) = entity_of(&level, BOAT_UUID).expect("boat");
        assert_eq!(level.get_entity(player).unwrap().base().vehicle, Some(boat));
        assert!(entity_of(&level, ZOMBIE_UUID).is_some());
    }

    #[test]
    fn the_player_rides_an_attached_passenger() {
        let (level, player) = rejoin(uuid_ints(ZOMBIE_UUID));
        let (zombie, _) = entity_of(&level, ZOMBIE_UUID).expect("zombie");
        assert_eq!(
            level.get_entity(player).unwrap().base().vehicle,
            Some(zombie)
        );
    }

    #[test]
    fn a_missing_attach_target_discards_the_vehicle() {
        let (level, player) = rejoin(uuid_ints(99));
        assert!(!level.get_entity(player).unwrap().base().is_passenger());
        assert!(entity_of(&level, BOAT_UUID).is_none());
    }
}
