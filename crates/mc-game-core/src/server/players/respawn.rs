//! `PlayerList.respawn(player, keepAllPlayerData, reason)` with `ServerPlayer.findRespawnPositionAndUseSpawnBlock`,
//! `restoreFrom` and the packet sequence of a respawn. The player must be in its level (not taken out): callers that
//! are inside the player's own packet handling queue it with [`queue_respawn`], which runs it right after.

use super::join::{
    create_common_spawn_info, send_active_player_effects, send_level_info, send_post_effects,
};
use super::server_join_host::{ServerJoinHost, level_join_info};
use super::teleport_transition::{PostTeleportTransition, TeleportTransition};
use crate::server::MinecraftServer;
use crate::server::level::ServerLevel;
use crate::server::level::player_spawn_finder::{
    FindSpawn, PlayerSpawnFinder, Step, fixup_spawn_height,
};
use crate::world::entity::entity::Entity;
use crate::world::entity::respawn::{RespawnConfig, RespawnPosAngle};
use crate::world::entity::{EntityId, RemovalReason, ServerPlayer};
use crate::world::level::block::behaviour;
use crate::world::level::gamerules::GameRules;
use crate::world::level::{BlockGetter, DimensionKey, LevelAccessor};
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_protocol::packets::play::{
    ClientboundChangeDifficultyPacket, ClientboundGameEventPacket, ClientboundRespawnPacket,
    ClientboundSetDefaultSpawnPositionPacket, ClientboundSetExperiencePacket,
    ClientboundSoundPacket, SoundEventHolder, SoundSource,
};

/// `PlayerSpawnFinder.findSpawn` + `managedBlock`: the chunks are loaded synchronously.
pub(crate) fn adjust_spawn_location(
    server: &mut MinecraftServer,
    level: &mut ServerLevel,
    suggestion: BlockPos,
) -> Vec3 {
    let adventure =
        server.world_data.lock().settings.game_type == mc_world::level_storage::GameType::Adventure;
    let radius = server.game_rules_ref().get(GameRules::RESPAWN_RADIUS);
    find_spawn_position(level, adventure, radius, suggestion)
}

