//! `PlayerList.respawn(player, keepAllPlayerData, reason)` with `ServerPlayer.findRespawnPositionAndUseSpawnBlock`,
//! `restoreFrom` and the packet sequence of a respawn. The player must be in its level (not taken out): callers that
//! are inside the player's own packet handling queue it with [`queue_respawn`], which runs it right after.

use super::join::{abilities_packet, create_common_spawn_info, send_active_player_effects, send_level_info, send_post_effects};
use super::server_join_host::{ServerJoinHost, level_join_info};
use crate::server::MinecraftServer;
use crate::server::level::ServerLevel;
use crate::server::level::player_spawn_finder::{FindSpawn, PlayerSpawnFinder, Step, fixup_spawn_height};
use crate::world::entity::entity::Entity;
use crate::world::entity::player::food_data::FoodData;
use crate::world::entity::player::inventory::Inventory;
use crate::world::entity::respawn::{RespawnConfig, RespawnPosAngle};
use crate::world::entity::{EntityId, RemovalReason, ServerPlayer};
use crate::world::level::block::behaviour;
use crate::world::level::gamerules::GameRules;
use crate::world::level::{BlockGetter, DimensionKey, LevelAccessor};
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_protocol::packets::play::{
    ClientboundChangeDifficultyPacket, ClientboundGameEventPacket, ClientboundRespawnPacket, ClientboundSetDefaultSpawnPositionPacket,
    ClientboundSetExperiencePacket, ClientboundSoundPacket, SoundEventHolder, SoundSource,
};

/// `TeleportTransition` as `findRespawnPositionAndUseSpawnBlock` returns it.
struct RespawnTransition {
    level: DimensionKey,
    position: Vec3,
    y_rot: f32,
    x_rot: f32,
    missing_respawn_block: bool,
}

/// `PlayerSpawnFinder.findSpawn` + `managedBlock`: the chunks are loaded synchronously.
fn adjust_spawn_location(server: &mut MinecraftServer, level: &mut ServerLevel, suggestion: BlockPos) -> Vec3 {
    let adventure = server.world_data.lock().settings.game_type == mc_world::level_storage::GameType::Adventure;
    let radius = server.game_rules_ref().get(GameRules::RESPAWN_RADIUS);
    let border = mc_core::math::mth::floor_f64(level.get_world_border().get_distance_to_border(f64::from(suggestion.x()), f64::from(suggestion.z())));
    match PlayerSpawnFinder::find_spawn(adventure, radius, border, suggestion) {
        FindSpawn::Adventure(pos) => {
            level.chunk_blocking_mut(pos.x() >> 4, pos.z() >> 4);
            fixup_spawn_height(&*level, pos)
        }
        FindSpawn::Search(mut finder) => loop {
            match finder.next_candidate() {
                Step::Candidate(c) => {
                    level.chunk_blocking_mut(c.x >> 4, c.z >> 4);
                    if let Some(found) = finder.check(&*level, &c) {
                        return found;
                    }
                }
                Step::Fallback(pos) => {
                    level.chunk_blocking_mut(pos.x() >> 4, pos.z() >> 4);
                    return fixup_spawn_height(&*level, pos);
                }
            }
        },
    }
}

/// `ServerPlayer.findRespawnAndUseSpawnBlock(level, respawnConfig, consumeSpawnBlock)`.
fn find_respawn_and_use_spawn_block(level: &mut ServerLevel, config: &RespawnConfig, consume_spawn_block: bool) -> Option<RespawnPosAngle> {
    let data = &config.respawn_data;
    let (pos, yaw, pitch, forced) = (data.pos, data.yaw, data.pitch, config.forced);
    let state = level.get_block_state(pos);
    let block = state.block();
    if let Some(stand_up) = behaviour(block).find_respawn_position(level, state, pos, forced, consume_spawn_block, yaw) {
        return stand_up.map(|p| RespawnPosAngle::of(p, pos, 0.0));
    }
    if !forced {
        return None;
    }
    let free_bottom = behaviour(block).is_possible_to_respawn_in_this(state);
    let top = level.get_block_state(pos.above());
    let free_top = behaviour(top.block()).is_possible_to_respawn_in_this(top);
    (free_bottom && free_top).then(|| RespawnPosAngle {
        position: Vec3::new(f64::from(pos.x()) + 0.5, f64::from(pos.y()) + 0.1, f64::from(pos.z()) + 0.5),
        yaw,
        pitch,
    })
}

