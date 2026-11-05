//! The player part of the play-packet dispatcher's `PlayerSystems` (vanilla's `this.player.xxx(...)` calls of
//! `ServerGamePacketListenerImpl`). Exactly one implementor is used (`PLAYER_SYSTEMS.first()`), so every slice that owns
//! one of these methods adds its delegation here.

use crate::server::network::game_packets::PlayCtx;
use crate::server::network::game_packets::hooks::{PLAYER_SYSTEMS, PlayerSystems};
use linkme::distributed_slice;

struct ServerPlayerSystems;

impl PlayerSystems for ServerPlayerSystems {
    /// `ServerPlayer.jumpFromGround()` (jump power, sprint boost, `Stats.JUMP`, food exhaustion).
    fn jump_from_ground(&self, ctx: &mut PlayCtx) {
        crate::world::entity::Entity::jump_from_ground(&mut *ctx.player, &*ctx.level);
    }

    /// `LivingEntity.onClimbable()` of the player (records `lastClimbablePos`).
    fn on_climbable(&self, ctx: &mut PlayCtx) -> bool {
        crate::world::entity::living_entity::travel::climbable(&mut *ctx.player, &*ctx.level)
    }

    /// `Player.tryToStartFallFlying()`.
    fn try_to_start_fall_flying(&self, ctx: &mut PlayCtx) -> bool {
        use crate::world::entity::Entity;
        let player = &mut *ctx.player;
        if !player.player.living.is_fall_flying() && player.can_glide() && !player.is_in_liquid() {
            // `startFallFlying()`
            player.base_mut().set_shared_flag(crate::world::entity::entity::FLAG_FALL_FLYING, true);
            true
        } else {
            false
        }
    }

    /// `LivingEntity.stopFallFlying()`.
    fn stop_fall_flying(&self, ctx: &mut PlayCtx) {
        crate::world::entity::living_entity::fall_flying::stop_fall_flying(&mut *ctx.player);
    }

    /// `ServerPlayer.sendBuildLimitMessage(isTooHigh, limit)`.
    fn send_build_limit_message(&self, ctx: &mut PlayCtx, upper: bool, limit: i32) {
        ctx.player.send_build_limit_message(upper, limit);
    }

    /// `ServerPlayer.sendSpawnProtectionMessage(pos)`.
    fn send_spawn_protection_message(&self, ctx: &mut PlayCtx, pos: mc_core::pos::BlockPos) {
        ctx.player.send_spawn_protection_message(pos);
    }

    /// `ServerPlayer.setCamera(target)`.
    fn set_camera(&self, ctx: &mut PlayCtx, target: crate::world::entity::EntityId) {
        ctx.player.set_camera(ctx.level, Some(target));
    }

    /// `ServerPlayer.teleportTo(level, x, y, z, Set.of(), yRot, xRot, resetCamera)` (spectator
    /// `ServerboundTeleportToEntityPacket`). Another dimension is a `teleport(TeleportTransition)`, which needs the
    /// player in its level: it runs as a deferred server task once the player is put back.
    fn teleport_to(&self, ctx: &mut PlayCtx, dimension: &crate::world::level::DimensionKey, x: f64, y: f64, z: f64, y_rot: f32, x_rot: f32, set_camera: bool) {
        if set_camera {
            ctx.player.set_camera(ctx.level, None);
        }
        if *dimension != ctx.level.dimension {
            let (old, id, target) = (ctx.level.dimension.clone(), ctx.player.base_id(), dimension.clone());
            ctx.lis().deferred_tasks.push(Box::new(move |server| {
                let transition = crate::server::players::dimension_change::TeleportTransition::new(target, mc_core::phys::Vec3::new(x, y, z), mc_core::phys::Vec3::ZERO, y_rot, x_rot);
                crate::server::players::dimension_change::teleport_player(server, &old, id, &transition);
            }));
            return;
        }
        ctx.player.connection_teleport(x, y, z, y_rot, x_rot);
        crate::world::entity::Entity::set_y_head_rot(&mut *ctx.player, y_rot);
        if let Some(l) = ctx.player.listener.as_mut() {
            l.reset_flying_ticks();
        }
    }

    /// `ServerPlayer.interactOn(target, hand, location)`.
    fn interact_on(
        &self,
        ctx: &mut PlayCtx,
        target: crate::world::entity::EntityId,
        hand: crate::world::item::InteractionHand,
        location: mc_core::phys::Vec3,
    ) -> crate::world::item::InteractionResult {
        ctx.player.interact_on(ctx.level, target, hand, location)
    }

    /// `ServerPlayer.stopSleepInBed(forcefulWakeUp, updateLevelList)`.
    fn stop_sleep_in_bed(&self, ctx: &mut PlayCtx, forceful_wake_up: bool, update_level_list: bool) {
        ctx.player.stop_sleep_in_bed_impl(ctx.level, forceful_wake_up, update_level_list);
    }

    /// `getStats().sendStats(player)`.
    fn send_stats(&self, ctx: &mut PlayCtx) {
        let packet = ctx.player.stats.lock().stats_packet();
        ctx.send(&packet);
    }

    /// `ServerPlayer.checkMovementStatistics(dx, dy, dz)`.
    fn check_movement_statistics(&self, ctx: &mut PlayCtx, dx: f64, dy: f64, dz: f64) {
        ctx.player.check_movement_statistics(ctx.level, dx, dy, dz);
    }
}

#[distributed_slice(PLAYER_SYSTEMS)]
static SERVER_PLAYER_SYSTEMS: &dyn PlayerSystems = &ServerPlayerSystems;