/// The level half of `adjust_spawn_location` (`adventure` = the world's default game type, `radius` = the
/// `respawnRadius` rule).
pub fn find_spawn_position(
    level: &mut ServerLevel,
    adventure: bool,
    radius: i32,
    suggestion: BlockPos,
) -> Vec3 {
    let border = mc_core::math::mth::floor_f64(
        level
            .get_world_border()
            .get_distance_to_border(f64::from(suggestion.x()), f64::from(suggestion.z())),
    );
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
fn find_respawn_and_use_spawn_block(
    level: &mut ServerLevel,
    config: &RespawnConfig,
    consume_spawn_block: bool,
) -> Option<RespawnPosAngle> {
    let data = &config.respawn_data;
    let (pos, yaw, pitch, forced) = (data.pos, data.yaw, data.pitch, config.forced);
    let state = level.get_block_state(pos);
    let block = state.block();
    if let Some(stand_up) =
        behaviour(block).find_respawn_position(level, state, pos, forced, consume_spawn_block, yaw)
    {
        return stand_up.map(|p| RespawnPosAngle::of(p, pos, 0.0));
    }
    if !forced {
        return None;
    }
    let free_bottom = behaviour(block).is_possible_to_respawn_in_this(state);
    let top = level.get_block_state(pos.above());
    let free_top = behaviour(top.block()).is_possible_to_respawn_in_this(top);
    (free_bottom && free_top).then(|| RespawnPosAngle {
        position: Vec3::new(
            f64::from(pos.x()) + 0.5,
            f64::from(pos.y()) + 0.1,
            f64::from(pos.z()) + 0.5,
        ),
        yaw,
        pitch,
    })
}

/// `ServerPlayer.findRespawnPositionAndUseSpawnBlock(consume, post)`; `level` is the player's current level (taken out of
/// `server`), `config` the player's `getRespawnConfig()`.
pub fn find_respawn_position_and_use_spawn_block(
    server: &mut MinecraftServer,
    level: &mut ServerLevel,
    config: Option<&RespawnConfig>,
    consume: bool,
    post: PostTeleportTransition,
) -> TeleportTransition {
    let respawn_dimension = RespawnConfig::dimension_or_default(config);
    let found = match config {
        Some(config) => {
            if respawn_dimension == level.dimension {
                Some(find_respawn_and_use_spawn_block(level, config, consume))
            } else {
                server
                    .level_mut(&respawn_dimension)
                    .map(|l| find_respawn_and_use_spawn_block(l, config, consume))
            }
        }
        None => None,
    };
    match found {
        Some(Some(angle)) => TeleportTransition::new(
            respawn_dimension,
            angle.position,
            Vec3::ZERO,
            angle.yaw,
            angle.pitch,
            post,
        ),
        Some(None) => TeleportTransition::missing_respawn_block(server, level, post),
        None => TeleportTransition::create_default(server, level, post),
    }
}

/// `TeleportTransition.findAdjustedSharedSpawnPos(newLevel, player)`: `level` is the player's current level (out of
/// `server`), `dimension` the destination.
pub(crate) fn find_adjusted_shared_spawn_pos(
    server: &mut MinecraftServer,
    level: &mut ServerLevel,
    dimension: &DimensionKey,
    pos: BlockPos,
) -> Vec3 {
    if *dimension == level.dimension {
        adjust_spawn_location(server, level, pos)
    } else {
        match server.with_level(dimension, |server, other| {
            adjust_spawn_location(server, other, pos)
        }) {
            Some(position) => position,
            None => Vec3::new(
                f64::from(pos.x()) + 0.5,
                f64::from(pos.y()),
                f64::from(pos.z()) + 0.5,
            ),
        }
    }
}

/// `restoreFrom(oldPlayer, restoreAll)` + the field copies of `PlayerList.respawn` (`oldPlayer` is consumed).
/// Returns the old player's active effects (copies) that `restoreFrom` re-adds with `addEffect`, which needs the new
/// level: the caller applies them right after this call.
fn restore_from(
    new: &mut ServerPlayer,
    mut old: ServerPlayer,
    restore_all: bool,
    keep_inventory: bool,
) -> Vec<crate::world::entity::effect::MobEffectInstance> {
    new.warden_spawn_tracker = old.warden_spawn_tracker;
    new.set_game_mode_for_player(
        old.game_mode.game_mode_for_player,
        old.game_mode.previous_game_mode_for_player,
    );
    new.on_update_abilities();
    new.player
        .living
        .attributes
        .assign_base_values(&old.player.living.attributes);
    let mut effects = Vec::new();
    let transfer_inventory = |new: &mut ServerPlayer, old: &mut ServerPlayer| {
        new.player.inventory = std::mem::take(&mut old.player.inventory);
        new.player.experience_level = old.player.experience_level;
        new.player.total_experience = old.player.total_experience;
        new.player.experience_progress = old.player.experience_progress;
        let score = old.player.get_score();
        new.player.set_score(score);
    };
    if restore_all {
        let health = old.player.living.get_health();
        new.player.living.set_health(health);
        new.player.food_data = std::mem::take(&mut old.player.food_data);
        new.player
            .living
            .attributes
            .assign_permanent_modifiers(&old.player.living.attributes);
        effects = old
            .player
            .living
            .active_effects
            .values()
            .map(|e| e.copy())
            .collect();
        new.base_mut().portal_process = old.base().portal_process.clone();
        transfer_inventory(new, &mut old);
    } else {
        let max = new.player.living.get_max_health();
        new.player.living.set_health(max);
        if keep_inventory || crate::world::entity::Entity::is_spectator(&old) {
            transfer_inventory(new, &mut old);
        }
    }
    new.player.enchantment_seed = old.player.enchantment_seed;
    new.player.ender_chest = std::mem::take(&mut old.player.ender_chest);
    let customisation = old
        .player
        .living
        .entity
        .entity_data
        .get(crate::world::entity::player::DATA_PLAYER_MODE_CUSTOMISATION);
    new.player.living.entity.entity_data.set(
        crate::world::entity::player::DATA_PLAYER_MODE_CUSTOMISATION,
        customisation,
    );
    new.last_sent_exp = -1;
    new.last_sent_health = -1.0;
    new.last_sent_food = -1;
    new.recipe_book.copy_over_data(&old.recipe_book);
    new.seen_credits = old.seen_credits;
    new.entered_nether_position = old.entered_nether_position;
    new.current_explosion_impact_pos = old.current_explosion_impact_pos;
    new.current_explosion_cause = old.current_explosion_cause;
    new.shoulder_entity_left = std::mem::take(&mut old.shoulder_entity_left);
    new.shoulder_entity_right = std::mem::take(&mut old.shoulder_entity_right);
    new.player.last_death_location = old.player.last_death_location.take();
    effects
}

/// `PlayerList.respawn` step 1: the old player's state is taken out of its level and the entity is removed
/// (`removePlayerImmediately`); `level` keeps a removed placeholder until the entity manager drops it.
pub fn take_old_player(
    level: &mut ServerLevel,
    id: EntityId,
    reason: RemovalReason,
) -> Option<ServerPlayer> {
    level.with_player(id, |level, player| {
        let mut dummy = ServerPlayer::new(
            id,
            player.dimension.clone(),
            player.player.game_profile.clone(),
            Default::default(),
        );
        dummy.remove(reason);
        let mut old = std::mem::replace(player, dummy);
        // `Player.remove`: the player is removed before its menus are closed, so a removed player drops the cursor stack
        if matches!(reason, RemovalReason::Killed | RemovalReason::Discarded) {
            crate::world::entity::living_entity::effects::trigger_on_death_mob_effects(
                &mut old, level, reason,
            );
        }
        old.remove(reason);
        old.close_menus_on_removal(level);
        old
    })
}

/// `PlayerList.respawn` step 2: the fresh `ServerPlayer` (same entity id and connection) with `restoreFrom` applied,
/// the restored effects added in the new `level` and the player placed at the transition's position.
pub fn create_respawned_player(
    old: ServerPlayer,
    level: &mut ServerLevel,
    transition: &TeleportTransition,
    keep_all_player_data: bool,
    keep_inventory: bool,
) -> ServerPlayer {
    let mut old = old;
    let client_information = old.client_information.clone();
    let mut new = ServerPlayer::new(
        old.base_id(),
        transition.level.clone(),
        old.player.game_profile.clone(),
        client_information,
    );
    new.connection = old.connection.take();
    new.listener = old.listener.take();
    new.stats = old.stats.clone();
    new.advancements = old.advancements.clone();
    new.handle = old.handle.clone();
    new.player.set_main_arm(old.player.get_main_arm());
    if !transition.missing_respawn_block {
        new.respawn_config = old.respawn_config.clone();
    }
    for tag in old.base().tags.iter_java_order() {
        new.base_mut().add_tag(tag);
    }
    new.player.post_effects = old.player.post_effects.clone();
    let restored_effects = restore_from(&mut new, old, keep_all_player_data, keep_inventory);
    // `restoreFrom`: `addEffect(new MobEffectInstance(effect))` in the new level (sends the effect packets before the respawn packet)
    for effect in restored_effects {
        crate::world::entity::Entity::add_effect(&mut new, level, effect, None);
    }
    new.snap_to(
        transition.position.x,
        transition.position.y,
        transition.position.z,
        transition.y_rot,
        transition.x_rot,
    );
    new.dimension = transition.level.clone();
    new.sync_handle();
    if transition.missing_respawn_block {
        new.send(&ClientboundGameEventPacket {
            event: ClientboundGameEventPacket::NO_RESPAWN_BLOCK_AVAILABLE,
            param: 0.0,
        });
    }
    new
}

/// `PlayerList.respawn`: replaces the player `id` of `old_dimension` by a fresh `ServerPlayer` placed at its respawn
/// position; `after` runs on the new player (the listener's `resetPosition()` etc.). Returns the new player's id.
pub fn respawn(
    server: &mut MinecraftServer,
    old_dimension: &DimensionKey,
    id: EntityId,
    keep_all_player_data: bool,
    reason: RemovalReason,
    after: impl FnOnce(&mut MinecraftServer, &mut ServerLevel, &mut ServerPlayer),
) -> Option<EntityId> {
    // 1. take the old player's state out of its level and remove it (`removePlayerImmediately`)
    let keep_inventory = server.game_rules_ref().get(GameRules::KEEP_INVENTORY);
    let detached = server
        .detached_players
        .iter()
        .find(|(_, p)| p.base_id() == id)
        .map(|(uuid, _)| *uuid)
        .and_then(|uuid| server.detached_players.remove(&uuid));
    let (old, transition) = if let Some(old) = detached {
        // `showEndCredits` already removed the player from its level (`removePlayerImmediately` again is a no-op)
        let config = old.respawn_config.clone();
        let transition = server.with_level(old_dimension, |server, level| {
            find_respawn_position_and_use_spawn_block(
                server,
                level,
                config.as_ref(),
                !keep_all_player_data,
                PostTeleportTransition::DO_NOTHING,
            )
        })?;
        (*old, transition)
    } else {
        server
            .with_level(old_dimension, |server, level| {
                let config = level
                    .get_entity(id)
                    .or_else(|| level.get_dead_player(id))
                    .and_then(|e| e.as_any().downcast_ref::<ServerPlayer>())
                    .and_then(|p| p.respawn_config.clone());
                let transition = find_respawn_position_and_use_spawn_block(
                    server,
                    level,
                    config.as_ref(),
                    !keep_all_player_data,
                    PostTeleportTransition::DO_NOTHING,
                );
                let old = take_old_player(level, id, reason)?;
                Some((old, transition))
            })
            .flatten()?
    };
    let handle = old.handle.clone()?;
    server.player_list.remove_from_players(&handle);

    // 2. the new player
    let old_respawn_config = old.respawn_config.clone();
    let mut new = server.with_level(&transition.level, |_, level| {
        create_respawned_player(
            old,
            level,
            &transition,
            keep_all_player_data,
            keep_inventory,
        )
    })?;

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
        let (y_rot, x_rot) = (
            new.player.living.entity.y_rot(),
            new.player.living.entity.x_rot(),
        );
        new.connection_teleport(position.x, position.y, position.z, y_rot, x_rot);
        new.send(&ClientboundSetDefaultSpawnPositionPacket {
            respawn_data: info.respawn_data.clone(),
        });
        new.send(&ClientboundChangeDifficultyPacket {
            difficulty: info.difficulty,
            locked: info.difficulty_locked,
        });
        new.send(&ClientboundSetExperiencePacket {
            experience_progress: new.player.experience_progress,
            experience_level: new.player.experience_level,
            total_experience: new.player.total_experience,
        });
        send_active_player_effects(&new);
        send_post_effects(&mut new);
        {
            let host = ServerJoinHost {
                server: &mut *server,
                level: &mut *level,
            };
            send_level_info(&host, &info, &handle);
        }
        server.player_list.add_player(handle.clone());
        level.add_player(Box::new(new));
        {
            let mut host = ServerJoinHost {
                server: &mut *server,
                level: &mut *level,
            };
            super::join::send_player_permission_level(&mut host, &handle);
        }
        level.with_player(new_id, |level, player| {
            player.init_inventory_menu(level);
            after(server, level, player);
        });
    })?;
    // the respawn anchor that was used up plays its depletion sound for the player
    if !keep_all_player_data && let Some(config) = old_respawn_config {
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
    after: impl FnOnce(&mut MinecraftServer, &mut ServerLevel, &mut ServerPlayer) + Send + 'static,
) {
    server.shared.execute(move |server| {
        respawn(
            server,
            &old_dimension,
            id,
            keep_all_player_data,
            reason,
            after,
        );
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use mc_core::ident::Identifier;
    use mc_data::enums::GameType;
    use mc_protocol::codec::GameProfile;

    fn player() -> ServerPlayer {
        let profile = GameProfile {
            id: uuid::Uuid::from_u128(5),
            name: "Bot".into(),
            properties: vec![],
        };
        ServerPlayer::new(
            3,
            Identifier::parse("minecraft:overworld").unwrap(),
            profile,
            Default::default(),
        )
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
        assert_eq!(
            new.player.living.get_health(),
            new.player.living.get_max_health()
        );
        assert_eq!(
            (new.player.experience_level, new.player.total_experience),
            (0, 0)
        );
        // game mode always carries over
        assert_eq!(new.game_mode.game_mode_for_player, GameType::Creative);
        assert_eq!(
            new.game_mode.previous_game_mode_for_player,
            Some(GameType::Survival)
        );

        let mut new = player();
        restore_from(&mut new, dead_player(), false, true);
        assert_eq!(
            (new.player.experience_level, new.player.total_experience),
            (5, 55)
        );
        assert_eq!(
            new.player.living.get_health(),
            new.player.living.get_max_health()
        );
    }

    #[test]
    fn leaving_the_end_keeps_everything() {
        let mut new = player();
        restore_from(&mut new, dead_player(), true, false);
        assert_eq!(new.player.living.get_health(), 3.0);
        assert_eq!(
            (new.player.experience_level, new.player.total_experience),
            (5, 55)
        );
        assert_eq!(new.last_sent_health, -1.0);
        assert_eq!(new.last_sent_exp, -1);
    }
}