/// `ServerPlayer.findRespawnPositionAndUseSpawnBlock(consume, DO_NOTHING)`; `level` is the player's current level.
fn find_respawn_transition(server: &mut MinecraftServer, level: &mut ServerLevel, config: Option<&RespawnConfig>, consume: bool) -> RespawnTransition {
    let respawn_dimension = RespawnConfig::dimension_or_default(config);
    let found = match config {
        Some(config) => {
            let found = if respawn_dimension == level.dimension {
                Some(find_respawn_and_use_spawn_block(level, config, consume))
            } else {
                server.level_mut(&respawn_dimension).map(|l| find_respawn_and_use_spawn_block(l, config, consume))
            };
            found
        }
        None => None,
    };
    match found {
        Some(Some(angle)) => RespawnTransition { level: respawn_dimension, position: angle.position, y_rot: angle.yaw, x_rot: angle.pitch, missing_respawn_block: false },
        other => {
            // `TeleportTransition.createDefault` / `missingRespawnBlock`: the world spawn of the respawn dimension
            let missing = matches!(other, Some(None));
            let dimension = server.find_respawn_dimension();
            let respawn = server.get_respawn_data();
            let position = if dimension == level.dimension {
                adjust_spawn_location(server, level, respawn.pos)
            } else {
                match server.with_level(&dimension, |server, other| adjust_spawn_location(server, other, respawn.pos)) {
                    Some(position) => position,
                    None => Vec3::new(f64::from(respawn.pos.x()) + 0.5, f64::from(respawn.pos.y()), f64::from(respawn.pos.z()) + 0.5),
                }
            };
            RespawnTransition { level: dimension, position, y_rot: respawn.yaw, x_rot: respawn.pitch, missing_respawn_block: missing }
        }
    }
}

/// `restoreFrom(oldPlayer, restoreAll)` + the field copies of `PlayerList.respawn` (`oldPlayer` is consumed).
fn restore_from(new: &mut ServerPlayer, mut old: ServerPlayer, restore_all: bool, keep_inventory: bool) {
    new.warden_spawn_tracker = old.warden_spawn_tracker;
    new.set_game_mode_for_player(old.game_mode.game_mode_for_player, old.game_mode.previous_game_mode_for_player);
    let transfer_inventory = |new: &mut ServerPlayer, old: &mut ServerPlayer| {
        new.player.inventory = std::mem::replace(&mut old.player.inventory, Inventory::new());
        new.player.experience_level = old.player.experience_level;
        new.player.total_experience = old.player.total_experience;
        new.player.experience_progress = old.player.experience_progress;
        let score = old.player.get_score();
        new.player.set_score(score);
    };
    if restore_all {
        let health = old.player.living.get_health();
        new.player.living.set_health(health);
        new.player.food_data = std::mem::replace(&mut old.player.food_data, FoodData::default());
        new.player.living.active_effects = std::mem::take(&mut old.player.living.active_effects);
        transfer_inventory(new, &mut old);
    } else {
        let max = new.player.living.get_max_health();
        new.player.living.set_health(max);
        if keep_inventory || crate::world::entity::Entity::is_spectator(&old) {
            transfer_inventory(new, &mut old);
        }
    }
    new.player.enchantment_seed = old.player.enchantment_seed;
    new.player.ender_chest = std::mem::replace(&mut old.player.ender_chest, crate::world::container::SimpleContainer::new(27));
    let customisation = old.player.living.entity.entity_data.get(crate::world::entity::player::DATA_PLAYER_MODE_CUSTOMISATION);
    new.player.living.entity.entity_data.set(crate::world::entity::player::DATA_PLAYER_MODE_CUSTOMISATION, customisation);
    new.last_sent_exp = -1;
    new.last_sent_health = -1.0;
    new.last_sent_food = -1;
    new.recipe_book = old.recipe_book.take();
    new.seen_credits = old.seen_credits;
    new.entered_nether_position = old.entered_nether_position;
    new.current_explosion_impact_pos = old.current_explosion_impact_pos;
    new.shoulder_entity_left = std::mem::take(&mut old.shoulder_entity_left);
    new.shoulder_entity_right = std::mem::take(&mut old.shoulder_entity_right);
    new.player.last_death_location = old.player.last_death_location.take();
}

/// `PlayerList.respawn`: replaces the player `id` of `old_dimension` by a fresh `ServerPlayer` placed at its respawn
/// position; `after` runs on the new player (the listener's `resetPosition()` etc.). Returns the new player's id.
pub fn respawn(
    server: &mut MinecraftServer,
    old_dimension: &DimensionKey,
    id: EntityId,
    keep_all_player_data: bool,
    reason: RemovalReason,
    after: impl FnOnce(&mut MinecraftServer, &mut ServerPlayer),
) -> Option<EntityId> {
    // 1. take the old player's state out of its level and remove it (`removePlayerImmediately`)
    let keep_inventory = server.game_rules_ref().get(GameRules::KEEP_INVENTORY);
    let (mut old, transition) = server
        .with_level(old_dimension, |server, level| {
            let profile = level.get_entity(id).and_then(|e| e.as_any().downcast_ref::<ServerPlayer>()).map(|p| p.player.game_profile.clone())?;
            let config = level.get_entity(id).and_then(|e| e.as_any().downcast_ref::<ServerPlayer>()).and_then(|p| p.respawn_config.clone());
            let transition = find_respawn_transition(server, level, config.as_ref(), !keep_all_player_data);
            let old = level.with_player(id, |_, player| {
                let mut dummy = ServerPlayer::new(id, player.dimension.clone(), profile, Default::default());
                dummy.remove(reason);
                std::mem::replace(player, dummy)
            })?;
            Some((old, transition))
        })
        .flatten()?;
    let handle = old.handle.clone()?;
    server.player_list.remove_from_players(&handle);

    // 2. the new player
    let client_information = old.client_information.clone();
    let mut new = ServerPlayer::new(id, transition.level.clone(), old.player.game_profile.clone(), client_information);
    new.connection = old.connection.take();
    new.listener = old.listener.take();
    new.handle = Some(handle.clone());
    new.player.set_main_arm(old.player.get_main_arm());
    if !transition.missing_respawn_block {
        new.respawn_config = old.respawn_config.clone();
    }
    for tag in old.base().tags.iter_java_order() {
        new.base_mut().add_tag(tag);
    }
    new.player.post_effects = old.player.post_effects.clone();
    let old_respawn_config = old.respawn_config.clone();
    restore_from(&mut new, old, keep_all_player_data, keep_inventory);
    new.snap_to(transition.position.x, transition.position.y, transition.position.z, transition.y_rot, transition.x_rot);
    new.dimension = transition.level.clone();
    new.sync_handle();
    if transition.missing_respawn_block {
        new.send(&ClientboundGameEventPacket { event: ClientboundGameEventPacket::NO_RESPAWN_BLOCK_AVAILABLE, param: 0.0 });
    }

    // 3. the packets and the placement in the new level
    let new_id = new.base_id();
    let dimension = transition.level.clone();
    server.with_level(&dimension, |server, level| {
        let info = level_join_info(server, level);
        new.send(&ClientboundRespawnPacket {
            common_player_spawn_info: create_common_spawn_info(&new, &info, &dimension),
            data_to_keep: i8::from(keep_all_player_data),
        });
        let position = new.position();
        let (y_rot, x_rot) = (new.player.living.entity.y_rot(), new.player.living.entity.x_rot());
        new.connection_teleport(position.x, position.y, position.z, y_rot, x_rot);
        new.send(&ClientboundSetDefaultSpawnPositionPacket { respawn_data: info.respawn_data.clone() });
        new.send(&ClientboundChangeDifficultyPacket { difficulty: info.difficulty, locked: info.difficulty_locked });
        new.send(&ClientboundSetExperiencePacket {
            experience_progress: new.player.experience_progress,
            experience_level: new.player.experience_level,
            total_experience: new.player.total_experience,
        });
        send_active_player_effects(&new);
        send_post_effects(&mut new);
        {
            let host = ServerJoinHost { server: &mut *server, level: &mut *level };
            send_level_info(&host, &info, &handle);
        }
        new.send(&abilities_packet(&new));
        server.player_list.add_player(handle.clone());
        level.add_player(Box::new(new));
        {
            let mut host = ServerJoinHost { server: &mut *server, level: &mut *level };
            super::join::send_player_permission_level(&mut host, &handle);
        }
        level.with_player(new_id, |level, player| {
            player.init_inventory_menu(level);
            after(server, player);
        });
    })?;
    // the respawn anchor that was used up plays its depletion sound for the player
    if !keep_all_player_data
        && let Some(config) = old_respawn_config
    {
        let pos = config.respawn_data.pos;
        let sound = server.with_level(&config.respawn_data.dimension, |_, level| {
            level.get_block_state(pos).block() == mc_data::Block::RESPAWN_ANCHOR
        });
        if sound == Some(true) {
            let seed = server.with_level(&dimension, |_, level| level.random().next_long());
            if let Some(seed) = seed {
                handle.connection.send(&ClientboundSoundPacket::new(
                    SoundEventHolder::Reference(mc_data::SoundEvent::BLOCK_RESPAWN_ANCHOR_DEPLETE),
                    SoundSource::Blocks,
                    f64::from(pos.x()),
                    f64::from(pos.y()),
                    f64::from(pos.z()),
                    1.0,
                    1.0,
                    seed,
                ));
            }
        }
    }
    Some(new_id)
}

/// Queues [`respawn`] for after the current packet/tick handling (the player is taken out of its level while it runs);
/// `after` receives the new player (`resetPosition`, `restartClientLoadTimerAfterRespawn`, ...).
pub fn queue_respawn(
    server: &MinecraftServer,
    old_dimension: DimensionKey,
    id: EntityId,
    keep_all_player_data: bool,
    reason: RemovalReason,
    after: impl FnOnce(&mut MinecraftServer, &mut ServerPlayer) + Send + 'static,
) {
    server.shared.execute(move |server| {
        respawn(server, &old_dimension, id, keep_all_player_data, reason, after);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use mc_core::ident::Identifier;
    use mc_data::enums::GameType;
    use mc_protocol::codec::GameProfile;

    fn player() -> ServerPlayer {
        let profile = GameProfile { id: uuid::Uuid::from_u128(5), name: "Bot".into(), properties: vec![] };
        ServerPlayer::new(3, Identifier::parse("minecraft:overworld").unwrap(), profile, Default::default())
    }

    fn dead_player() -> ServerPlayer {
        let mut old = player();
        old.player.living.set_health(3.0);
        old.player.experience_level = 5;
        old.player.total_experience = 55;
        old.set_game_mode_for_player(GameType::Creative, Some(GameType::Survival));
        old
    }

    #[test]
    fn death_resets_health_and_xp_unless_keep_inventory() {
        let mut new = player();
        restore_from(&mut new, dead_player(), false, false);
        assert_eq!(new.player.living.get_health(), new.player.living.get_max_health());
        assert_eq!((new.player.experience_level, new.player.total_experience), (0, 0));
        // game mode always carries over
        assert_eq!(new.game_mode.game_mode_for_player, GameType::Creative);
        assert_eq!(new.game_mode.previous_game_mode_for_player, Some(GameType::Survival));

        let mut new = player();
        restore_from(&mut new, dead_player(), false, true);
        assert_eq!((new.player.experience_level, new.player.total_experience), (5, 55));
        assert_eq!(new.player.living.get_health(), new.player.living.get_max_health());
    }

    #[test]
    fn leaving_the_end_keeps_everything() {
        let mut new = player();
        restore_from(&mut new, dead_player(), true, false);
        assert_eq!(new.player.living.get_health(), 3.0);
        assert_eq!((new.player.experience_level, new.player.total_experience), (5, 55));
        assert_eq!(new.last_sent_health, -1.0);
        assert_eq!(new.last_sent_exp, -1);
    }
}
